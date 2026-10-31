#!/usr/bin/env bash
# Egress firewall for the CI user. The internet stays open; this host's own addresses
# (loopback, public IP, Docker gateways), prod's container networks and other private
# ranges are refused. Covers the runner and its rootless Docker, whose container traffic
# leaves through slirp4netns running as the same user.
# Run as root at boot by genesy-ci-firewall.service. It rebuilds the chain, so re-runs are safe.
set -Eeuo pipefail

CI_USER="${CI_USER:-ci-runner}"
CHAIN=GENESY-CI

# $1 = iptables | ip6tables, then the rules for the chain (one per line, appended in order).
load() {
  local ipt="$1" rule
  "$ipt" -w -N "$CHAIN" 2> /dev/null || "$ipt" -w -F "$CHAIN"
  while read -r rule; do
    [[ -n "$rule" ]] || continue
    # shellcheck disable=SC2086 # each line is a list of iptables arguments
    "$ipt" -w -A "$CHAIN" $rule
  done <<< "$2"
  "$ipt" -w -C OUTPUT -m owner --uid-owner "$CI_USER" -j "$CHAIN" 2> /dev/null \
    || "$ipt" -w -I OUTPUT 1 -m owner --uid-owner "$CI_USER" -j "$CHAIN"
}

# systemd-resolved is the host's DNS (127.0.0.53/54); everything else local is off limits.
load iptables "
-d 127.0.0.53/32 -p udp --dport 53 -j RETURN
-d 127.0.0.53/32 -p tcp --dport 53 -j RETURN
-d 127.0.0.54/32 -p udp --dport 53 -j RETURN
-d 127.0.0.54/32 -p tcp --dport 53 -j RETURN
-m addrtype --dst-type LOCAL -j REJECT
-d 10.0.0.0/8 -j REJECT
-d 172.16.0.0/12 -j REJECT
-d 192.168.0.0/16 -j REJECT
-d 169.254.0.0/16 -j REJECT
-d 100.64.0.0/10 -j REJECT
"

load ip6tables "
-m addrtype --dst-type LOCAL -j REJECT
-d fc00::/7 -j REJECT
-d fe80::/10 -j REJECT
"

echo "genesy-ci firewall loaded for user $CI_USER"
