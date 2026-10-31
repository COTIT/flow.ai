#!/usr/bin/env bash
# Housekeeping for the CI user's own rootless Docker. Never prod's: it refuses any other
# DOCKER_HOST, and the CI user can't open prod's socket anyway.
#   cleanup.sh        job-started hook (runner .env). Runs before the job's containers
#                     start, so it wipes everything a crashed job left behind and caps
#                     the cache volumes.
#   cleanup.sh prune  daily timer. Jobs may be running, so it only touches unused things.
set -uo pipefail

[[ "${DOCKER_HOST:-}" == unix:///run/user/*/docker.sock ]] \
  || { echo "cleanup: DOCKER_HOST is not a rootless socket, refusing" >&2; exit 1; }

# Remove a volume when bigger than $2 GB; the next job rebuilds it.
cap() {
  local mountpoint size_kb
  mountpoint="$(docker volume inspect -f '{{.Mountpoint}}' "$1" 2> /dev/null)" || return 0
  size_kb="$(du -sk "$mountpoint" 2> /dev/null | cut -f1)"
  if (( ${size_kb:-0} > $2 * 1024 * 1024 )); then
    echo "cleanup: $1 is over $2 GB, removing"
    docker volume rm -f "$1" > /dev/null
  fi
}

if [[ "${1:-job}" == job ]]; then
  docker ps -aq | xargs -r docker rm -f > /dev/null
  docker network prune -f > /dev/null
  cap genesy-ci-cargo-target 10
  cap genesy-ci-cargo-registry 3
  cap genesy-ci-pnpm-store 5
  exit 0
fi

docker container prune -f > /dev/null
docker network prune -f > /dev/null
docker image prune -f > /dev/null
docker builder prune -f > /dev/null
# Unused images are re-pulled on the next job, so only drop them when the CI disk fills up.
used_pct="$(df --output=pcent "$HOME" | tail -1 | tr -dc 0-9)"
if (( used_pct >= 70 )); then
  echo "cleanup: CI disk at ${used_pct}%, removing all unused images and build cache"
  docker image prune -af > /dev/null
  docker builder prune -af > /dev/null
fi
docker system df
