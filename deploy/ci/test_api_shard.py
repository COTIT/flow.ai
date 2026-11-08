import importlib.util
from pathlib import Path
import subprocess
from types import SimpleNamespace
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("api_shard", Path(__file__).with_name("api-shard.py"))
shard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shard)


def listing(names):
    return "\n".join([*(f"{name}: test" for name in names), "", f"{len(names)} tests, 0 benchmarks", ""])


class ApiShardTests(unittest.TestCase):
    def setUp(self):
        self.names = [*shard.PRECHECKS, *(f"feature::test_{n}" for n in range(40))]
        self.output = listing(self.names)
        self.env = {
            "DATABASE_URL": "postgres://postgres:ci-only@postgres:5432/postgres",
            "CARGO_TARGET_DIR": "/ci-target",
            "SQLX_OFFLINE": "true",
        }

    def test_partition_is_disjoint_complete_and_stable(self):
        expected = set(self.names) - set(shard.PRECHECKS)
        for count in (1, 2, 3):
            parts = [shard.select(self.names, index, count) for index in range(count)]
            self.assertEqual(set.union(*(set(part) for part in parts)), expected)
            self.assertEqual(sum(map(len, parts)), len(expected))
            for index, part in enumerate(parts):
                self.assertEqual(part, shard.select(list(reversed(self.names)), index, count))
        extra = self.names + ["new_feature::new_test"]
        assigned = [shard.select(extra, index, 2) for index in range(2)]
        self.assertEqual(sum("new_feature::new_test" in part for part in assigned), 1)

    def test_inventory_preserves_full_names_and_precheck_prefixes(self):
        similar = shard.PRECHECKS[0] + "_regression"
        names = [*self.names, similar]
        parsed = shard.inventory(listing(names))
        self.assertEqual(parsed, sorted(names))
        self.assertIn(similar, shard.select(parsed, 0, 1))
        self.assertFalse(set(shard.PRECHECKS) & set(shard.select(parsed, 0, 1)))

    def test_inventory_rejects_incomplete_or_unknown_output(self):
        cases = [
            "", "0 tests, 0 benchmarks\n", self.output.replace("42 tests", "43 tests"),
            self.output.replace("0 benchmarks", "1 benchmark"),
            self.output + "unexpected line\n", self.output + "42 tests, 0 benchmarks\n",
            self.output + "late::test: test\n", self.output.replace("feature::test_0: test", "feature::test_0: benchmark"),
            self.output.replace("feature::test_0: test", "--list: test"),
            self.output.replace("feature::test_0: test", "feature::bad name: test"),
            listing(self.names + [self.names[-1]]), listing(self.names[1:]),
            listing([name for name in self.names if name != shard.PRECHECKS[1]]),
            self.output.replace("42 tests, 0 benchmarks", ""),
        ]
        for output in cases:
            with self.subTest(output=output), self.assertRaises(ValueError):
                shard.inventory(output)

    def test_invalid_indices_and_empty_shard_fail_before_migrations(self):
        for index, count in ((-1, 2), (2, 2), (0, 0), (0, -1)):
            with self.subTest(index=index, count=count), patch.object(shard.subprocess, "run") as command:
                with self.assertRaises(ValueError):
                    shard.run(index, count, self.env)
                command.assert_not_called()
        with patch.object(shard.subprocess, "run", return_value=SimpleNamespace(stdout=listing(shard.PRECHECKS))) as command:
            with self.assertRaisesRegex(ValueError, "selects no tests"):
                shard.run(0, 1, self.env)
            self.assertEqual(command.call_count, 1)

    def test_commands_keep_empty_database_checks_before_template_and_shard(self):
        with patch.object(shard.subprocess, "run", return_value=SimpleNamespace(stdout=self.output)) as command:
            shard.run(0, 2, self.env)
        calls = command.call_args_list
        self.assertEqual(calls[0].args[0], shard.CARGO + ["--", "--list", "--color", "never"])
        self.assertEqual(calls[1].args[0], shard.CARGO + ["--", "--exact", *shard.PRECHECKS])
        self.assertEqual(calls[2].args[0], ["/ci-target/debug/genesy-360-api", "migrate"])
        self.assertEqual(calls[2].kwargs["env"], {**self.env, "DATABASE_URL": "postgres://postgres:ci-only@postgres:5432/template1"})
        self.assertEqual(calls[3].args[0], shard.CARGO + ["--", "--exact", *shard.select(self.names, 0, 2)])
        self.assertEqual(self.env["DATABASE_URL"].rsplit("/", 1)[-1], "postgres")
        for call in calls:
            self.assertTrue(call.kwargs["check"])
            self.assertNotIn("--include-ignored", call.args[0])
        for index in (0, 1, 3):
            self.assertEqual(calls[index].kwargs["env"], self.env)

    def test_invalid_inventory_never_runs_tests_or_migrations(self):
        with patch.object(shard.subprocess, "run", return_value=SimpleNamespace(stdout=listing(self.names[1:]))) as command:
            with self.assertRaises(ValueError):
                shard.run(0, 2, self.env)
            self.assertEqual(command.call_count, 1)

    def test_subprocess_failure_stops_later_commands(self):
        for position in range(4):
            error = subprocess.CalledProcessError(7, ["failing-command"])
            results = [SimpleNamespace(stdout=self.output)] * position + [error]
            with self.subTest(position=position), patch.object(shard.subprocess, "run", side_effect=results) as command:
                with self.assertRaises(subprocess.CalledProcessError) as caught:
                    shard.run(0, 2, self.env)
                self.assertEqual(caught.exception.returncode, 7)
                self.assertEqual(command.call_count, position + 1)

    def test_database_guard_rejects_remote_production_and_ambiguous_urls(self):
        invalid = [
            "", "postgres://postgres@production.example/postgres", "postgres://genesy@postgres/postgres",
            "postgres://postgres@postgres/genesy", "postgres://postgres@postgres/template1",
            "postgres://postgres@postgres/postgres?host=production.example", "postgres://postgres@postgres/postgres#fragment",
            "https://postgres@postgres/postgres", "postgres://postgres@postgres:0/postgres",
            "postgres://postgres@postgres:invalid/postgres", "postgres://postgres@[::1/postgres",
            "postgres://postgres@postgres/postgres\n", "postgres://postgres@postgres/postgres\0",
        ]
        for database in invalid:
            with self.subTest(database=database), patch.object(shard.subprocess, "run") as command:
                with self.assertRaises(ValueError):
                    shard.run(0, 2, {**self.env, "DATABASE_URL": database})
                command.assert_not_called()
        with patch.object(shard.subprocess, "run") as command:
            with self.assertRaisesRegex(ValueError, "CARGO_TARGET_DIR"):
                shard.run(0, 2, {**self.env, "CARGO_TARGET_DIR": ""})
            command.assert_not_called()

    def test_loopback_database_urls_preserve_credentials_and_port(self):
        for host in ("localhost", "127.0.0.1", "[::1]"):
            database = f"postgresql://postgres:password%40escaped@{host}:54339/postgres"
            with self.subTest(host=host), patch.object(shard.subprocess, "run", return_value=SimpleNamespace(stdout=self.output)) as command:
                shard.run(0, 2, {**self.env, "DATABASE_URL": database})
                self.assertEqual(command.call_args_list[2].kwargs["env"]["DATABASE_URL"], database.rsplit("/", 1)[0] + "/template1")


if __name__ == "__main__":
    unittest.main()
