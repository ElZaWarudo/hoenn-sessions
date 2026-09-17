# Production releases

Each deployed release corresponds to exactly one `main` commit: one ROM, one
`bridge_manifest.json`, one sidecar build, one server image, one VPS release
directory. All compilation happens in GitHub Actions; the VPS never builds
anything.

## How it works

`.github/workflows/deploy.yml` runs on pushes to `main` (or manually via
`workflow_dispatch`; never on pull requests). After the `validate` job
(`cargo test` over the workspace) succeeds, the `release` job:

1. Builds the Emerald ROM once (`make`, outputs `pokeemerald.gba` /
   `pokeemerald.elf`, same flags as PR CI).
2. Generates `dist/bridge_manifest.json` from that exact ROM+ELF with
   `python tools/generate_bridge_manifest.py --elf pokeemerald.elf
   --rom pokeemerald.gba`. This generated file is authoritative; the copy
   committed in git is never substituted.
3. Builds `coop-sidecar.exe` for Windows x86_64
   (`--target x86_64-pc-windows-gnu`).
4. Builds `deploy/coop/Dockerfile` **after** step 2 (it copies the fresh
   manifest, which the server embeds) and pushes
   `ghcr.io/<owner>/hoenn-sessions-server:<full-commit-sha>` (plus `latest`
   for convenience only).
5. Captures the pushed image **digest** and deploys by digest
   (`name@sha256:<digest>`), never by tag: GHCR tags are mutable and a
   re-run could otherwise swap the image behind a promoted release.
6. Writes `release.json` + `SHA256SUMS`, copies everything to
   `/srv/hoenn/staging/<sha>/` on the VPS via SSH.
7. SSH: `promote-release.sh` verifies and atomically promotes the release
   (idempotent, so re-running after a failed rollout is safe), then
   `deploy-release.sh` pins digest-`COOP_IMAGE`, pulls, recreates only the
   `server` container (`--no-build`, no volume touches; postgres dependency
   honoured), and gates on `/health/ready` with a bounded retry. The
   workflow fails if health never returns 200. The ROM only ever exists on
   the runner and the VPS; it is never a public GitHub artifact.

