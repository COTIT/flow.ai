#!/usr/bin/env bash
# watch.sh: the 5-minute check (timer genesy-watch), as root. One alert, check "vps-watch",
# listing everything wrong, each line prefixed with its env:
#   - every public host answers through Traefik (lib.sh hosts_down);
#   - every certificate verifies and is valid 14 more days (Let's Encrypt renews at 30: 14 = stuck);
#   - no container restart-looping (restart count up 3+ since the last run), restarting or unhealthy;
#   - disk use under DISK_ALERT_PCT (80) on / and the backups disk;
#   - the newest production dump is under 26 h old.
# A dead VPS sends nothing, and the missing ping is the alert.
set -Euo pipefail
. "$(dirname "$(readlink -f "$0")")/lib.sh"

check=vps-watch
problems=()
install -d -m 700 "$OPS_STATE"

for env in production staging; do
  # An env with no checkout yet (or ever) is skipped, not an alert.
  ( env_setup "$env" ) > /dev/null 2>&1 || continue
  env_setup "$env"

  down="$(hosts_down)"
  [[ -z "$down" ]] || problems+=("$env: down:$down")

  for host in $PUBLIC_HOSTS; do
    # Verified chain and name: Traefik's own default certificate (no Let's Encrypt one) fails here.
    if ! openssl s_client -connect 127.0.0.1:443 -servername "$host" -verify_hostname "$host" \
      -verify_return_error < /dev/null 2> /dev/null \
      | openssl x509 -noout -checkend $((14 * 86400)) > /dev/null 2>&1; then
      problems+=("$env: certificate of $host expires within 14 days (or is missing)")
    fi
  done

  project="$(sed -n 's/^name: //p' "$DIR/docker-compose.$env.yml")"
  seen="$OPS_STATE/restarts-$env"
  touch "$seen"
  ids="$(docker ps -aq --filter "label=com.docker.compose.project=$project")"
  now=""
  if [[ -n "$ids" ]]; then
    now="$(docker inspect -f '{{.Name}} {{.RestartCount}} {{.State.Status}} {{if .State.Health}}{{.State.Health.Status}}{{end}}' $ids)"
  fi
  while read -r name count status health; do
    [[ -n "$name" ]] || continue
    before="$(awk -v n="$name" '$1 == n { print $2 }' "$seen")"
    if (( count - ${before:-$count} >= 3 )); then
      problems+=("$env: ${name#/} restarted $((count - before)) times in 5 min")
    fi
    [[ "$status" == restarting ]] && problems+=("$env: ${name#/} is restarting")
    [[ "$health" == unhealthy ]] && problems+=("$env: ${name#/} is unhealthy")
  done <<< "$now"
  printf '%s\n' "$now" > "$seen"
done

pct="${DISK_ALERT_PCT:-80}"
while read -r mount used; do
  (( ${used%\%} < pct )) || problems+=("disk: $mount at $used")
done < <(df -P / "$BACKUPS" 2> /dev/null | awk 'NR > 1 { print $6, $5 }' | sort -u)

if [[ -d "$BACKUPS/production" ]]; then
  newest="$(latest_dump production)"
  if [[ -z "$newest" || -z "$(find "$newest" -mmin -1560)" ]]; then
    problems+=("production: no dump in the last 26 h")
  fi
fi

if (( ${#problems[@]} )); then
  msg="$(printf '%s\n' "${problems[@]}")"
  echo "$msg"
  notify "$check" fail "$msg"
  exit 1
fi
echo "watch: all good"
notify "$check" ok
