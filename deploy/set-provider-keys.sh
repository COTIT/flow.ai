#!/usr/bin/env bash
# Writes Stripe or Resend keys into deploy/<env>/secrets without echoing them, so keys never
# land in shell history, logs or chat. Run on the VPS from the deploy checkout:
#   deploy/set-provider-keys.sh production stripe   # live (or test) secret key + whsec_
#   deploy/set-provider-keys.sh staging stripe      # test keys only
#   deploy/set-provider-keys.sh production resend   # rewrites secrets/smtp_url
# Then restart with the -f chain in that environment's README ("Turning on Stripe / Resend").
set -Eeuo pipefail

usage() {
  echo "usage: $0 <production|staging> <stripe|resend>" >&2
  exit 2
}
die() {
  echo "refused: $*" >&2
  exit 1
}

env="${1:-}"
provider="${2:-}"
[[ "$env" == production || "$env" == staging ]] || usage
[[ "$provider" == stripe || "$provider" == resend ]] || usage
dir="${SECRETS_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$env/secrets}"
[[ -d "$dir" ]] || die "no secrets folder at $dir (README \"One-time setup\")"

# Read one value silently (prompt on stderr); never printed back.
ask() {
  local value
  IFS= read -rs -p "$1: " value || true
  echo >&2
  printf '%s' "$value"
}

# Atomic write. 444 like the other secrets: containers run as non-root users and the 700
# folder keeps other users out.
put() {
  local tmp
  tmp="$(umask 077 && mktemp "$dir/.$1.XXXXXX")"
  printf '%s' "$2" > "$tmp"
  chmod 444 "$tmp"
  mv -f "$tmp" "$dir/$1"
}

case "$provider" in
  stripe)
    key="$(ask "Stripe secret key (sk_… or rk_…)")"
    [[ "$key" =~ ^[sr]k_(test|live)_[A-Za-z0-9]+$ ]] \
      || die "not a Stripe secret key (sk_test_, sk_live_, rk_test_ or rk_live_)"
    if [[ "$key" == ?k_live_* && "$env" == staging ]]; then
      die "staging runs Stripe test mode only; use a sk_test_ key"
    fi
    secret="$(ask "Stripe webhook signing secret (whsec_…)")"
    [[ "$secret" =~ ^whsec_[A-Za-z0-9]+$ ]] || die "not a webhook signing secret (whsec_…)"
    put stripe_secret_key "$key"
    put stripe_webhook_secret "$secret"
    [[ "$key" == ?k_test_* && "$env" == production ]] \
      && echo "note: a test-mode key on production: no real money moves." >&2
    echo "wrote $dir/stripe_secret_key and $dir/stripe_webhook_secret"
    ;;
  resend)
    [[ "$env" == production ]] || die "staging mail stays on Mailpit"
    key="$(ask "Resend API key (re_…)")"
    [[ "$key" =~ ^re_[A-Za-z0-9_]+$ ]] || die "not a Resend API key (re_…)"
    put smtp_url "smtps://resend:$key@smtp.resend.com:465"
    echo "wrote $dir/smtp_url"
    ;;
esac
unset key secret
