#!/usr/bin/env bash
# Tests for set-provider-keys.sh with keys piped on stdin:  bash deploy/set-provider-keys.test.sh
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
# Fake keys only: real ones never appear in tests.
run() { SECRETS_DIR="$tmp" bash "$HERE/set-provider-keys.sh" "$@" > "$tmp/out" 2>&1; }
mode() { stat -c %a "$1" 2> /dev/null || stat -f %Lp "$1"; }

# Stripe on staging: test key and signing secret land in their files, never on screen.
printf 'sk_test_AAA111\nwhsec_BBB222\n' | run staging stripe || fail "staging stripe refused: $(cat "$tmp/out")"
[[ "$(cat "$tmp/stripe_secret_key")" == "sk_test_AAA111" ]] || fail "secret key not written"
[[ "$(cat "$tmp/stripe_webhook_secret")" == "whsec_BBB222" ]] || fail "webhook secret not written"
[[ "$(mode "$tmp/stripe_secret_key")" == "444" ]] || fail "secret key mode $(mode "$tmp/stripe_secret_key")"
if grep -q "AAA111\|BBB222" "$tmp/out"; then fail "a key was echoed"; fi

# Staging refuses a live key and leaves the old file alone.
if printf 'sk_live_CCC333\nwhsec_DDD444\n' | run staging stripe; then fail "staging took a live key"; fi
[[ "$(cat "$tmp/stripe_secret_key")" == "sk_test_AAA111" ]] || fail "refused key overwrote the file"
if grep -q "CCC333" "$tmp/out"; then fail "the refused key was echoed"; fi

# Production takes a live key; wrong shapes are refused.
printf 'rk_live_EEE555\nwhsec_FFF666\n' | run production stripe || fail "production live key refused"
[[ "$(cat "$tmp/stripe_secret_key")" == "rk_live_EEE555" ]] || fail "live key not written"
if printf 'pk_live_x\nwhsec_y\n' | run production stripe; then fail "publishable key taken"; fi
if printf 'sk_live_x\nnot_a_secret\n' | run production stripe; then fail "bad webhook secret taken"; fi
if printf '\n\n' | run production stripe; then fail "empty key taken"; fi

# Resend on production: SMTP_URL with the API key as password.
printf 're_GGG777_hh\n' | run production resend || fail "resend refused: $(cat "$tmp/out")"
[[ "$(cat "$tmp/smtp_url")" == "smtps://resend:re_GGG777_hh@smtp.resend.com:465" ]] || fail "smtp_url: $(cat "$tmp/smtp_url")"
if grep -q "GGG777" "$tmp/out"; then fail "resend key echoed"; fi
if printf 're_x/y\n' | run production resend; then fail "resend key with odd characters taken"; fi
# Staging mail stays on Mailpit.
if printf 're_x\n' | run staging resend; then fail "staging took resend"; fi

# B2 uploads key: production only, written as is, never echoed.
printf 'K005AbCdEfGh+IjKlMnOpQrStUvWx/yz12\n' | run production b2-uploads || fail "b2-uploads refused: $(cat "$tmp/out")"
[[ "$(cat "$tmp/b2_uploads_key")" == "K005AbCdEfGh+IjKlMnOpQrStUvWx/yz12" ]] || fail "b2 key not written"
[[ "$(mode "$tmp/b2_uploads_key")" == "444" ]] || fail "b2 key mode"
if grep -q "AbCdEfGh" "$tmp/out"; then fail "b2 key echoed"; fi
if printf 'not a key!\n' | run production b2-uploads; then fail "bad b2 key taken"; fi
if printf 'K005AbCdEfGhIjKlMnOpQrStUvWxyz12\n' | run staging b2-uploads; then fail "staging took b2-uploads"; fi

# Usage errors.
if run nowhere stripe < /dev/null; then fail "unknown environment taken"; fi
if run production mollie < /dev/null; then fail "unknown provider taken"; fi
echo "set-provider-keys.sh: all tests passed"
