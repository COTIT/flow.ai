#!/usr/bin/env bash
# One command per deploy step on the VPS, as user deploy (README "Deploy"):
#   deploy.sh <staging|production> <sha> [--bundle <file>]   deploy a commit
#   deploy.sh <staging|production> rollback [--restore-db]    back to the previous version
#   deploy.sh <staging|production> prune                     drop image tags but live + previous
#   deploy.sh <staging|production> compose <args…>           docker compose with the right -f chain
# Deploy: commit (from the checkout or a git bundle; the VPS has no git network access) →
# images (IMAGE_SOURCE=build|pull in .env.<env>) → backup → migrate → up --wait → health →
# record live/previous in deploy/<env>/.deploy-state → prune.
# The whole body is one function: bash has read all of it before `git checkout` rewrites this file.
set -Eeuo pipefail

main() {
  local here
  here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
  . "$here/ops/lib.sh"
  OPS_LIB="${GENESY_OPS_LIB:-/usr/local/lib/genesy-ops}"
  env_setup "${1:-}"
  STATE="$DIR/.deploy-state"
  local action="${2:-}"
  (( $# >= 2 )) && shift 2 || shift $#
  case "$action" in
    compose) exec "${COMPOSE[@]}" "$@" ;;
    "") die "usage: deploy.sh <staging|production> <sha> [--bundle <file>] | rollback [--restore-db] | prune | compose <args>" ;;
  esac
  exec 9> "$DIR/.deploy.lock"
  flock -n 9 || die "another deploy of $GENESY_ENV is running"
  case "$action" in
    rollback) rollback "$@" ;;
    prune) prune ;;
    *) deploy "$action" "$@" ;;
  esac
}

state_get() { [[ ! -f "$STATE" ]] || sed -n "s/^$1=//p" "$STATE" | tail -1; }

# state_write key=value…: replaces the state file in one move.
state_write() { printf '%s\n' "$@" > "$STATE.new" && mv "$STATE.new" "$STATE"; }

log() { echo "$(date -u +%FT%TZ) ${USER:-?} $GENESY_ENV $*" >> "$DIR/deploys.log"; }

# use_ref <sha>: check the commit out and make its short sha the image tag in the env file.
use_ref() {
  git -C "$ROOT" checkout -q --detach "$1"
  REF="$(git -C "$ROOT" rev-parse --short HEAD)"
  { grep -v '^GENESY_REF=' "$ENV_FILE" || true; echo "GENESY_REF=$REF"; } > "$ENV_FILE.new"
  cat "$ENV_FILE.new" > "$ENV_FILE" && rm "$ENV_FILE.new"   # cat keeps the file's 600 mode
}

images() {
  case "$(env_value IMAGE_SOURCE)" in
    "" | build) "${COMPOSE[@]}" build ;;
    pull) "${COMPOSE[@]}" pull ;;
    *) die "IMAGE_SOURCE must be build or pull" ;;
  esac
}

# health: every host answers (ops/lib.sh hosts_down). Traefik needs a moment to see new
# containers: up to 12 tries, 5 s apart.
health() {
  local bad
  for _ in $(seq 12); do
    bad="$(hosts_down)"
    [[ -z "$bad" ]] && { echo "health: all ${PUBLIC_HOSTS// /, } answer"; return 0; }
    sleep "${HEALTH_WAIT:-5}"
  done
  echo "health: failing:$bad" >&2
  return 1
}

# backup <label>: root-only dump via the installed kit; prints the dump's path.
backup() {
  sudo -n "$OPS_LIB/backup.sh" "$GENESY_ENV" "$1" | tee /dev/stderr \
    | sed -n 's/^backup: \(\/[^ ]*\.dump\) .*/\1/p'
}

