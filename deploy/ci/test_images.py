import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
from types import SimpleNamespace
import unittest
from unittest.mock import mock_open, patch
import zipfile


spec = importlib.util.spec_from_file_location("release_images", Path(__file__).with_name("images.py"))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)

SHA = "a" * 40
BASE = "b" * 40
DIGEST = "sha256:" + "c" * 64
ENV = {
    "GITHUB_SHA": SHA, "GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "2",
    "GITHUB_REPOSITORY": "COTIT/flow.ai", "GH_TOKEN": "test-token",
    "GITHUB_OUTPUT": "/unused/output", "GENESY_REF": "stale-shell-ref",
}


def expected(producer, ref=SHA):
    suffixes = {
        "api-production": [("api", "")],
        "api-staging": [("api", "-staging")],
        "web": [("office", ""), ("field", ""), ("shop", ""), ("office", "-staging"),
                ("till", ""), ("landing", ""), ("field", "-staging"), ("flow", "")],
    }
    return sorted(f"ghcr.io/cotit/genesy-{app}:{ref}{suffix}" for app, suffix in suffixes[producer])


def manifest(producer, attempt=2):
    return {
        "commit": SHA, "run_id": "123", "attempt": attempt, "producer": producer,
        "images": [{"destination": image, "source": image.rsplit(":", 1)[0] + "@" + DIGEST}
                   for image in expected(producer)],
    }


def artifact(producer, attempt=2, identity=None, **overrides):
    return {
        "id": identity or producer + "-" + str(attempt),
        "name": f"release-123-{producer}-{attempt}", "expired": False, "size_in_bytes": 512,
        "workflow_run": {"id": 123, "head_sha": SHA}, **overrides,
    }


def archive(files):
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as zipped:
        for name, content in files.items():
            zipped.writestr(name, content)
    return out.getvalue()


