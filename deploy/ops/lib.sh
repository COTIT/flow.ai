# shellcheck shell=bash disable=SC2034  # variables here are read by the scripts that source it
# Shared by deploy/deploy.sh and deploy/ops/*.sh (sourced, never run). Paths, the compose -f
# chain and the alert notifier live here once.
# Test overrides: GENESY_ROOT (checkout), GENESY_OPS_CONF (/etc/genesy-ops),
# GENESY_BACKUPS (/srv/genesy-backups), GENESY_OPS_STATE (/var/lib/genesy-ops).

OPS_CONF="${GENESY_OPS_CONF:-/etc/genesy-ops}"
BACKUPS="${GENESY_BACKUPS:-/srv/genesy-backups}"
OPS_STATE="${GENESY_OPS_STATE:-/var/lib/genesy-ops}"

# die <message>: stop; scripts that set $check also send it as a fail alert.
die() {
  echo "$(basename "$0"): $*" >&2
  [[ -z "${check:-}" ]] || notify "$check" fail "$*"
  exit 1
}

# env_setup <staging|production>: sets ROOT (checkout), DIR (deploy/<env>), ENV_FILE, COMPOSE
# (the full command with every -f the stack needs), MIGRATE_SVC and PUBLIC_HOSTS.
env_setup() {
  case "${1:-}" in
    production) ROOT="${GENESY_ROOT:-/srv/genesy-prod}" MIGRATE_SVC=cli
      PUBLIC_HOSTS="api.genesy.app shop.genesy.app office.genesy.app till.genesy.app field.genesy.app genesy.app" ;;
    staging) ROOT="${GENESY_ROOT:-/srv/genesy-staging}" MIGRATE_SVC=seed
      PUBLIC_HOSTS="staging-api.genesy.app staging-shop.genesy.app staging-office.genesy.app staging-till.genesy.app staging-field.genesy.app staging.genesy.app" ;;
    *) die "environment must be staging or production, got '${1:-}'" ;;
  esac
  GENESY_ENV="$1"
  DIR="$ROOT/deploy/$1"
  ENV_FILE="$DIR/.env.$1"
  [[ -f "$ENV_FILE" ]] || die "missing $ENV_FILE"
  COMPOSE=(docker compose -f "$DIR/docker-compose.$1.yml")
  # Untracked until DNS is live (README "Turning on Stripe"); sits between main and Stripe.
  [[ -f "$DIR/docker-compose.nodns.yml" ]] && COMPOSE+=(-f "$DIR/docker-compose.nodns.yml")
  # Stripe is on exactly when its key file exists, so no command can forget the overlay.
  [[ -f "$DIR/secrets/stripe_secret_key" ]] && COMPOSE+=(-f "$DIR/docker-compose.stripe.yml")
  # File uploads likewise, production only (staging has no overlay: demo files stay out of B2).
  [[ -f "$DIR/secrets/b2_uploads_key" && -f "$DIR/docker-compose.uploads.yml" ]] \
    && COMPOSE+=(-f "$DIR/docker-compose.uploads.yml")
  COMPOSE+=(--env-file "$ENV_FILE")
}

# env_value KEY: the value of KEY in ENV_FILE (last one wins), empty when unset.
env_value() { sed -n "s/^$1=//p" "$ENV_FILE" | tail -1; }

# notify <check> <start|ok|fail> [message]: always logged; also pinged when OPS_CONF/ping_base
# holds a dead-man URL (healthchecks.io style: <base>/<check>, /start, /fail). A failed ping
# never fails the caller.
notify() {
  local check="$1" status="$2" msg="${3:-}" url
  echo "notify: $check $status${msg:+: $msg}" >&2
  [[ -s "$OPS_CONF/ping_base" ]] || return 0
  url="$(tr -d '[:space:]' < "$OPS_CONF/ping_base")/$check"
  case "$status" in start) url+=/start ;; fail) url+=/fail ;; esac
  curl -fsS -m 10 --retry 3 -o /dev/null --data-raw "$msg" "$url" || echo "notify: ping failed" >&2
}

# schema_version: newest applied migration of the env's live database, 0 on a fresh one.
schema_version() {
  local psql=("${COMPOSE[@]}" exec -T db psql -U genesy -d genesy -Atc)
  if [[ "$("${psql[@]}" "SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")" == t ]]; then
    "${psql[@]}" "SELECT coalesce(max(version), 0) FROM _sqlx_migrations WHERE success"
  else
    echo 0
  fi
}

# hosts_down: prints " host=code" for every public host of the env that answers wrong: API
# /healthz must be 200, apps 2xx/3xx (the shop 503 too, until its app key is set). Asks the
# local Traefik (--resolve), so DNS and certificates don't matter here.
hosts_down() {
  local host path want code
  for host in $PUBLIC_HOSTS; do
    path=/ want='2..|3..'
    [[ "$host" == *api.* ]] && path=/healthz want=200
    [[ "$host" == *shop.* ]] && want='2..|3..|503'
    code="$(curl -sk -o /dev/null -m 10 -w '%{http_code}' --resolve "$host:443:127.0.0.1" "https://$host$path" || true)"
    [[ "$code" =~ ^($want)$ ]] || printf ' %s=%s' "$host" "${code:-none}"
  done
}

# latest_dump <env>: newest finished dump of that env, empty when none.
latest_dump() {
  ls -1t "$BACKUPS/$1/genesy-$1-"*.dump 2> /dev/null | head -1
}
