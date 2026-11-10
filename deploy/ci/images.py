#!/usr/bin/env python3
"""Prepare candidates and promote reviewed digests; final SHA tags are written only by promote."""
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[2]
LAYOUT = {
    "api-production": {"production": ["api"]},
    "api-staging": {"staging": ["api"]},
    "web": {"production": ["office", "field"], "staging": ["shop", "office", "till", "landing", "field", "flow"]},
}
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")


def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def context():
    commit, run_id = os.environ["GITHUB_SHA"], os.environ["GITHUB_RUN_ID"]
    attempt = int(os.environ["GITHUB_RUN_ATTEMPT"])
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or not run_id.isdecimal() or int(run_id) < 1 or attempt < 1:
        raise ValueError("invalid release commit/run/attempt")
    return commit, run_id, attempt


def compose_file(env):
    return ROOT / "deploy" / env / ("docker-compose." + env + ".yml")


def images(producer, ref):
    found = set()
    for env, services in LAYOUT[producer].items():
        output = run(
            ["docker", "compose", "-f", str(compose_file(env)), "config", "--images", *services],
            env={**os.environ, "GENESY_REF": ref}, capture_output=True, text=True,
        ).stdout
        found.update(line for line in output.splitlines() if line.startswith("ghcr.io/cotit/genesy-"))
    expected = {
        "ghcr.io/cotit/genesy-" + service + ":" + ref
        + ("-staging" if env == "staging" and service in ("api", "office", "field") else "")
        for env, services in LAYOUT[producer].items() for service in services
    }
    if found != expected:
        raise ValueError("unexpected compose image set for " + producer)
    return sorted(found)


def resolve(image):
    info = json.loads(run(
        ["docker", "buildx", "imagetools", "inspect", image, "--format", "{{json .Manifest}}"],
        capture_output=True, text=True,
    ).stdout)
    digest = info.get("digest", "")
    if not DIGEST.fullmatch(digest):
        raise ValueError("registry returned no immutable digest for " + image)
    return image.rsplit(":", 1)[0] + "@" + digest


def write_manifest(producer, entries):
    commit, run_id, attempt = context()
    manifest = dict(commit=commit, run_id=run_id, attempt=attempt, producer=producer, images=entries)
    validate(manifest, producer, images(producer, commit), attempt)
    Path(producer + ".json").write_text(json.dumps(manifest, indent=2) + "\n")


def plan(producer):
    commit, run_id, attempt = context()
    candidate = "ci-" + run_id + "-" + str(attempt)
    destinations = images(producer, commit)
    candidates = images(producer, candidate)
    Path("images.txt").write_text("\n".join(candidates) + "\n")
    base = os.environ.get("BASE", "")
    entries = []
    if os.environ.get("CHANGED") == "false" and re.fullmatch(r"[0-9a-f]{40}", base):
        try:
            for img in destinations:
                entries.append(dict(destination=img, source=resolve(img.replace(":" + commit, ":" + base))))
        except (subprocess.CalledProcessError, ValueError, KeyError):
            entries = []  # Missing prior tags fall back to a complete group build.
    if entries:
        write_manifest(producer, entries)
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write("mode=" + ("reuse" if entries else "build") + "\n")


def build(producer):
    cache = []
    if producer.startswith("api-"):
        cache = [
            "--set", "api.cache-from=type=registry,ref=ghcr.io/cotit/genesy-buildcache:" + producer,
            "--set", "api.cache-from+=type=registry,ref=ghcr.io/cotit/genesy-buildcache:api",
            "--set", "api.cache-to=type=registry,ref=ghcr.io/cotit/genesy-buildcache:" + producer + ",mode=max,ignore-error=true",
        ]
    for env, services in LAYOUT[producer].items():
        file = compose_file(env)
        run(["docker", "buildx", "bake", "--allow", "fs.read=" + str(ROOT),
             "-f", file.name, "--load", *cache, *services], cwd=file.parent)


def record(producer):
    commit, run_id, attempt = context()
    candidate = "ci-" + run_id + "-" + str(attempt)
    entries = [
        dict(destination=img, source=resolve(img.replace(":" + commit, ":" + candidate)))
        for img in images(producer, commit)
    ]
    write_manifest(producer, entries)


