# CI runner on the VPS (retired)

**Retired 2026-11-02:** CI moved to GitHub-hosted runners (root `README.md`, "CI runners and cost"). This kit only serves `.github/workflows/ci-selfhosted.yml`, a by-hand rollback. Both are deleted, and the VPS cleaned up, once hosted CI has been green twice.

`.github/workflows/ci.yml` runs on a self-hosted GitHub Actions runner on the VPS (`ssh genesy`). That machine also runs prod, so the runner is walled off from prod. A good comparison is a guest flat with its own front door, its own meter and no key to the main house.

## How it is walled off

| Risk | Guard |
|---|---|
| CI uses prod's Docker (that is root on the box) | User `ci-runner` is in no `docker`, `sudo`, `adm` or `deploy` group. It gets **its own rootless Docker** (a second, private Docker running as that user), which can't see prod's containers, volumes or networks. |
| Job code touches the host | The runner refuses any job without `container:` (`ACTIONS_RUNNER_REQUIRE_JOB_CONTAINER=true`). Jobs only see their checkout. |
| The runner mounts `/var/run/docker.sock` (prod's socket) into every job container, which GitHub hard-codes | The mount is harmless only because `ci-runner` is not in the `docker` group. **Never add it.** `verify.sh` checks this from inside a container. |
| CI reaches prod over the network (Postgres sits on 172.16.x) | An iptables owner match on `ci-runner` rejects this host's own addresses, 10/8, 172.16/12, 192.168/16 and link-local. DNS and the internet stay open. Rootless Docker also has host loopback disabled. |
| CI reads prod files or secrets | `/home/deploy` is `750`. The runner gets no secrets: the workflow uses only `GITHUB_TOKEN`, read-only. |
| CI fills the disk | Everything CI writes lives on an **80 GB file-backed disk** at `/srv/genesy-ci` (`nodev,nosuid`). The folder underneath is immutable, so nothing lands on prod's disk if the mount fails. Caches are capped: cargo target 10 GB, registry 3 GB, pnpm store 5 GB, Trivy database 1 GB, image build cache 12 GB. A daily timer prunes images and trims the build cache. |
| CI starves prod of CPU or RAM | `user-<uid>.slice`: at most 3 of 4 cores and 6 GB RAM, with low CPU/IO weight so prod wins when both are busy. |
| Fork PRs run code here | The workflow runs only on pushes to `main` and on PRs from this repo. The runner is registered to `COTIT/genesy` only. |
| Image builds touch prod | The `images` job builds release images on the CI user's rootless Docker (its socket is mounted into that one job only), inside the same CPU, RAM and disk limits. It runs on pushes to `main` only, never for PRs. |
| Two jobs at once | One runner means one job at a time. `concurrency` also cancels older runs of the same PR. |

Leftovers: the runner removes each job's containers and network itself. The job-started hook (`cleanup.sh`) also wipes anything a crashed job left before the next one starts. It runs before the job's containers exist, so it is safe; the whole rootless daemon belongs to CI. The completed hook is not used because it would run before the runner stops its own containers.

## What you do (once)

1. Make the VPS reachable (hotspot if needed). From the repo root on your laptop, run:
   ```bash
   scp -r deploy/ci-runner genesy:ci-runner-setup
   ```
2. Get a registration token, valid for 1 hour. Either use GitHub → COTIT/genesy → Settings → Actions → Runners → **New self-hosted runner** and copy the `--token` value, or run on your laptop:
   ```bash
   gh api -X POST repos/COTIT/genesy/actions/runners/registration-token --jq .token
   ```
3. On the VPS:
   ```bash
   sudo bash ~/ci-runner-setup/setup.sh
   ```
   It installs `uidmap` and `slirp4netns`, plus the runner's own libraries via its `installdependencies.sh`. It creates the CI disk, the user, the limits, the firewall, rootless Docker, the runner service and the prune timer. It asks for the token without showing it, then runs `verify.sh`. The last line must be `ALL CHECKS PASSED`. Re-running it is safe.
4. In GitHub → Settings → Actions → General, check that "Run workflows from fork pull requests" is **off** and that "Workflow permissions" is **Read repository contents**.
5. Remove the copied kit, which now lives in `/usr/local/lib/genesy-ci`:
   ```bash
   rm -rf ~/ci-runner-setup
   ```

## Checking the first green run

1. GitHub → Settings → Actions → Runners shows `genesy-vps` as **Idle**.
2. Push a branch and open a PR to `main`. In the Actions tab, workflow `ci` runs `api` and `web` one after the other. The first run is slow because it pulls images and fills the cargo and pnpm caches. Both must be green.
3. Watch it live on the VPS:
   ```bash
   sudo journalctl -fu "$(cat /srv/genesy-ci/home/actions-runner/.service)"
   ```
4. After the run, check that isolation still holds:
   ```bash
   sudo /usr/local/lib/genesy-ci/verify.sh
   ```

## Day 2

- CI disk use: `df -h /srv/genesy-ci`.
- CI Docker use:
  ```bash
  sudo -u ci-runner -H env DOCKER_HOST=unix:///run/user/$(id -u ci-runner)/docker.sock docker system df
  ```
- Prune now: `sudo systemctl start genesy-ci-prune.service`.
- Restart the runner: `sudo systemctl restart "$(cat /srv/genesy-ci/home/actions-runner/.service)"`.
- A job fails with "Jobs without a job container are forbidden on this runner": add `container:` to that job.
- `verify.sh` fails on DNS or github.com: add `"dns": ["1.1.1.1", "9.9.9.9"]` to `/srv/genesy-ci/home/.config/docker/daemon.json`, then run:
  ```bash
  sudo -u ci-runner -H env XDG_RUNTIME_DIR=/run/user/$(id -u ci-runner) systemctl --user restart docker
  ```

- Grow the CI disk (from 40 GB to 80 GB on a box set up before the image builds). Online, while the runner is idle:
  ```bash
  sudo fallocate -l 80G /var/lib/genesy-ci.img
  loopdev="$(findmnt -no SOURCE /srv/genesy-ci)"
  sudo losetup -c "$loopdev" && sudo resize2fs "$loopdev"
  df -h /srv/genesy-ci
  ```
- After changing a file in this folder, copy the kit again and re-run `setup.sh` (step 1 and 3 above); it installs the new `cleanup.sh` into `/usr/local/lib/genesy-ci`.

## Teardown

```bash
cd /srv/genesy-ci/home/actions-runner
sudo ./svc.sh stop && sudo ./svc.sh uninstall
# remove the runner in GitHub (Settings > Actions > Runners), or use a removal token with:
#   sudo -u ci-runner ./config.sh remove --token <removal token>
CI_UID=$(id -u ci-runner)
sudo systemctl disable --now genesy-ci-prune.timer genesy-ci-firewall.service
sudo iptables -D OUTPUT -m owner --uid-owner ci-runner -j GENESY-CI
sudo ip6tables -D OUTPUT -m owner --uid-owner ci-runner -j GENESY-CI
sudo loginctl disable-linger ci-runner
sudo rm -rf /usr/local/lib/genesy-ci /etc/systemd/system/genesy-ci-* \
  "/etc/systemd/system/user-$CI_UID.slice.d" "/etc/systemd/system/user@$CI_UID.service.d"
sudo userdel ci-runner
sudo umount /srv/genesy-ci && sudo chattr -i /srv/genesy-ci
sudo sed -i '\|^/var/lib/genesy-ci.img |d' /etc/fstab
sudo rm -f /var/lib/genesy-ci.img && sudo systemctl daemon-reload
```
