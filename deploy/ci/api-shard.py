"""Run one API integration-test shard against an isolated CI Postgres server."""
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlsplit


CARGO = ["cargo", "test", "--test", "api"]
PRECHECKS = (
    "db::migrations_fill_an_empty_database",
    "cli_migrate::migrate_fills_an_empty_database_then_is_a_no_op",
)


def inventory(output):
    names = []
    summary = None
    for line in output.splitlines():
        if not line:
            continue
        test = re.fullmatch(r"(\w+(?:::\w+)*): test", line)
        footer = re.fullmatch(r"(\d+) tests?, (\d+) benchmarks?", line)
        if test and summary is None:
            names.append(test[1])
        elif footer and summary is None:
            summary = tuple(map(int, footer.groups()))
        else:
            raise ValueError("unexpected API test inventory format")
    if not names or len(set(names)) != len(names) or summary != (len(names), 0):
        raise ValueError("API test inventory is empty, duplicated or incomplete")
    if not set(PRECHECKS) <= set(names):
        raise ValueError("API inventory is missing an empty-database migration test")
    return sorted(names)


def select(names, index, count):
    if count < 1 or not 0 <= index < count:
        raise ValueError("shard index must be zero-based and smaller than positive shard count")
    selected = [
        name for name in sorted(names)
        if name not in PRECHECKS
        and int.from_bytes(hashlib.sha256(name.encode()).digest(), "big") % count == index
    ]
    if not selected:
        raise ValueError("API shard selects no tests")
    return selected


def run(index, count, env):
    if count < 1 or not 0 <= index < count:
        raise ValueError("shard index must be zero-based and smaller than positive shard count")
    database = env.get("DATABASE_URL", "")
    url = urlsplit(database)
    if (
        url.scheme not in ("postgres", "postgresql")
        or url.hostname not in ("postgres", "localhost", "127.0.0.1", "::1")
        or url.username != "postgres"
        or url.path != "/postgres"
        or url.query or url.fragment or url.port == 0
        or any(c.isspace() or c == "\0" for c in database)
    ):
        raise ValueError("DATABASE_URL must use the local CI postgres admin database")
    target = env.get("CARGO_TARGET_DIR")
    if not target:
        raise ValueError("CARGO_TARGET_DIR is required")
    listed = subprocess.run(
        CARGO + ["--", "--list", "--color", "never"],
        check=True, stdout=subprocess.PIPE, text=True, env=env,
    )
    names = inventory(listed.stdout)
    selected = select(names, index, count)
    print(f"API shard {index + 1}/{count}: {len(selected)} of {len(names) - len(PRECHECKS)} tests", flush=True)
    subprocess.run(CARGO + ["--", "--exact", *PRECHECKS], check=True, env=env)
    subprocess.run(
        [str(Path(target) / "debug" / "genesy-360-api"), "migrate"],
        check=True, env={**env, "DATABASE_URL": url._replace(path="/template1").geturl()},
    )
    subprocess.run(CARGO + ["--", "--exact", *selected], check=True, env=env)


if __name__ == "__main__":
    try:
        if len(sys.argv) != 3:
            raise ValueError("usage: api-shard.py <zero-based index> <count>")
        run(int(sys.argv[1]), int(sys.argv[2]), dict(os.environ))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"api-shard: {error}", file=sys.stderr)
        sys.exit(error.returncode if isinstance(error, subprocess.CalledProcessError) else 1)
