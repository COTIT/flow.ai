"""Run one API integration-test shard against an isolated CI Postgres server."""
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlsplit


# Every `apps/api/tests/api_*.rs` is one integration-test crate; the split keeps each compile under the runner's RAM.
TESTS = Path(__file__).resolve().parents[2] / "apps" / "api" / "tests"
# `tests/common` is compiled into every crate, so its own unit tests are listed once per crate; run them once.
SHARED = "common::"
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
    return sorted(names)


def crates():
    names = sorted(path.stem for path in TESTS.glob("api_*.rs"))
    if not names:
        raise ValueError("no API test crates under apps/api/tests")
    return names


def cargo(crate):
    return ["cargo", "test", "--test", crate]


def owners(listings):
    """Map each test name to the one crate that runs it."""
    owner = {}
    for crate in sorted(listings):
        for name in listings[crate]:
            if name in owner and not name.startswith(SHARED):
                raise ValueError(f"API test {name} is in two crates")
            owner.setdefault(name, crate)
    if not set(PRECHECKS) <= set(owner):
        raise ValueError("API inventory is missing an empty-database migration test")
    return owner


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
    listings = {
        crate: inventory(subprocess.run(
            cargo(crate) + ["--", "--list", "--color", "never"],
            check=True, stdout=subprocess.PIPE, text=True, env=env,
        ).stdout)
        for crate in crates()
    }
    owner = owners(listings)
    selected = select(owner, index, count)
    print(f"API shard {index + 1}/{count}: {len(selected)} of {len(owner) - len(PRECHECKS)} tests", flush=True)
    for crate in sorted({owner[name] for name in PRECHECKS}):
        subprocess.run(cargo(crate) + ["--", "--exact", *(n for n in PRECHECKS if owner[n] == crate)], check=True, env=env)
    subprocess.run(
        [str(Path(target) / "debug" / "genesy-360-api"), "migrate"],
        check=True, env={**env, "DATABASE_URL": url._replace(path="/template1").geturl()},
    )
    for crate in sorted(listings):
        mine = [name for name in selected if owner[name] == crate]
        if mine:
            subprocess.run(cargo(crate) + ["--", "--exact", *mine], check=True, env=env)


if __name__ == "__main__":
    try:
        if len(sys.argv) != 3:
            raise ValueError("usage: api-shard.py <zero-based index> <count>")
        run(int(sys.argv[1]), int(sys.argv[2]), dict(os.environ))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"api-shard: {error}", file=sys.stderr)
        sys.exit(error.returncode if isinstance(error, subprocess.CalledProcessError) else 1)
