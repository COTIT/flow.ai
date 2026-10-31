#!/usr/bin/env bash
# Sets up genesy's GitHub Actions runner on the prod VPS, walled off from prod.
# Copy this folder to the VPS and run:  sudo bash ~/ci-runner-setup/setup.sh
# Safe to re-run: finished steps are skipped or rewritten with the same content.
# What each part does and why: README.md next to this file.
set -Eeuo pipefail

REPO="COTIT/genesy"
CI_USER="ci-runner"
CI_ROOT="/srv/genesy-ci"
CI_IMG="/var/lib/genesy-ci.img"
CI_DISK_GB="${CI_DISK_GB:-40}"
RUNNER_VERSION="2.338.2"
RUNNER_SHA256="baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40"
RUNNER_NAME="genesy-vps"
RUNNER_LABELS="genesy-ci"
LIB="/usr/local/lib/genesy-ci"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

say() { printf '\n==> %s\n' "$*"; }
die() { echo "ERROR: $*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || die "run with sudo"
[[ "$(uname -m)" == x86_64 ]] || die "the runner checksum below is for x86_64"
command -v dockerd-rootless-setuptool.sh > /dev/null || die "docker-ce-rootless-extras is not installed"
for f in cleanup.sh firewall.sh verify.sh; do [[ -f "$HERE/$f" ]] || die "missing $HERE/$f"; done

say "Packages for rootless Docker (uidmap, slirp4netns)"
apt-get install -y --no-install-recommends uidmap slirp4netns > /dev/null

say "Fixed-size CI disk: ${CI_DISK_GB} GB file mounted at $CI_ROOT (CI can never fill prod's disk)"
if ! findmnt -n "$CI_ROOT" > /dev/null; then
  if [[ ! -f "$CI_IMG" ]]; then
    avail_gb="$(df --output=avail -BG "$(dirname "$CI_IMG")" | tail -1 | tr -dc 0-9)"
    (( avail_gb >= CI_DISK_GB + 20 )) || die "need $((CI_DISK_GB + 20)) GB free for the CI disk, have $avail_gb GB"
    fallocate -l "${CI_DISK_GB}G" "$CI_IMG"
    chmod 600 "$CI_IMG"
    mkfs.ext4 -q -L genesy-ci "$CI_IMG"
  fi
  mkdir -p "$CI_ROOT"
  # Immutable while unmounted: nothing can land on prod's disk if the mount ever fails.
  chattr +i "$CI_ROOT"
  # nofail: a broken CI disk must never stop prod from booting.
  grep -q "^$CI_IMG " /etc/fstab || echo "$CI_IMG $CI_ROOT ext4 loop,nodev,nosuid,nofail 0 0" >> /etc/fstab
  systemctl daemon-reload
  mount "$CI_ROOT"
fi

say "User $CI_USER (no password, no ssh, no privileged groups)"
if ! id "$CI_USER" > /dev/null 2>&1; then
  useradd --create-home --home-dir "$CI_ROOT/home" --shell /usr/sbin/nologin --user-group "$CI_USER"
fi
passwd -l "$CI_USER" > /dev/null
chmod 700 "$CI_ROOT/home"
uid="$(id -u "$CI_USER")"
# Rootless Docker maps container users onto this range; keep clear of deploy's 100000-165535.
grep -q "^$CI_USER:" /etc/subuid || usermod --add-subuids 300000-365535 "$CI_USER"
grep -q "^$CI_USER:" /etc/subgid || usermod --add-subgids 300000-365535 "$CI_USER"
if id -nG "$CI_USER" | tr ' ' '\n' | grep -qxE 'root|docker|sudo|adm|deploy|lxd'; then
  die "$CI_USER is in a privileged group: remove it (gpasswd -d $CI_USER <group>) and re-run"
fi

say "Scripts to $LIB (root-owned: CI can't change its own hooks)"
install -d -m 755 "$LIB"
install -m 755 "$HERE/cleanup.sh" "$HERE/firewall.sh" "$HERE/verify.sh" "$LIB/"

say "CPU, memory and IO limits for everything $CI_USER runs"
# 4 cores / 15 GB box: CI gets at most 3 cores and 6 GB, and yields to prod when both are busy.
install -d "/etc/systemd/system/user-$uid.slice.d"
cat > "/etc/systemd/system/user-$uid.slice.d/genesy-ci.conf" << 'EOF'
[Slice]
CPUQuota=300%
CPUWeight=20
IOWeight=20
MemoryHigh=5G
MemoryMax=6G
TasksMax=4096
EOF

say "Egress firewall for $CI_USER (internet yes; this host, prod containers, private ranges no)"
cat > /etc/systemd/system/genesy-ci-firewall.service << EOF
[Unit]
Description=genesy CI egress firewall ($CI_USER may not reach this host or private networks)
After=network-pre.target
Before=network.target user@$uid.service

[Service]
Type=oneshot
RemainAfterExit=yes
Environment=CI_USER=$CI_USER
ExecStart=$LIB/firewall.sh

[Install]
WantedBy=multi-user.target
EOF
# The CI user's own systemd (and so its Docker) only starts with the firewall and the CI disk in place.
install -d "/etc/systemd/system/user@$uid.service.d"
cat > "/etc/systemd/system/user@$uid.service.d/genesy-ci.conf" << EOF
[Unit]
Requires=genesy-ci-firewall.service
After=genesy-ci-firewall.service
RequiresMountsFor=$CI_ROOT
EOF
systemctl daemon-reload
systemctl enable --now genesy-ci-firewall.service > /dev/null
systemctl restart genesy-ci-firewall.service

say "Rootless Docker for $CI_USER (its own daemon; prod's containers, volumes and networks are invisible)"
loginctl enable-linger "$CI_USER"
for _ in {1..30}; do [[ -S "/run/user/$uid/bus" ]] && break; sleep 1; done
[[ -S "/run/user/$uid/bus" ]] || die "systemd user session for $CI_USER did not start"
as_ci() {
  sudo -u "$CI_USER" -H env XDG_RUNTIME_DIR="/run/user/$uid" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" "$@"
}
cfg="$CI_ROOT/home/.config"
as_ci mkdir -p "$cfg/docker" "$cfg/systemd/user/docker.service.d"
as_ci tee "$cfg/docker/daemon.json" > /dev/null << 'EOF'
{
  "log-driver": "local",
  "log-opts": { "max-size": "10m", "max-file": "3" },
  "no-new-privileges": true
}
EOF
as_ci tee "$cfg/systemd/user/docker.service.d/genesy-ci.conf" > /dev/null << 'EOF'
[Service]
Environment=DOCKERD_ROOTLESS_ROOTLESSKIT_NET=slirp4netns
Environment=DOCKERD_ROOTLESS_ROOTLESSKIT_DISABLE_HOST_LOOPBACK=true
EOF
as_ci dockerd-rootless-setuptool.sh install > /dev/null
as_ci systemctl --user daemon-reload
as_ci systemctl --user enable docker > /dev/null 2>&1
as_ci systemctl --user restart docker
docker_host="unix:///run/user/$uid/docker.sock"
for _ in {1..30}; do as_ci docker -H "$docker_host" info > /dev/null 2>&1 && break; sleep 1; done
as_ci docker -H "$docker_host" info -f '{{.SecurityOptions}}' | grep -q rootless \
  || die "rootless Docker for $CI_USER is not answering on $docker_host"

say "GitHub Actions runner $RUNNER_VERSION for $REPO (label $RUNNER_LABELS)"
runner_dir="$CI_ROOT/home/actions-runner"
if [[ ! -f "$runner_dir/.runner" ]]; then
  tarball="$(mktemp)"
  trap 'rm -f "$tarball"' EXIT
  curl -fsSL --retry 5 -o "$tarball" \
    "https://github.com/actions/runner/releases/download/v$RUNNER_VERSION/actions-runner-linux-x64-$RUNNER_VERSION.tar.gz"
  echo "$RUNNER_SHA256  $tarball" | sha256sum -c --quiet - || die "runner download checksum mismatch"
  chmod 644 "$tarball"
  as_ci mkdir -p "$runner_dir"
  as_ci tar xzf "$tarball" -C "$runner_dir"
  "$runner_dir/bin/installdependencies.sh" > /dev/null

  echo "Paste the registration token (GitHub > $REPO > Settings > Actions > Runners > New self-hosted runner)."
  read -rsp "Token (hidden): " ACTIONS_RUNNER_INPUT_TOKEN
  echo
  [[ -n "$ACTIONS_RUNNER_INPUT_TOKEN" ]] || die "empty token"
  export ACTIONS_RUNNER_INPUT_TOKEN
  # Token goes in through the environment (runner reads ACTIONS_RUNNER_INPUT_*), never argv.
  (cd "$runner_dir" && sudo --preserve-env=ACTIONS_RUNNER_INPUT_TOKEN -u "$CI_USER" -H \
    ./config.sh --unattended --replace --url "https://github.com/$REPO" \
    --name "$RUNNER_NAME" --labels "$RUNNER_LABELS" --work _work)
  unset ACTIONS_RUNNER_INPUT_TOKEN
fi

# Jobs talk to the CI user's Docker, must run in a container, and start from a clean daemon.
for line in "DOCKER_HOST=$docker_host" \
  "ACTIONS_RUNNER_REQUIRE_JOB_CONTAINER=true" \
  "ACTIONS_RUNNER_HOOK_JOB_STARTED=$LIB/cleanup.sh"; do
  grep -qxF "$line" "$runner_dir/.env" || echo "$line" | as_ci tee -a "$runner_dir/.env" > /dev/null
done

if [[ ! -f "$runner_dir/.service" ]]; then
  (cd "$runner_dir" && ./svc.sh install "$CI_USER" > /dev/null)
fi
runner_unit="$(cat "$runner_dir/.service")"
install -d "/etc/systemd/system/$runner_unit.d"
cat > "/etc/systemd/system/$runner_unit.d/genesy-ci.conf" << EOF
[Unit]
Requires=genesy-ci-firewall.service user@$uid.service
After=genesy-ci-firewall.service user@$uid.service
RequiresMountsFor=$CI_ROOT

[Service]
Slice=user-$uid.slice
EOF

say "Daily prune of CI images and build cache"
cat > /etc/systemd/system/genesy-ci-prune.service << EOF
[Unit]
Description=Prune genesy CI Docker images and build cache
Requires=user@$uid.service
After=user@$uid.service
RequiresMountsFor=$CI_ROOT

[Service]
Type=oneshot
User=$CI_USER
Slice=user-$uid.slice
Environment=DOCKER_HOST=$docker_host
Environment=XDG_RUNTIME_DIR=/run/user/$uid
ExecStart=$LIB/cleanup.sh prune
EOF
cat > /etc/systemd/system/genesy-ci-prune.timer << 'EOF'
[Unit]
Description=Daily prune of genesy CI Docker

[Timer]
OnCalendar=daily
RandomizedDelaySec=1h
Persistent=true

[Install]
WantedBy=timers.target
EOF
systemctl daemon-reload
systemctl enable --now genesy-ci-prune.timer > /dev/null
systemctl restart "$runner_unit"

say "Isolation checks"
"$LIB/verify.sh"
echo
echo "Done. The runner '$RUNNER_NAME' should show as Idle under GitHub > $REPO > Settings > Actions > Runners."