deploy() {
  local sha="$1" bundle=""
  shift
  [[ "${1:-}" == --bundle ]] && bundle="$(readlink -f "${2:?--bundle needs a file}")"
  [[ "$sha" =~ ^[0-9a-f]{7,40}$ ]] || die "not a commit sha: '$sha'"
  local free_kb
  free_kb="$(df -Pk "$ROOT" | awk 'NR == 2 { print $4 }')"
  (( free_kb >= 5 * 1024 * 1024 )) || die "less than 5 GB free on $ROOT; prune first"

  if [[ -n "$bundle" ]]; then
    git -C "$ROOT" bundle verify -q "$bundle" > /dev/null
    # Every head in the bundle; only the objects matter, cat-file finds the sha below.
    git -C "$ROOT" bundle list-heads "$bundle" | cut -d' ' -f2 \
      | xargs git -C "$ROOT" fetch -q "$bundle"
  fi
  git -C "$ROOT" cat-file -e "$sha^{commit}" 2> /dev/null \
    || die "commit $sha is not in $ROOT; copy a bundle over and pass --bundle <file>"
  local was_live was_schema
  was_live="$(state_get live)"
  use_ref "$sha"
  log "deploy $REF start (was ${was_live:-none})"
  images

  "${COMPOSE[@]}" up -d --wait db
  was_schema="$(schema_version)"
  local dump
  dump="$(backup "pre-$REF")"
  [[ -n "$dump" ]] || die "backup printed no dump path"
  "${COMPOSE[@]}" --profile "$MIGRATE_SVC" run --rm "$MIGRATE_SVC" migrate
  "${COMPOSE[@]}" up -d --wait --wait-timeout 300

  # A redeploy of the live commit keeps the recorded previous version.
  local previous="$was_live" previous_schema="$was_schema"
  if [[ "$was_live" == "$REF" ]]; then
    previous="$(state_get previous)" previous_schema="$(state_get previous_schema)"
  fi
  state_write "live=$REF" "live_schema=$(schema_version)" "previous=$previous" \
    "previous_schema=$previous_schema" "pre_deploy_dump=$dump"
  if ! health; then
    log "deploy $REF unhealthy"
    die "deploy of $REF is up but unhealthy; to go back: deploy/deploy.sh $GENESY_ENV rollback"
  fi
  log "deploy $REF ok"
  prune
}

rollback() {
  local restore=0
  [[ "${1:-}" == --restore-db ]] && restore=1
  local live previous previous_schema dump now_schema
  live="$(state_get live)" previous="$(state_get previous)"
  previous_schema="$(state_get previous_schema)" dump="$(state_get pre_deploy_dump)"
  [[ -n "$previous" ]] || die "no previous version recorded in $STATE"
  now_schema="$(schema_version)"
  if [[ "$now_schema" != "$previous_schema" ]]; then
    (( restore )) || die "the database changed since $previous (schema $previous_schema -> $now_schema). \
Rolling back needs --restore-db, which restores the pre-deploy dump and loses everything written since."
    [[ -n "$dump" ]] || die "no pre-deploy dump recorded: deploy $previous again instead"
  else
    restore=0
  fi

  use_ref "$previous"
  log "rollback $live -> $REF start (restore-db=$restore)"
  # Build mode: the previous images are still here (prune keeps them); pull fetches them.
  if [[ "$(env_value IMAGE_SOURCE)" == pull ]] || ! docker image inspect "$("${COMPOSE[@]}" config --images | grep -v @ | head -1)" > /dev/null 2>&1; then
    images
  fi
  (( restore )) && sudo -n "$OPS_LIB/restore.sh" "$GENESY_ENV" "$dump"
  "${COMPOSE[@]}" up -d --wait --wait-timeout 300
  state_write "live=$REF" "live_schema=$(schema_version)" "previous=$live" \
    "previous_schema=$now_schema" "pre_deploy_dump="
  health || { log "rollback to $REF unhealthy"; die "rolled back to $REF but it is unhealthy"; }
  log "rollback to $REF ok"
}

# prune: image tags of this stack other than live and previous; dangling images; build cache
# above 10 GB (kept warm so the next build is quick).
prune() {
  local keep repo tag
  keep=" $(state_get live) $(state_get previous) "
  for repo in $("${COMPOSE[@]}" config --images | grep -v @ | sed 's/:[^:/]*$//' | sort -u); do
    for tag in $(docker image ls --format '{{.Tag}}' "$repo"); do
      [[ "$keep" == *" $tag "* ]] || docker image rm "$repo:$tag" > /dev/null || true
    done
  done
  docker image prune -f > /dev/null
  docker builder prune -f --max-used-space 10GB > /dev/null
  echo "prune: kept${keep% }"
}

main "$@"; exit
