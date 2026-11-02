# Decisions for the user: where CI and staging live

Today one VPS (4 cores, 15 GB RAM) runs production, staging, the CI runner and other projects (control360, genesy-backoffice). The walls are good: CI has its own rootless Docker, user, firewall, disk and a 3-core / 6 GB slice. Staging has lower CPU weight and memory ceilings. The ceilings still add up to about 14 GB before the other projects, and all of it shares one disk and one network. Prices below are rough, from memory; check them before deciding.

## 1. The CI runner off the production VPS

**Decided 2026-11-02: B.** All CI runs on GitHub-hosted runners; the VPS runner and `deploy/ci-runner/` are removed.

| Option | Rough cost | Risk | Work |
| --- | --- | --- | --- |
| **A. Second small VPS for CI** (4 vCPU / 8 GB class, e.g. Hetzner or the current host) | about €8–15 a month | Low. Production no longer shares CPU, disk I/O or RAM with builds, and a CI escape lands on a box with nothing on it. | `deploy/ci-runner/setup.sh` runs unchanged on the new box. Register it, then remove the old runner (README "Teardown"). About an hour. |
| B. GitHub-hosted runners | free minutes first, then per minute | Low for production. Slower builds (no warm cargo or pnpm cache on the runner unless cached in Actions). Rust release plus Playwright runs will likely use up the private-repo free minutes. | Change `runs-on` and add caching (ci-images/ci-speed area). |
| C. Stay as is | €0 | Medium. A heavy build still slows production's disk, and the box has one more thing that can fill it. | None. |

**Recommendation: A**, ideally the same box as staging (below), so it's one extra server, not two.

## 2. Staging on its own server

| Option | Rough cost | Risk | Work |
| --- | --- | --- | --- |
| **A. Staging and CI together on one second VPS** (4 vCPU / 8 GB) | about €8–15 a month total | Low. Production's box then holds only production, Traefik and the other projects. A staging mistake (`down -v`, a runaway container, a full disk) can't touch production. | Traefik (or Caddy) on the new box. Point the `staging-*` and `staging` DNS records to it; Let's Encrypt issues new certificates within minutes. Copy `deploy/staging/secrets` over, or make fresh ones (staging data is demo data: reseed). Install the ops kit there for staging only. About half a day. |
| B. Staging alone on its own small VPS (2 vCPU / 4 GB) | about €5–8 a month | Low, as A. | As A, plus CI stays or moves separately. |
| C. Stay as is | €0 | Medium. Limits protect production's CPU and memory, but not its disk or Docker daemon. A staging `docker system prune -a` by hand would take production's previous images with it. | None. |

**Recommendation: 2A.** One box for everything non-production. It costs about one lunch a month and takes CI and staging off the box that holds customer data.

## What it changes here

- `deploy/ops/setup.sh` installs both envs' timers. On a split, each box keeps only its own (disable the other env's timers there).
- `deploy.sh` and the composes stay as they are: they already take the env name and find their checkout under `/srv`.
- The staging limits could then grow. Production's could too, once CI's 6 GB ceiling is gone.
