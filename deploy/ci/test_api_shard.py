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
        # Two crates; tests/common's own test is compiled into both.
        self.listings = {
            "api_a": sorted([*shard.PRECHECKS, *(f"feature::test_{n}" for n in range(20)), "common::shared"]),
            "api_b": sorted([*(f"feature::test_{n}" for n in range(20, 40)), "common::shared"]),
        }
        self.crates = patch.object(shard, "crates", return_value=sorted(self.listings))
        self.crates.start()
        self.addCleanup(self.crates.stop)
        self.env = {
            "DATABASE_URL": "postgres://postgres:ci-only@postgres:5432/postgres",
            "CARGO_TARGET_DIR": "/ci-target",
            "SQLX_OFFLINE": "true",
        }

    def cargo(self, *results):
        """subprocess.run stand-in: each crate's --list output, then the given results in order."""
        later = list(results)

        def run(command, **kwargs):
            if "--list" in command:
                return SimpleNamespace(stdout=listing(self.listings[command[3]]))
            if later:
                result = later.pop(0)
                if isinstance(result, Exception):
                    raise result
            return SimpleNamespace(stdout="")
        return run

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
            listing(self.names + [self.names[-1]]),
            self.output.replace("42 tests, 0 benchmarks", ""),
        ]
        for output in cases:
            with self.subTest(output=output), self.assertRaises(ValueError):
                shard.inventory(output)

    def test_owners_run_each_test_once_and_shared_common_tests_in_the_first_crate(self):
        owner = shard.owners(self.listings)
        self.assertEqual(set(owner), set(self.names) | {"common::shared"})
        self.assertEqual(owner["common::shared"], "api_a")
        self.assertEqual(owner["feature::test_30"], "api_b")
        with self.assertRaisesRegex(ValueError, "two crates"):
            shard.owners({**self.listings, "api_c": ["feature::test_3"]})
        missing = {crate: [n for n in names if n != shard.PRECHECKS[1]] for crate, names in self.listings.items()}
        with self.assertRaisesRegex(ValueError, "empty-database"):
            shard.owners(missing)

    def test_crates_are_the_api_test_roots(self):
        self.crates.stop()
        try:
            found = shard.crates()
        finally:
            self.crates.start()
        self.assertGreater(len(found), 1)
        self.assertEqual(found, sorted(found))
        self.assertTrue(all(name.startswith("api_") for name in found))

    def test_invalid_indices_and_empty_shard_fail_before_migrations(self):
        for index, count in ((-1, 2), (2, 2), (0, 0), (0, -1)):
            with self.subTest(index=index, count=count), patch.object(shard.subprocess, "run") as command:
                with self.assertRaises(ValueError):
                    shard.run(index, count, self.env)
                command.assert_not_called()
        self.listings = {"api_a": list(shard.PRECHECKS), "api_b": ["common::shared"]}
        self.listings["api_a"].append("common::shared")
        with patch.object(shard.subprocess, "run", side_effect=self.cargo()) as command:
            shard.run(0, 1, self.env)  # common::shared alone is a valid selection
        self.listings["api_b"] = list(shard.PRECHECKS)
        self.listings["api_a"] = list(shard.PRECHECKS)
        with patch.object(shard.subprocess, "run", side_effect=self.cargo()) as command:
            with self.assertRaisesRegex(ValueError, "two crates"):
                shard.run(0, 1, self.env)
        self.listings = {"api_a": list(shard.PRECHECKS)}
        with patch.object(shard, "crates", return_value=["api_a"]), \
                patch.object(shard.subprocess, "run", side_effect=self.cargo()) as command:
            with self.assertRaisesRegex(ValueError, "selects no tests"):
                shard.run(0, 1, self.env)
            self.assertEqual(command.call_count, 1)

    def test_commands_keep_empty_database_checks_before_template_and_shard(self):
        with patch.object(shard.subprocess, "run", side_effect=self.cargo()) as command:
            shard.run(0, 2, self.env)
        calls = command.call_args_list
        owner = shard.owners(self.listings)
        selected = shard.select(owner, 0, 2)
        self.assertEqual([c.args[0] for c in calls[:2]], [
            shard.cargo(crate) + ["--", "--list", "--color", "never"] for crate in ("api_a", "api_b")
        ])
        self.assertEqual(calls[2].args[0], shard.cargo("api_a") + ["--", "--exact", *shard.PRECHECKS])
        self.assertEqual(calls[3].args[0], ["/ci-target/debug/verzo-360-api", "migrate"])
        self.assertEqual(calls[3].kwargs["env"], {**self.env, "DATABASE_URL": "postgres://postgres:ci-only@postgres:5432/template1"})
        ran = []
        self.assertEqual(len(calls), 6)
        for call, crate in zip(calls[4:], ("api_a", "api_b")):
            self.assertEqual(call.args[0][:4], shard.cargo(crate))
            self.assertEqual(call.args[0][4:6], ["--", "--exact"])
            ran += [(crate, name) for name in call.args[0][6:]]
        self.assertEqual(sorted(ran), sorted((owner[name], name) for name in selected))
        self.assertEqual(self.env["DATABASE_URL"].rsplit("/", 1)[-1], "postgres")
        for call in calls:
            self.assertTrue(call.kwargs["check"])
            self.assertNotIn("--include-ignored", call.args[0])
        for index in (0, 1, 2, 4, 5):
            self.assertEqual(calls[index].kwargs["env"], self.env)

    def test_every_test_runs_exactly_once_across_shards(self):
        ran = []
        for index in range(3):
            with patch.object(shard.subprocess, "run", side_effect=self.cargo()) as command:
                shard.run(index, 3, self.env)
            ran += [n for c in command.call_args_list if c.args[0][4:6] == ["--", "--exact"] for n in c.args[0][6:]
                    if n not in shard.PRECHECKS]
        self.assertEqual(sorted(ran), sorted(set(self.names) - set(shard.PRECHECKS) | {"common::shared"}))

    def test_invalid_inventory_never_runs_tests_or_migrations(self):
        self.listings["api_a"] = self.listings["api_a"][1:]
        with patch.object(shard.subprocess, "run", side_effect=self.cargo()) as command:
            with self.assertRaises(ValueError):
                shard.run(0, 2, self.env)
            self.assertEqual(command.call_count, 2)

    def test_subprocess_failure_stops_later_commands(self):
        for position in range(4):
            error = subprocess.CalledProcessError(7, ["failing-command"])
            with self.subTest(position=position), \
                    patch.object(shard.subprocess, "run", side_effect=self.cargo(*[None] * position, error)) as command:
                with self.assertRaises(subprocess.CalledProcessError) as caught:
                    shard.run(0, 2, self.env)
                self.assertEqual(caught.exception.returncode, 7)
                self.assertEqual(command.call_count, 2 + position + 1)

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
            with self.subTest(host=host), patch.object(shard.subprocess, "run", side_effect=self.cargo()) as command:
                shard.run(0, 2, {**self.env, "DATABASE_URL": database})
                self.assertEqual(command.call_args_list[3].kwargs["env"]["DATABASE_URL"], database.rsplit("/", 1)[0] + "/template1")


if __name__ == "__main__":
    unittest.main()