Releases are serialised by a concurrency group, but GitHub cancels
superseded *queued* runs: an intermediate `main` commit may never get its
own run. Every *deployed release* still equals exactly one commit (`main`
is linear, so a later release contains earlier commits' code), but not
every commit is guaranteed its own release.

Re-running the workflow for the same commit is safe. It first asks the VPS
whether the commit is already live: if so, the rebuild and re-upload are
skipped, promotion re-verifies the released copy (a no-op when `current`
already points at it), and only the server rollout runs again with the
recorded digest. So a failed rollout is recovered with a plain re-run (no
manual steps, no conflicting second bundle). If the VPS is unreachable or
its live metadata is unreadable, the run fails loudly instead of rebuilding
blindly. A re-upload that genuinely differs from an already-released copy
under the same id is still rejected instead of silently replacing it.

## VPS layout

```text
/srv/hoenn/
├── staging/<sha>/      # in-flight upload; never served
├── releases/<sha>/     # immutable: game.gba, coop-sidecar.exe,
│                       # bridge_manifest.json, release.json, SHA256SUMS
├── current -> releases/<sha>   # relative symlink, switched atomically
└── .previous-release   # id active before the last promotion
```

Each release directory contains:

```text
game.gba
coop-sidecar.exe
bridge_manifest.json
release.json
SHA256SUMS
```

`release.json` looks like:

```json
{
  "version": "<commit sha>",
  "commit": "<full commit sha>",
  "rom": "game.gba",
  "sidecar": "coop-sidecar.exe",
  "manifest": "bridge_manifest.json",
  "server_image": "ghcr.io/<owner>/hoenn-sessions-server@sha256:<digest>",
  "server_image_tag": "ghcr.io/<owner>/hoenn-sessions-server:<commit sha>",
  "server_image_digest": "sha256:<digest>",
  "sidecar_sha256": "...",
  "manifest_sha256": "..."
}
```

`server_image` is the immutable deployment handle; `server_image_tag` is
kept for human readability only and must never be deployed by itself.

The canonical ROM hash lives in `bridge_manifest.json`
(`game_build.rom_sha256`) and is cross-checked against `game.gba` during
promotion; it is intentionally not duplicated in `release.json`. No secrets
are ever written to `release.json`.

## GitHub secrets / settings

| Secret | Purpose |
|---|---|
| `VPS_HOST` | VPS hostname/IP for SSH + scp |
| `VPS_USER` | SSH user (should own `/srv/hoenn` + docker rights) |
| `VPS_SSH_KEY` | Private deploy key (never logged; `chmod 600` in-runner) |
| `VPS_PORT` | Optional SSH port (default `22`) |
| `VPS_KNOWN_HOSTS` | **Required** pinned VPS host key (`ssh-keyscan` output captured once over a trusted network — never `StrictHostKeyChecking=no`, never an in-run keyscan) |
| `VPS_DEPLOY_DIR` | Absolute path of `deploy/coop` on the VPS (e.g. `/opt/hoenn-sessions/deploy/coop`); must not contain single quotes |
| `VPS_GHCR_USER` / `VPS_GHCR_TOKEN` | Read-only GHCR credentials (`read:packages`) so the VPS can `pull` the private server image; set together or not at all (`[A-Za-z0-9_.-]` user; the token travels over the SSH channel on stdin, never in a command line) |

The workflow needs `packages: write` (push image); `GITHUB_TOKEN` covers it.
No other tokens are required.

## One-time VPS setup

Prerequisites on the VPS: Docker Engine with Compose v2, `bash`, `python3`,
GNU coreutils (`sha256sum`, `awk`, `mv -T`) and `diffutils` (`diff -qr`)
for the promotion script.

```sh
# 1. Follow deploy/coop/README.md for the base stack (secrets, .env, volumes).
# 2. Keep a repo checkout for the deploy scripts (or copy the two scripts):
git clone https://github.com/ElZaWarudo/hoenn-sessions.git /opt/hoenn-sessions
# 3. Release store owned by the SSH user:
sudo mkdir -p /srv/hoenn/staging /srv/hoenn/releases
sudo chown -R "$USER" /srv/hoenn
# 4. GHCR read-only login (fine-grained PAT, read:packages only):
docker login ghcr.io -u <user>
# 5. Authorize the deploy key: append its public half to ~<VPS_USER>/.ssh/authorized_keys.
# 6. Pin the host key over a trusted network and store it as VPS_KNOWN_HOSTS:
ssh-keyscan -p <port> <vps-host>
# 7. Add the GitHub secrets above (VPS_DEPLOY_DIR=/opt/hoenn-sessions/deploy/coop).
```

## Normal deployment

Just merge to `main`. The workflow uploads, promotes, rolls the server and
health-checks automatically. Watch it in Actions; on success:

```sh
readlink /srv/hoenn/current            # -> releases/<new-sha>
cat /srv/hoenn/current/release.json
docker compose --project-directory "$VPS_DEPLOY_DIR" ps server
```

## Manual deploy / re-promote

```sh
HOENN_ROOT=/srv/hoenn bash deploy/coop/promote-release.sh <full-sha>
GHCR_USER=... GHCR_TOKEN=... bash deploy/coop/deploy-release.sh \
  --image 'ghcr.io/<owner>/hoenn-sessions-server@sha256:<digest>' \
  --deploy-dir /opt/hoenn-sessions/deploy/coop
```

Only digest-pinned references are accepted — tags are never deployed, not
even full-SHA ones, because GHCR tags are mutable and a pipeline re-run
overwrites them. Pre-check without touching anything with
`--validate-only`. Re-running promotion for an already-promoted release is
a safe no-op (a differing re-upload under the same id is rejected
instead), so a failed rollout can be retried from the server step alone.

## Rollback

Client files (atomic: symlink built aside, then renamed over `current`, so
readers never see a missing link):

```sh
ln -sfn /srv/hoenn/releases/<previous-sha> /srv/hoenn/current.tmp
mv -Tf /srv/hoenn/current.tmp /srv/hoenn/current
# previous id hint: cat /srv/hoenn/.previous-release; ls /srv/hoenn/releases
```

Server (keeps volumes, secrets, Caddy untouched; never `down -v`):

```sh
bash deploy/coop/deploy-release.sh \
  --image 'ghcr.io/<owner>/hoenn-sessions-server@sha256:<previous-digest>' \
  --deploy-dir /opt/hoenn-sessions/deploy/coop
```

Old releases and the previous image are deliberately retained; nothing is
auto-deleted. Released files are made read-only by convention (ownership
remains the real enforcer). Database state is never rolled back
automatically: a persistence-format change may need its matching backup
(see README.md).

## Verify the deployed version

```sh
cat /srv/hoenn/current/release.json
(cd /srv/hoenn/current && sha256sum -c SHA256SUMS)
python3 -c 'import json; m=json.load(open("/srv/hoenn/current/bridge_manifest.json")); print(m["game_build"]["rom_sha256"])'
sha256sum /srv/hoenn/current/game.gba   # must match the line above
docker inspect --format='{{.Config.Image}}' "$(docker compose ps -q server)"
curl -fsS https://<domain>/health/ready
```

## Downloading current client files

Serve `/srv/hoenn/current` privately through the existing Caddy (never the
staging directories, never without auth):

1. `docker compose exec caddy caddy hash-password` → put the output in
   `.env` as `COOP_DOWNLOAD_HASH` plus `COOP_DOWNLOAD_USER`. The bcrypt
   hash contains `$`: double each one as `$$` in `.env` so Compose passes
   it through literally.
2. Uncomment the `/srv/hoenn` **parent** mount in `compose.yaml` (never the
   `current` symlink itself: Docker resolves symlinks at container
   creation, which would pin downloads to one release). The parent mount
   lets every request resolve the live `current` target, so promotions need
   no caddy restart. Only `/download/*` is served; staging directories and
   `..` escapes are never reachable.
3. Paste the `handle /download/*` block from the Caddyfile comments inside
   the site block and recreate caddy once (`docker compose up -d caddy`).
   Missing credentials fail closed at startup.
4. Fetch (example): `curl -u user:pass
   https://<domain>/download/game.gba`, `/download/coop-sidecar.exe`,
   `/download/bridge_manifest.json`, `/download/release.json`.
