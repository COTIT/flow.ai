#!/usr/bin/env bash
# Proves the CI runner can't reach prod. Run as root after setup and after the first CI run:
#   sudo /usr/local/lib/genesy-ci/verify.sh
# Prints PASS/FAIL per check; exits non-zero if any check fails.
set -uo pipefail

CI_USER="${CI_USER:-ci-runner}"
CI_ROOT="${CI_ROOT:-/srv/genesy-ci}"
PROD_HOME="${PROD_HOME:-/home/deploy}"
RUNNER_DIR="$CI_ROOT/home/actions-runner"

[[ $EUID -eq 0 ]] || { echo "run with sudo" >&2; exit 1; }
uid="$(id -u "$CI_USER")" || exit 1
public_ip="$(hostname -I | awk '{print $1}')"
fails=0

as_ci() {
  sudo -u "$CI_USER" -H env DOCKER_HOST="unix:///run/user/$uid/docker.sock" \
    XDG_RUNTIME_DIR="/run/user/$uid" "$@"
}
# Run bash inside a throwaway job-like container on the CI user's Docker.
in_job() { as_ci docker run --rm debian:trixie-slim bash -c "$1"; }
tcp() { echo "timeout 5 bash -c 'exec 3<>/dev/tcp/$1/$2'"; }

# ok "<what must be true>" cmd...    the command must succeed
# no "<what must be true>" cmd...    the command must fail
ok() { local what="$1"; shift; if "$@" > /dev/null 2>&1; then echo "PASS  $what"; else echo "FAIL  $what"; fails=$((fails + 1)); fi; }
no() { local what="$1"; shift; if "$@" > /dev/null 2>&1; then echo "FAIL  $what"; fails=$((fails + 1)); else echo "PASS  $what"; fi; }

privileged_group() { id -nG "$CI_USER" | tr ' ' '\n' | grep -qxE 'root|docker|sudo|adm|deploy|lxd'; }
rootless() { as_ci docker info -f '{{.SecurityOptions}}' | grep -q rootless; }
data_on_ci_disk() { [[ "$(as_ci docker info -f '{{.DockerRootDir}}')" == "$CI_ROOT"/* ]]; }
own_disk() { findmnt -n -o OPTIONS "$CI_ROOT" | grep -q nosuid; }
memory_capped() { [[ "$(systemctl show "user-$uid.slice" -p MemoryMax --value)" != infinity ]]; }
firewall_loaded() { iptables -w -C OUTPUT -m owner --uid-owner "$CI_USER" -j GENESY-CI; }
job_container_required() { grep -qx 'ACTIONS_RUNNER_REQUIRE_JOB_CONTAINER=true' "$RUNNER_DIR/.env"; }
runner_active() { systemctl is-active --quiet "$(cat "$RUNNER_DIR/.service")"; }

echo "== user and files"
no "$CI_USER is in no privileged group (docker, sudo, adm, deploy...)" privileged_group
no "$CI_USER cannot list $PROD_HOME" as_ci ls "$PROD_HOME"
no "$CI_USER cannot list $PROD_HOME/shared" as_ci ls "$PROD_HOME/shared"
ok "CI disk is its own nosuid mount at $CI_ROOT" own_disk

echo "== docker"
no "$CI_USER cannot use prod's Docker socket" as_ci docker -H unix:///var/run/docker.sock info
ok "$CI_USER has its own rootless Docker" rootless
ok "its Docker data lives on the CI disk" data_on_ci_disk
ok "test images pulled" as_ci docker pull -q debian:trixie-slim
ok "test images pulled (docker cli)" as_ci docker pull -q docker:29-cli
# The runner mounts /var/run/docker.sock into every job container (actions/runner ContainerInfo.cs).
no "a job container cannot use the mounted prod socket" \
  as_ci docker run --rm -v /var/run/docker.sock:/var/run/docker.sock docker:29-cli docker info

echo "== network"
ok "a job container reaches github.com:443 (DNS + internet)" in_job "$(tcp github.com 443)"
no "a job container cannot reach the host via slirp (10.0.2.2:22)" in_job "$(tcp 10.0.2.2 22)"
no "a job container cannot reach the host's public IP ($public_ip:22)" in_job "$(tcp "$public_ip" 22)"
no "a job container cannot reach prod's Docker gateway (172.16.0.1:22)" in_job "$(tcp 172.16.0.1 22)"
no "$CI_USER cannot reach host loopback (127.0.0.1:22)" as_ci bash -c "$(tcp 127.0.0.1 22)"
no "$CI_USER cannot reach prod's Docker gateway (172.16.0.1:22)" as_ci bash -c "$(tcp 172.16.0.1 22)"
traefik_ip="$(docker ps -q --filter name=traefik | head -1 | xargs -r docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}' | awk '{print $1}')"
if [[ -n "$traefik_ip" ]]; then
  no "a job container cannot reach prod's traefik container ($traefik_ip:80)" in_job "$(tcp "$traefik_ip" 80)"
fi
ok "egress firewall chain is loaded" firewall_loaded

echo "== limits and runner"
ok "memory cap on user-$uid.slice" memory_capped
ok "runner refuses jobs without a container" job_container_required
ok "runner service is active" runner_active

if (( fails == 0 )); then
  echo "ALL CHECKS PASSED"
else
  echo "$fails CHECK(S) FAILED"
  exit 1
fi