class ReleaseImagesTests(unittest.TestCase):
    def setUp(self):
        self.environment = patch.dict(os.environ, ENV, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def fake_collection(self, overrides=None, artifacts=None, bad_archives=None):
        self.commands = []
        artifacts = artifacts or [artifact(producer) for producer in release.LAYOUT]
        documents = {producer: manifest(producer) for producer in release.LAYOUT}
        documents.update(overrides or {})
        archives = {
            str(item["id"]): archive({producer + ".json": json.dumps(document)})
            for item in artifacts for producer, document in documents.items()
            if item["name"].startswith(f"release-123-{producer}-")
        }
        archives.update(bad_archives or {})

        def command(args, **kwargs):
            self.commands.append((args, kwargs))
            if args[:4] == ["gh", "api", "--paginate", "--slurp"]:
                return SimpleNamespace(stdout=json.dumps([{"artifacts": artifacts[:1]}, {"artifacts": artifacts[1:]}]))
            if args[:2] == ["gh", "api"]:
                identity = args[2].split("/")[-2]
                return SimpleNamespace(stdout=archives[identity])
            if args[:4] == ["docker", "buildx", "imagetools", "create"]:
                return SimpleNamespace(stdout="")
            raise AssertionError(f"unexpected command: {args}")

        return command

    def test_manifest_requires_exact_destinations_and_repository_digests(self):
        for producer in release.LAYOUT:
            good = manifest(producer)
            self.assertEqual(release.validate(good, producer, expected(producer), 2), good["images"])
            for source in ("ghcr.io/other/genesy-api@" + DIGEST,
                           "ghcr.io/cotit/genesy-other@" + DIGEST,
                           "ghcr.io/cotit/genesy-api:mutable",
                           "ghcr.io/cotit/genesy-api@sha256:bad"):
                bad = manifest(producer)
                bad["images"][0]["source"] = source
                with self.subTest(producer=producer, source=source), self.assertRaises(ValueError):
                    release.validate(bad, producer, expected(producer), 2)
        swapped = manifest("api-production")
        swapped["images"] = manifest("api-staging")["images"]
        with self.assertRaises(ValueError):
            release.validate(swapped, "api-production", expected("api-production"), 2)
        for entries in ([], manifest("web")["images"][:-1], manifest("web")["images"][:-1] + [manifest("web")["images"][0]]):
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                release.validate({**manifest("web"), "images": entries}, "web", expected("web"), 2)

    def test_manifest_identity_must_match_selected_attempt_and_current_run(self):
        for key, wrong in (("commit", BASE), ("run_id", "124"), ("attempt", 1), ("attempt", "2"), ("producer", "web")):
            bad = {**manifest("api-production"), key: wrong}
            with self.subTest(key=key, wrong=wrong), self.assertRaises(ValueError):
                release.validate(bad, "api-production", expected("api-production"), 2)
        older = manifest("api-production", attempt=1)
        self.assertEqual(release.validate(older, "api-production", expected("api-production"), 1), older["images"])

    def test_context_rejects_invalid_commit_run_or_attempt(self):
        for key, value in (("GITHUB_SHA", "a" * 39), ("GITHUB_SHA", "G" * 40),
                           ("GITHUB_RUN_ID", "0"), ("GITHUB_RUN_ID", "abc"),
                           ("GITHUB_RUN_ATTEMPT", "0"), ("GITHUB_RUN_ATTEMPT", "bad")):
            with self.subTest(key=key, value=value), patch.dict(os.environ, {key: value}), self.assertRaises(ValueError):
                release.context()

    def test_partial_rerun_uses_newest_available_attempt_per_producer(self):
        selected = release.select_artifacts([
            artifact("api-production", 1), artifact("api-staging", 1),
            artifact("web", 1), artifact("web", 2), artifact("web", 3),
            {"name": "sbom-web"}, {"name": "release-124-web-2"},
        ])
        self.assertEqual({producer: choice[0] for producer, choice in selected.items()},
                         {"api-production": 1, "api-staging": 1, "web": 2})

    def test_only_duplicates_at_highest_selected_attempt_are_rejected(self):
        baseline = [artifact("api-production"), artifact("api-staging")]
        for web in ([artifact("web", 1), artifact("web", 1), artifact("web", 2)],
                    [artifact("web", 2), artifact("web", 1), artifact("web", 1)]):
            with self.subTest(order=[a["name"] for a in web]):
                self.assertEqual(release.select_artifacts(baseline + web)["web"][0], 2)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            release.select_artifacts(baseline + [artifact("web"), artifact("web")])

    def test_invalid_newest_artifact_never_falls_back_to_older_valid_attempt(self):
        baseline = [artifact("api-production"), artifact("api-staging"), artifact("web", 1)]
        for invalid in (artifact("web", expired=True), artifact("web", size_in_bytes=1024 * 1024 + 1),
                        artifact("web", workflow_run={"id": 124, "head_sha": SHA}),
                        artifact("web", workflow_run={"id": 123, "head_sha": BASE}),
                        artifact("web", workflow_run={})):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                release.select_artifacts(baseline + [invalid])
        with self.assertRaisesRegex(ValueError, "missing"):
            release.select_artifacts(baseline[:2])

    def test_compose_resolves_each_producer_and_overrides_stale_shell_ref(self):
        for producer in release.LAYOUT:
            def command(args, **kwargs):
                self.assertEqual(kwargs["env"]["GENESY_REF"], SHA)
                if producer == "web":
                    env = "production" if "production" in args[3] else "staging"
                    wanted = expected("web")
                    output = ([f"ghcr.io/cotit/genesy-office:{SHA}", f"ghcr.io/cotit/genesy-field:{SHA}"]
                              if env == "production" else
                              [image for image in wanted if image not in
                               (f"ghcr.io/cotit/genesy-office:{SHA}", f"ghcr.io/cotit/genesy-field:{SHA}")])
                    self.assertEqual(args[6:], ["office", "field"] if env == "production" else ["shop", "office", "till", "landing", "field", "flow"])
                else:
                    self.assertEqual(args[6:], ["api"])
                    output = expected(producer)
                return SimpleNamespace(stdout="\n".join(["postgres:18-alpine", *output]) + "\n")
            with self.subTest(producer=producer), patch.object(release, "run", side_effect=command):
                self.assertEqual(release.images(producer, SHA), expected(producer))

    def test_compose_rejects_missing_or_wrong_refs_and_environment_mixups(self):
        wrong = [[], [f"ghcr.io/cotit/genesy-api:{BASE}"],
                 [f"ghcr.io/cotit/genesy-api:{SHA}-staging"],
                 [f"ghcr.io/cotit/genesy-other:{SHA}"]]
        for output in wrong:
            with self.subTest(output=output), patch.object(release, "run", return_value=SimpleNamespace(stdout="\n".join(output))), self.assertRaises(ValueError):
                release.images("api-production", SHA)
        with patch.object(release, "run", return_value=SimpleNamespace(stdout="")), self.assertRaises(ValueError):
            release.images("web", SHA)

    def test_missing_base_digest_falls_back_to_complete_group_build(self):
        with patch.dict(os.environ, {"BASE": BASE, "CHANGED": "false"}), \
             patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)), \
             patch.object(release, "resolve", side_effect=["ghcr.io/cotit/genesy-field@" + DIGEST, subprocess.CalledProcessError(1, ["inspect"])]), \
             patch.object(release, "write_manifest") as write, patch.object(Path, "write_text") as image_list, \
             patch("builtins.open", mock_open()) as output:
            release.plan("web")
            write.assert_not_called()
            self.assertEqual(image_list.call_args.args[0].splitlines(), expected("web", "ci-123-2"))
            output().write.assert_called_once_with("mode=build\n")

    def test_reuse_records_immutable_base_sources_without_tagging(self):
        def resolved(image):
            self.assertIn(":" + BASE, image)
            return image.rsplit(":", 1)[0] + "@" + DIGEST
        with patch.dict(os.environ, {"BASE": BASE, "CHANGED": "false"}), \
             patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)), \
             patch.object(release, "resolve", side_effect=resolved), patch.object(release, "run") as command, \
             patch.object(release, "write_manifest") as write, patch.object(Path, "write_text"), \
             patch("builtins.open", mock_open()) as output:
            release.plan("web")
            write.assert_called_once_with("web", manifest("web")["images"])
            command.assert_not_called()
            output().write.assert_called_once_with("mode=reuse\n")

    def test_changed_inputs_or_invalid_base_do_not_reuse_prior_images(self):
        for changed, base in (("true", BASE), ("false", "bad"), ("false", "")):
            with self.subTest(changed=changed, base=base), patch.dict(os.environ, {"BASE": base, "CHANGED": changed}), \
                 patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)), \
                 patch.object(release, "resolve") as resolved, patch.object(release, "write_manifest") as write, \
                 patch.object(Path, "write_text"), patch("builtins.open", mock_open()) as output:
                release.plan("api-production")
                resolved.assert_not_called()
                write.assert_not_called()
                output().write.assert_called_once_with("mode=build\n")

    def test_record_resolves_only_current_candidates_and_never_pushes_sha_tags(self):
        for producer in release.LAYOUT:
            def resolved(image):
                self.assertIn(":ci-123-2", image)
                self.assertNotIn(":" + SHA, image)
                return image.rsplit(":", 1)[0] + "@" + DIGEST
            with self.subTest(producer=producer), patch.object(release, "images", return_value=expected(producer)), \
                 patch.object(release, "resolve", side_effect=resolved), patch.object(release, "run") as command, \
                 patch.object(release, "write_manifest") as write:
                release.record(producer)
                write.assert_called_once_with(producer, manifest(producer)["images"])
                command.assert_not_called()

    def test_write_manifest_validates_before_writing(self):
        with patch.object(release, "images", return_value=expected("web")), patch.object(Path, "write_text") as write:
            release.write_manifest("web", manifest("web")["images"])
            self.assertEqual(json.loads(write.call_args.args[0]), manifest("web"))
            write.reset_mock()
            with self.assertRaises(ValueError):
                release.write_manifest("web", [])
            write.assert_not_called()

    def test_api_variants_build_separately_with_new_cache_and_legacy_fallback(self):
        for producer in ("api-production", "api-staging"):
            with self.subTest(producer=producer), patch.object(release, "run") as command:
                release.build(producer)
                command.assert_called_once()
                args = command.call_args.args[0]
                self.assertIn(f"api.cache-from=type=registry,ref=ghcr.io/cotit/genesy-buildcache:{producer}", args)
                self.assertIn("api.cache-from+=type=registry,ref=ghcr.io/cotit/genesy-buildcache:api", args)
                self.assertIn(f"api.cache-to=type=registry,ref=ghcr.io/cotit/genesy-buildcache:{producer},mode=max,ignore-error=true", args)
                self.assertEqual(args[-1], "api")
                self.assertEqual(command.call_args.kwargs["cwd"], release.ROOT / "deploy" / producer.removeprefix("api-"))
                self.assertNotIn("--push", args)
        with patch.object(release, "run") as command:
            release.build("web")
            self.assertEqual(command.call_count, 2)
            self.assertFalse(any("cache-to" in arg for call in command.call_args_list for arg in call.args[0]))

    def test_resolve_rejects_missing_or_mutable_registry_digest(self):
        image = expected("api-production")[0]
        for digest in ("", "sha256:bad", "sha256:" + "A" * 64):
            with self.subTest(digest=digest), patch.object(release, "run", return_value=SimpleNamespace(stdout=json.dumps({"digest": digest}))), self.assertRaises(ValueError):
                release.resolve(image)
        with patch.object(release, "run", return_value=SimpleNamespace(stdout=json.dumps({"digest": DIGEST}))):
            self.assertEqual(release.resolve(image), image.rsplit(":", 1)[0] + "@" + DIGEST)

    def test_collect_uses_paginated_current_run_api_and_inherited_gh_token(self):
        command = self.fake_collection()
        with patch.object(release, "run", side_effect=command), patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)):
            self.assertEqual(release.collect(), {producer: manifest(producer) for producer in release.LAYOUT})
        args = self.commands[0][0]
        self.assertEqual(args, ["gh", "api", "--paginate", "--slurp", "repos/COTIT/flow.ai/actions/runs/123/artifacts?per_page=100"])
        downloads = [args for args, _ in self.commands[1:]]
        self.assertEqual(len(downloads), 3)
        self.assertTrue(all(args[2].startswith("repos/COTIT/flow.ai/actions/artifacts/") and args[2].endswith("/zip") for args in downloads))
        self.assertTrue(all(ENV["GH_TOKEN"] not in arg for args, _ in self.commands for arg in args))
        with patch.object(release.subprocess, "run", return_value=SimpleNamespace()) as process:
            release.run(["gh", "api", "test"])
            self.assertEqual(os.environ["GH_TOKEN"], "test-token")
            self.assertTrue(process.call_args.kwargs["check"])
            self.assertNotIn("env", process.call_args.kwargs)

    def test_collect_rejects_wrong_repository_before_download(self):
        for repo in ("other/flow.ai", "COTIT/genesy"):
            with self.subTest(repo=repo), patch.dict(os.environ, {"GITHUB_REPOSITORY": repo}), patch.object(release, "run") as command, self.assertRaises(ValueError):
                release.collect()
            command.assert_not_called()

    def test_collect_accepts_older_producer_manifests_during_partial_rerun(self):
        documents = {producer: manifest(producer, 1 if producer != "web" else 2)
                     for producer in release.LAYOUT}
        artifacts = [artifact("api-production", 1), artifact("api-staging", 1),
                     artifact("web", 1), artifact("web", 2)]
        command = self.fake_collection(overrides=documents, artifacts=artifacts)
        with patch.object(release, "run", side_effect=command), patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)):
            self.assertEqual(release.collect(), documents)
        downloads = [args[2] for args, _ in self.commands[1:]]
        self.assertIn("repos/COTIT/flow.ai/actions/artifacts/api-production-1/zip", downloads)
        self.assertNotIn("repos/COTIT/flow.ai/actions/artifacts/web-1/zip", downloads)

    def test_collect_rejects_wrong_manifest_attempt_without_fallback(self):
        command = self.fake_collection(overrides={"web": manifest("web", attempt=1)},
                                       artifacts=[artifact("api-production"), artifact("api-staging"), artifact("web", 1), artifact("web")])
        with patch.object(release, "run", side_effect=command), patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)), self.assertRaises(ValueError):
            release.collect()

    def test_collect_rejects_wrong_extra_and_oversized_archive_members(self):
        bad = [archive({"other.json": "{}"}), archive({"web.json": "{}", "extra": ""}),
               archive({"../web.json": "{}"}), archive({"web.json": " " * (1024 * 1024 + 1)}),
               archive({"web.json": "not json"}), b"not a zip"]
        for zipped in bad:
            with self.subTest(zipped_size=len(zipped)):
                command = self.fake_collection(bad_archives={"web-2": zipped})
                with patch.object(release, "run", side_effect=command), patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)), self.assertRaises((ValueError, zipfile.BadZipFile)):
                    release.collect()

    def test_promotion_validates_all_producers_before_any_tag_write(self):
        for group, invalid in (("api", "web"), ("web", "api-staging")):
            broken = manifest(invalid)
            broken["images"] = []
            command = self.fake_collection(overrides={invalid: broken})
            with self.subTest(group=group), patch.object(release, "run", side_effect=command), \
                 patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)), self.assertRaises(ValueError):
                release.promote(group)
            self.assertFalse(any(args[0] == "docker" for args, _ in self.commands))

    def test_promotion_writes_only_requested_group_after_all_three_archives(self):
        for group in ("api", "web"):
            command = self.fake_collection()
            with self.subTest(group=group), patch.object(release, "run", side_effect=command), \
                 patch.object(release, "images", side_effect=lambda producer, ref: expected(producer, ref)), \
                 patch.object(release, "resolve", side_effect=lambda image: image.rsplit(":", 1)[0] + "@" + DIGEST):
                release.promote(group)
            creates = [args for args, _ in self.commands if args[0] == "docker"]
            wanted = expected("web") if group == "web" else expected("api-production") + expected("api-staging")
            self.assertEqual(sorted(args[args.index("-t") + 1] for args in creates), sorted(wanted))
            self.assertTrue(all("--prefer-index=false" in args for args in creates))
            first_write = next(i for i, (args, _) in enumerate(self.commands) if args[0] == "docker")
            self.assertEqual(first_write, 4)

    def test_partial_promotion_failure_propagates_without_later_writes(self):
        manifests = {producer: manifest(producer) for producer in release.LAYOUT}
        with patch.object(release, "collect", return_value=manifests), \
             patch.object(release, "resolve", side_effect=lambda image: image.rsplit(":", 1)[0] + "@" + DIGEST), \
             patch.object(release, "run", side_effect=[SimpleNamespace(), subprocess.CalledProcessError(1, ["create"])]) as command:
            with self.assertRaises(subprocess.CalledProcessError):
                release.promote("web")
            self.assertEqual(command.call_count, 2)

    def test_promoted_digest_mismatch_blocks_further_writes(self):
        manifests = {producer: manifest(producer) for producer in release.LAYOUT}
        with patch.object(release, "collect", return_value=manifests), patch.object(release, "run") as command, \
             patch.object(release, "resolve", return_value="ghcr.io/cotit/genesy-field@sha256:" + "d" * 64):
            with self.assertRaisesRegex(ValueError, "digest differs"):
                release.promote("web")
            self.assertEqual(command.call_count, 1)


if __name__ == "__main__":
    unittest.main()
