#!/usr/bin/env bash
# Which CI jobs a commit needs, from the files changed since BASE (on main: the last green commit).
#   deploy/ci/changes.sh BASE HEAD   prints api=, web=, img_api=, img_web= (true/false), one per line
# Fails safe: an empty, all-zero or non-ancestor BASE, or a failing diff, runs everything.
# Tests: deploy/ci/changes.test.sh
set -uo pipefail

base="${1:-}" head="${2:-HEAD}"
all() { printf '%s=true\n' api web img_api img_web; exit 0; }

[[ -n "$base" && ! "$base" =~ ^0+$ ]] || all
git merge-base --is-ancestor "$base" "$head" 2> /dev/null || all
# --no-renames: rename detection reads file contents, which a blobless clone would fetch.
files="$(git diff --no-renames --name-only "$base" "$head")" || all

# Workflow or this script changed: the rules themselves may differ, so run everything.
has() { grep -qE "$1" <<< "$files"; }
has '^(\.github/workflows/|deploy/ci/)' && all
# Neither web code nor web image input: the API, deploy scripts, docs, notes.
outside_web() { [[ -n "$files" ]] && grep -qvE '^(apps/api/|deploy/|docs/|\.claude/|[^/]+\.md$)' <<< "$files"; }
compose='^deploy/(staging|production)/docker-compose[^/]*\.yml$'

api=false web=false img_api=false img_web=false
has '^apps/api/' && api=true
# The web job checks the generated client against apps/api/openapi.json.
{ outside_web || has '^(apps/api/openapi\.json|deploy/images/)'; } && web=true
{ has '^apps/api/' || has "$compose"; } && img_api=true
{ outside_web || has '^deploy/images/' || has "$compose"; } && img_web=true

printf 'api=%s\nweb=%s\nimg_api=%s\nimg_web=%s\n' "$api" "$web" "$img_api" "$img_web"
