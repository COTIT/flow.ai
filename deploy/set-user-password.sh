#!/usr/bin/env bash
# Sets a login password, typed by the operator, for a user of AFM's production workspace (`afm`).
# Asks twice without echo; the password reaches the API CLI on stdin only, never argv, env,
# shell history or logs. The API applies the password rules and prints what it did (never the
# password). Run on the VPS from the deploy checkout:
#   deploy/set-user-password.sh production <email> --admin
#   deploy/set-user-password.sh production <email> --admin --first-name <First> --last-name <Last>
# Names are needed only for a new user. --admin = join afm's Admin Team (every permission).
# An existing user's other sessions end.
set -Eeuo pipefail

usage() {
  echo "usage: $0 production <email> [--admin] [--first-name F --last-name L]" >&2
  exit 2
}
die() {
  echo "refused: $*" >&2
  exit 1
}

env="${1:-}"
email="${2:-}"
[[ "$env" == production ]] || usage
[[ -n "$email" && "$email" != -* ]] || usage
shift 2
deploy="${DEPLOY_SH:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/deploy.sh}"

# Read one line silently (prompt on stderr); never printed back.
ask() {
  local value
  IFS= read -rs -p "$1: " value || true
  echo >&2
  printf '%s' "$value"
}

password="$(ask "New password for $email")"
[[ -n "$password" ]] || die "empty password"
again="$(ask "Same password again")"
if [[ "$password" != "$again" ]]; then
  unset password again
  die "the two passwords differ; nothing changed"
fi
unset again

# printf is a shell builtin: the password never shows up in a process list.
status=0
printf '%s\n' "$password" \
  | "$deploy" production compose --profile cli run --rm -T cli \
    set-user-password --workspace afm --email "$email" "$@" || status=$?
unset password
exit "$status"