def validate(manifest, producer, expected, attempt):
    commit, run_id, _ = context()
    if any(manifest.get(key) != value for key, value in {
        "commit": commit, "run_id": run_id, "attempt": attempt, "producer": producer,
    }.items()):
        raise ValueError("manifest does not match producer/run/commit/attempt")
    entries = manifest.get("images")
    if not isinstance(entries, list) or len(entries) != len(expected):
        raise ValueError("incomplete image manifest")
    destinations = []
    for entry in entries:
        destination, source = entry.get("destination", ""), entry.get("source", "")
        if destination not in expected or not isinstance(source, str):
            raise ValueError("unexpected image destination")
        repo = destination.rsplit(":", 1)[0]
        if not source.startswith(repo + "@") or not DIGEST.fullmatch(source[len(repo) + 1:]):
            raise ValueError("source is not the expected repository's immutable digest")
        destinations.append(destination)
    if sorted(destinations) != sorted(expected):
        raise ValueError("duplicate or missing image destination")
    return entries


def select_artifacts(artifacts):
    commit, run_id, current_attempt = context()
    candidates = {}
    pattern = re.compile(r"release-" + run_id + r"-(api-production|api-staging|web)-([1-9][0-9]*)")
    for artifact in artifacts:
        match = pattern.fullmatch(artifact["name"])
        if not match or int(match[2]) > current_attempt:
            continue
        producer, attempt = match[1], int(match[2])
        candidates.setdefault(producer, {}).setdefault(attempt, []).append(artifact)
    if set(candidates) != set(LAYOUT):
        raise ValueError("missing release manifest; rerun the full workflow")
    selected = {}
    for producer, attempts in candidates.items():
        attempt = max(attempts)
        if len(attempts[attempt]) != 1:
            raise ValueError("duplicate producer artifact attempt")
        selected[producer] = (attempt, attempts[attempt][0])
    for _, artifact in selected.values():
        origin = artifact.get("workflow_run", {})
        if artifact.get("expired") or artifact.get("size_in_bytes", 0) > 1024 * 1024:
            raise ValueError("expired or oversized manifest; rerun the full workflow")
        if str(origin.get("id")) != run_id or origin.get("head_sha") != commit:
            raise ValueError("artifact is from another run or commit")
    return selected


def collect():
    _, run_id, _ = context()
    repo = os.environ["GITHUB_REPOSITORY"]
    if repo.lower() != "cotit/genesy":
        raise ValueError("unexpected release repository")
    pages = json.loads(run(
        ["gh", "api", "--paginate", "--slurp", "repos/" + repo + "/actions/runs/" + run_id + "/artifacts?per_page=100"],
        capture_output=True, text=True,
    ).stdout)
    selected = select_artifacts([artifact for page in pages for artifact in page["artifacts"]])
    manifests = {}
    for producer, (attempt, artifact) in selected.items():
        archive = run(
            ["gh", "api", "repos/" + repo + "/actions/artifacts/" + str(artifact["id"]) + "/zip"],
            capture_output=True,
        ).stdout
        with zipfile.ZipFile(io.BytesIO(archive)) as files:
            name = producer + ".json"
            if files.namelist() != [name] or files.getinfo(name).file_size > 1024 * 1024:
                raise ValueError("unexpected manifest archive contents")
            manifest = json.loads(files.read(name))
        validate(manifest, producer, images(producer, context()[0]), attempt)
        manifests[producer] = manifest
    return manifests


def promote(group):
    # Validate every producer before writing any final tag, including the other image group.
    manifests = collect()
    for producer, manifest in manifests.items():
        if (group == "web") != (producer == "web"):
            continue
        for entry in manifest["images"]:
            run(["docker", "buildx", "imagetools", "create", "--prefer-index=false",
                 "-t", entry["destination"], entry["source"]])
            if resolve(entry["destination"]) != entry["source"]:
                raise ValueError("promoted digest differs from reviewed source")
            print("released " + entry["destination"], flush=True)


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("plan", "build", "record", "promote"):
        raise ValueError("usage: images.py <plan|build|record> <producer> | promote <api|web>")
    command, target = sys.argv[1:]
    if target not in (("api", "web") if command == "promote" else LAYOUT):
        raise ValueError("unknown image producer/group")
    globals()[command](target)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, AttributeError, zipfile.BadZipFile, subprocess.CalledProcessError) as error:
        sys.exit("images: " + str(error))
