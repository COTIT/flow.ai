#!/usr/bin/env bash
# Explicit provisioning only. No secret appears in argv, environment, or command output.
set -Eeuo pipefail
set +x
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/ops/lib.sh"
env_setup "${1:-}"
mode="${2:-}"
case "$mode" in bootstrap | verify) ;; *) die 'usage: assistant-role.sh <staging|production> <bootstrap|verify>' ;; esac
role="assistant_checkpoint_$GENESY_ENV"
secrets_dir="$(env_value SECRETS_DIR)"
secrets_dir="${secrets_dir:-./secrets}"
[[ "$secrets_dir" == /* ]] || secrets_dir="$DIR/$secrets_dir"
password_file="$secrets_dir/assistant_checkpoint_password"
if [[ "$mode" == bootstrap ]]; then
  [[ -s "$password_file" ]] || die "missing or empty $password_file"
fi
{
  printf "BEGIN;\nSET LOCAL standard_conforming_strings=on;\n\\set checkpoint_role '%s'\n\\set checkpoint_mode '%s'\n" "$role" "$mode"
  cat "$HERE/assistant-role.sql"
  if [[ "$mode" == bootstrap ]]; then
    python3 - "$password_file" "$role" <<'PY'
import pathlib,sys
password=pathlib.Path(sys.argv[1]).read_text().strip()
if not password or '\x00' in password or '\n' in password or '\r' in password:
    raise SystemExit('Checkpoint password must be one nonempty line')
quote=lambda value: "'"+value.replace("'", "''")+"'"
print("SELECT format('ALTER ROLE %I LOGIN PASSWORD %L', "+quote(sys.argv[2])+", "+quote(password)+") \\gexec")
PY
  fi
  printf 'COMMIT;\n'
} | "${COMPOSE[@]}" exec -T db psql -X -v ON_ERROR_STOP=1 -U genesy -d genesy > /dev/null
echo "assistant-role: $GENESY_ENV $mode ok"
