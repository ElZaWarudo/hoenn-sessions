# Private-pilot releases

## Independent game channel

Android obtains the ROM and compatibility manifest from `/v1/releases/game/latest`.
This is a separate signed descriptor with platform `game`, the fixed
artifacts `rom` and `compatibility-manifest` plus the region catalog and every
world's signed files (see "Multi-world releases"), and its own monotonic sequence.
The server serves the immutable files from `game/<release-id>/` and selects a
generation through `game/current`. The Windows runtime and Android APK retain
their own release pointers. All three channels require bearer authentication.

The deploy workflow signs `game-bundle/<sha>/release-envelope.json` with the
same protected Ed25519 key and promotes it through `promote-game.sh` after
verifying the signature, exact inventory, artifact hashes, and sequence floor.
The game and APK artifact routes accept `Range: bytes=N-` for interrupted
downloads; metadata endpoints reject ranges. Android can still open a locally
installed Windows signed ROM generation during migration, but new downloads
use only the game channel.

The production workflow produces one immutable Windows runtime generation per
commit. `github.run_number` is the signed monotonic sequence; a release id is
the full lowercase commit SHA. Re-running a commit reuses its promoted
generation and image digest instead of creating conflicting metadata.

## What is signed and where it goes

The release tool uses the launcher’s schema-one types and fixed destinations.
Every generation contains these eleven fixed files, the multi-world files
described under "Multi-world releases", and `release-envelope.json`:

```text
releases/<sha>/
├── app/coop-launcher.exe
├── runtime/mgba.exe
├── runtime/game.gba
├── runtime/coop-sidecar.exe
├── bridge/main.lua
├── bridge/memory.lua
├── bridge/protocol.lua
├── bridge/generated_addresses.lua
├── bridge_manifest.json
├── trust/release-trust.json
└── THIRD_PARTY_NOTICES.txt
```

The envelope signs canonical descriptor bytes containing the schema, release
id, sequence, validity window (at most 90 days), platform, and the SHA-256
size/digest of each fixed identity. Promotion verifies the Ed25519 signature,
key id/public-key configuration, exact inventory, and every transported byte
before making the directory immutable.

The archive identity is pinned to:

```text
https://s3.amazonaws.com/mgba/build/mGBA-build-2026-09-19-win64-9139-3a5bc24629867576b0fb576a5d5a21d3b3d6b576.7z
archive sha256: ea7cc0e8632cd80d28bdb55e37aacc58b2b018f564209f790e8cc3caed8c002b
mGBA.exe sha256: 743157a16a1cb478a2b45e6e20e9a482ea397c3820d7e8e27b1e048e85bd5546
```

The ROM, mGBA, sidecar, signed envelope, and full runtime are private-pilot
transport only. They are never uploaded as GitHub Releases or workflow
artifacts. GitHub receives only the ROM-free MSI, `hashes.json`, and
`provenance.json`. The MSI contains the stable bootstrapper, desktop
onboarding fallback, nonsecret private-pilot config, and notices; mutable
runtime data remains outside the install root.

The signing seed is `HOENN_RELEASE_PRIVATE_SEED_HEX`, supplied to the signing
step through the protected environment. It must match the public key in the
protected `RELEASE_TRUST_PUBLIC_KEY_HEX` configuration. Never put the seed in
workflow arguments, logs, files, release directories, or GitHub artifacts.

## VPS layout and marker contract

```text
/srv/hoenn/
├── staging/<sha>/       # in-flight private SSH upload; never served
├── releases/<sha>/      # verified immutable runtime generation
├── release-metadata/<sha>.json # immutable release-to-image (+ server catalog) association
├── server-catalog/<catalog-sha>/ # promoted server build catalog + arrival saves
├── current              # regular file: <sha>\n, atomically replaced
└── .previous-release    # previous marker id for rollback
```

`current` is deliberately a regular bounded marker. The server reads it only
after authentication, resolves `releases/<id>`, and serves the signed
envelope or one fixed artifact. A legacy `current -> releases/<sha>` symlink
is accepted by `promote-release.sh` and `activate-release.sh` only to migrate
it atomically to the marker contract. Invalid markers, symlinks inside a generation, extra files,
missing artifacts, signature failures, and conflicting re-uploads fail closed.

Compose sets `COOP_RELEASE_ROOT=/srv/hoenn` and mounts the release parent
read-only into the server. Do not mount `current` itself: Docker resolves a
symlink at container creation and would pin the server to one generation.
The metadata file is outside the served eleven-artifact directory and has the
schema `{schema, release_id, image_ref, image_digest}` (schema 1) or, for
multi-world releases, schema 2 with an added `server_catalog_sha256`. Promotion validates
that `image_ref` ends in the exact `sha256:<64 lowercase hex>` digest, creates
the file without overwriting an existing association, and writes it before
moving staging into `releases/<sha>` or changing `current`; a failed move may
leave that validated orphan for a later matching retry. Status and retry resolve the requested
`releases/<sha>` plus its metadata; they never pair `current` with whichever
container happens to be running.

## Multi-world releases

Every production release contains every world registered in
`data/rom_worlds.json` (Main = world 1, Cormoria = world 2). The workflow runs
`tools/coop/multiworld_build_ci.py build` and `verify`, copies the main world's
`game.gba`, `bridge_manifest.json` and `generated_addresses.lua` to
`pokeemerald.gba`, `dist/bridge_manifest.json` and
`bridge/generated_addresses.lua` (read by the server image and the Android
build) and checks they are byte-identical, then runs
`tools/coop/assemble_release_catalog.py assemble`. The assembler joins
`data/rom_worlds.json`, `data/rom_world_release.json` (portals, presence
regions, location sections, arrival portals, save namespaces) and
`data/release_arrivals.json`, writes the canonical client region catalog
`release_catalog.json`, validates it with `tools/rom_release_catalog.py`
(including the server's V2 arrival-save parser) and derives the schema-3
`server-build-catalog.json`.

Signing lists come from the validated region catalog: the eleven fixed
artifacts, then `region-catalog`, then `world-<N>-rom`,
`world-<N>-compatibility` and `world-<N>-player-transfer` in ascending world
id. The game channel signs `rom`, `compatibility-manifest` and the same
dynamic list. Both envelopes are verified locally (`verify`, `verify-game`)
before upload. The extra signed files live at `release_catalog.json` and
`worlds/<N>/{game.gba,bridge_manifest.json,player_transfer.json}` in both
`releases/<sha>/` and `game/<sha>/`. `promote-release.sh` and
`promote-game.sh` derive the exact inventory from the signed descriptor and
cross-check every region-catalog world entry against its signed files; the old
fixed eleven-file and two-file sets still verify for reruns of old releases.

The server catalog is promoted separately, content-addressed by its digest:

```text
/srv/hoenn/
├── server-catalog-staging/<catalog-sha>/   # private upload
└── server-catalog/<catalog-sha>/           # read-only, uid 10001 can read
    ├── server-build-catalog.json           # <= 64 KiB, sha256 == <catalog-sha>
    └── worlds/<N>/arrival.sav              # exactly the saves it pins
```

Rollout order: `promote-server-catalog.sh <catalog-sha>`, `promote-game.sh
<sha> --no-flip`, `promote-release.sh <sha> ... --server-catalog-sha256
<catalog-sha> --no-flip` (the image association, metadata schema 2 with
`server_catalog_sha256`, is the last promotion written), then
`deploy-release.sh --image <ref> --catalog-path
/srv/hoenn/server-catalog/<catalog-sha>/server-build-catalog.json
--catalog-sha256 <catalog-sha>`, and only after a healthy server
`activate-release.sh <sha> --deploy-dir <dir>` flips `game/current`, `current`
(recording `.previous-release`) and, when the release carries a newer APK,
`android/current`. Nothing is visible to players before activation.
Activation preflights everything before the first rename: both generations
are promoted, every marker (and its `.tmp` sibling) is absent or a regular
file (a legacy `current` symlink is migrated), the Android decision is made,
and `<dir>/.env` carries exactly one `COOP_IMAGE` and one
`COOP_PHASE2_RELEASE_CATALOG_SHA256` equal to `release-metadata/<sha>.json`
(`image_ref`, `server_catalog_sha256`). On any mismatch it flips nothing: the
healthy server is not the release being activated.
`probe-release-status.sh` prints a fourth line, `server_catalog_sha256=`, so a
rerun of a `PENDING`/`RELEASED` commit redeploys the recorded catalog; a
schema-1 association (pre multi-world) cannot be redeployed by this workflow.

`deploy-release.sh` validates the arguments and hashes the catalog on the host,
takes a deploy lock, pulls the image (a pull failure changes nothing), writes
`.env.next` preserving every other line, validates it with the candidate
compose file, keeps timestamped mode-0600 `.env.bak.<ts>` and
`compose.yaml.bak.<ts>` (they contain secrets; only the newest
`DEPLOY_BACKUP_KEEP`, default 5, of each are kept), renames both into place,
runs `up -d` and waits for `/health/ready`. On failure or interruption (also
SIGHUP/SIGINT/SIGTERM after the swap) it restores both files byte-for-byte
with their original modes and brings the previous server back, or with
`--rollback-to-stopped` leaves it stopped. The rollback ignores SIGPIPE and
further signals, runs with errexit off and writes everything to a log
(`DEPLOY_LOG`, else `<deploy-dir>/deploy-rollback.<ts>.log`), so a dead SSH
channel cannot abort it. There is no separate catalog preflight; startup
validation plus the health check is the gate.

| Exit | Meaning | Production |
|---|---|---|
| 0 | new server healthy | new release serving; activation follows |
| 1 | refused before any change (lock held, pull failed, invalid candidate config, missing file) | unchanged |
| 2 | usage error | unchanged |
| 3 | rollout failed; files restored; previous server healthy again | previous image serving |
| 4 | rollout failed and the rollback did not complete | **possibly down**: operator now |
| 5 | rollout failed; files restored; server left **stopped** (`--rollback-to-stopped`) | down by design |

The workflow never runs `deploy-release.sh` on the SSH channel itself:
`deploy-detached.sh start` launches it under `setsid nohup` (stdin
`/dev/null`) with output in `/var/log/hoenn/deploy-<sha>-<run>-<attempt>.log`
and the exit status written to the matching `.status` file, and the workflow
polls `deploy-detached.sh wait` (bounded: 30 minutes, step timeout 35) while
streaming the log. Lost connections are retried; a cancelled run or dropped
SSH session leaves the rollout and any rollback running to completion. The
step reports 1, 3, 4 (`PRODUCTION POSSIBLY DOWN`), 5 and "state unknown"
(124, the deploy is still running detached) as distinct errors and never
activates after a failure. `/var/log/hoenn` must exist and be writable by the
SSH user (`sudo install -d -m 0750 -o <ssh-user> /var/log/hoenn`).

### Attested arrival saves and recertification

First-arrival saves are reviewed `.sav` files under `data/release_arrivals/`.
`data/release_arrivals.json` binds each one to the exact ROM SHA-256 it was
saved with, its own SHA-256 and a receipt. CI never regenerates them: when a
built ROM differs from the attested hash the assembler stops with
`recertify arrival saves` (exit 3) and nothing is signed. To recertify:

1. Build the exact release ROMs (the PR job `release-dryrun` prints the CI
   hashes and fails when they differ from the attestations). Build from a
   clean clone with `python3 tools/coop/multiworld_build_ci.py build`. The
   Makefile sorts source lists bytewise, so the link order no longer follows
   the builder's locale. Before that fix, an `en_US.UTF-8` WSL shell linked
   `pokemon_animation.o` before `pokemon.o` and produced different ROM hashes
   from CI (`C.UTF-8`), even with identical sources and toolchain.
2. For each changed world run, on Windows with the pinned mGBA build:

   ```sh
   python tools/coop/recert_arrival.py --mgba <pinned mGBA.exe> \
     --rom <world>.gba --rom-sha256 <new rom sha> \
     --save data/release_arrivals/<world>.sav --save-sha256 <attested sav sha> \
     --downs 4 --world main --portal from_cormoria \
     --out <empty run dir> --verify-cold-reload
   ```

   (`--downs 5 --world cormoria --portal from_main` for Cormoria.)
3. Inspect the screenshots (SAVE selected, "saved the game" in the arrival
   town; cold reload shows the START menu in the arrival map), copy the new
   save over `data/release_arrivals/<world>.sav`, merge the printed
   attestation into `data/release_arrivals.json`, and get it reviewed.

### Object catalog digest (shared object contract fingerprint v1)

Each region-catalog world carries `object_catalog_sha256`. It is no longer a
configured constant: `data/rom_world_release.json` declares only
`"object_catalog": {"source": "shared-object-contract-fingerprint", "version": 1}`,
and `tools/coop/assemble_release_catalog.py` derives the digest from each
world's freshly built manifests. The assembler refuses any other
`object_catalog` entry, including a configured `sha256`. The old
`--test-only-provisional-object-catalog` flag and its placeholder digest are
gone; the PR dry-run (`release-dryrun.sh`) assembles exactly as production
does.

**Definition.** SHA-256 over the canonical JSON (sorted keys, `,`/`:`
separators, ASCII, one trailing LF) of
`{"schema": "hoenn-sessions/shared-object-contract-fingerprint", "version": 1, "contracts": {...}}`.
`contracts` holds only the world-independent content of the three cross-ROM
contracts that `multiworld_build_ci.py verify` already requires to agree:

| Contract | Source (schema) | Included | Excluded (per world) |
| --- | --- | --- | --- |
| `experience_table` | `experience_table_manifest.json` (1) | symbol, size, SHA-256 of `gExperienceTables` (re-hashed from the built ROM) | `rom_sha256`, `address` |
| `object_scalar` | `object_scalar_manifest.json` (4) | scope; descriptor size and digest; count-probe size; per table (items, species, moves, abilities, TM/HM move IDs): size, record count and stride, pointer and text offsets, pointer-free scalar digest, pointer-presence digest, bounded display-text digest; move AdditionalEffect stride, count field and digest | `rom_sha256`, every `address`, `raw_sha256` (embeds ROM pointers) |
| `player_transfer` | `player_transfer_manifest.json` (3) | symbol, schema version, size, descriptor size and digest, field count, fields, saved spans, re-key and Day Care custody field IDs | `rom_sha256`, `address` |

Every manifest must name the built ROM. Each world's document is computed
separately and must be byte-identical across all worlds; otherwise assembly
stops before anything is written, naming the differing contract, because a
world whose shared objects cannot be represented identically is not
travel-compatible. `rom_release_catalog.validate_catalog` re-verifies the
object scalar manifest against the ROM and requires one digest across worlds.
The canonical document is written to `object-catalog-fingerprint.json` next to
`assembly.json` for review.

**Not covered.** Graphics, palettes, icons and cries; callbacks and other
function pointers; field and battle scripts; menus and UI; bag/PC capacity
beyond the saved spans; and full object semantics. Equal fingerprints mean the
covered bytes agree, not that every shared object behaves identically.

**Changing it.** Every manifest field is classified as included or excluded.
A new or missing field, or another generator schema version, makes assembly
refuse. Adding or removing coverage, or reclassifying a field, changes what
the digest means and requires bumping `FINGERPRINT_VERSION` in the assembler
together with `object_catalog.version` in `data/rom_world_release.json`.

### First multi-world rollout

The first multi-world release starts existing players fresh. The data steps
(backup, the server's `fresh-start` admin subcommand, its confirmation gates
and its own rollback) live in [FRESH_START.md](FRESH_START.md); this is the
surrounding release sequence. The workflow never runs the fresh start: a
repository-variable hold stops it after promotion so the operator can do the
maintenance window by hand, then a rerun deploys and activates.

**Preflight** (all must hold before the push):

1. The PR's `release-dryrun` job is green, i.e. CI reproduces the ROM hashes
   attested in `data/release_arrivals.json`.
2. The commit being released includes the fresh-start commit (its server
   image carries the `fresh-start` subcommand).
3. On the VPS `/srv/hoenn/current`, `/srv/hoenn/game/current` and
   `/srv/hoenn/android/current` (when present) are regular files
   (`stat -c %F`); a legacy `current` symlink is migrated by activation,
   anything else stops it.
4. `command -v flock setfacl python3 setsid` succeeds; the SSH user's `umask`
   is `022`; `/var/log/hoenn` exists and is writable by that user.
5. Record the old `COOP_IMAGE` digest from `<deploy-dir>/.env` and confirm it
   is still in the local image cache (`docker image ls --digests`).
6. `VPS_GHCR_USER`/`VPS_GHCR_TOKEN` are set (or the VPS is already logged in to
   GHCR) so the new image can be pulled.
7. A recent database backup exists and has been restore-drilled.
8. **Freeze `main` and disable queued deploys** for the whole window: lock the
   branch (branch protection "Lock branch" or restrict pushes), merge nothing
   else, and cancel any other queued or running "Production release" run. The
   concurrency group does not cancel queued runs, so a later push would
   otherwise queue behind the held run and start as soon as it fails. Announce
   the maintenance window to players.

**Sequence:**

1. Set the repository variables `HOENN_ROLLOUT_HOLD=true` and
   `HOENN_ROLLBACK_MODE=stopped`. Use exactly lowercase `true`: the workflow
   compares `vars.HOENN_ROLLOUT_HOLD == 'true'`, so `1`, `yes` or a typo
   does not hold and the run would roll the server.
2. Push or merge the release commit. The run builds, uploads and promotes the
   server catalog, game and runtime release with `--no-flip`, then fails at
   "Rollout hold (production untouched)". There is no downtime and nothing is
   visible to players. The annotation names the new image and catalog.
3. Read `NEW` (`image_ref`) and `CAT` (`server_catalog_sha256`) from
   `/srv/hoenn/release-metadata/<sha>.json` and compare them with the
   annotation.
4. Maintenance window, in `<deploy-dir>`, exactly as FRESH_START.md
   describes:
   1. `docker pull NEW`.
   2. Stop the server (`docker compose stop server`).
   3. Take the backup and copy it off-host.
   4. Run the fresh start with `COOP_IMAGE=NEW` on the `docker compose
      config` and `docker compose run … fresh-start` commands (shell
      environment wins over `.env`, which still names the old image). The
      fresh start does not read the server catalog, so `CAT` is not needed
      here.
   5. **Leave the server stopped.** Do not start the new server by hand: the
      `compose.yaml` on the VPS is still the previous release's, its
      `server` service does not pass `COOP_PHASE2_RELEASE_CATALOG_*` into the
      container, and the new server would fail its release-catalog check and
      crash-loop. Do not start the old server either (FRESH_START.md step 9).
5. Keep `HOENN_ROLLBACK_MODE=stopped` and set `HOENN_ROLLOUT_HOLD=false`.
6. Re-run the workflow for the same commit. The probe reports `RELEASED`,
   nothing is rebuilt and promotion is a no-op. "Roll the server" then does
   the first start of the new server: `deploy-release.sh` installs this
   release's `compose.yaml`, writes `NEW`/`CAT` into `.env`, recreates the
   server container from the release compose and requires `/health/ready`.
   `activate-release.sh` checks `.env` against the association and flips the
   markers. Installers follow.
   - First try "Re-run failed jobs" on the held run. It is not yet verified
     whether a rerun re-reads repository variables: if the rerun stops at
     "Rollout hold" again, the old value was reused. Then start a new
     "Production release" run with "Run workflow" (`workflow_dispatch`) on
     `main`, which is still frozen at the release commit; its probe also
     reports `RELEASED` and it proceeds the same way.
7. Verify `cat /srv/hoenn/current /srv/hoenn/game/current`, health, a client
   login through HTTPS and a download. Keep the backup, the deploy log
   (`/var/log/hoenn/deploy-<sha>-<run>-<attempt>.log`), `.env.bak.*`,
   `compose.yaml.bak.*` and `release-metadata/<sha>.json`. Set
   `HOENN_ROLLBACK_MODE` back to empty (or `previous`) once the new release
   is confirmed, then unfreeze `main`.

**If the rerun fails after the fresh start**, the old image must never serve
the fresh-started database:

- exit 5 (expected with `HOENN_ROLLBACK_MODE=stopped`): the new server did
  not become healthy; `deploy-release.sh` restored `.env` and `compose.yaml`
  (old image, old compose) and left the server stopped. Do not
  `docker compose up` in that state. Read the deploy log, fix the cause and
  re-run (step 6), or follow FRESH_START.md Rollback A or B.
- exit 3 (rollback mode was not `stopped`): the old image is running against
  the fresh-started database. **Stop the server immediately**
  (`docker compose stop server`), then proceed as for exit 5.
- exit 4 or "PRODUCTION STATE UNKNOWN": **stop the server immediately**, read
  the deploy log and `.status` file, restore `.env` and `compose.yaml` from
  the newest `.env.bak.*`/`compose.yaml.bak.*` if needed, then proceed as for
  exit 5.
- exit 1: `deploy-release.sh` refused before changing anything (lock held,
  pull credentials, compose config), so the server is still stopped. Before
  trusting "nothing changed", check `/var/log/hoenn` for other
  `deploy-<sha>-*.status` files and for a deploy still holding the lock (an
  earlier attempt may still be running detached, or may have finished with a
  different status). Then fix the cause and re-run.
- "Deploy start unconfirmed": the SSH connection dropped while starting the
  detached deploy and polling then reported exit 1. If
  `/var/log/hoenn/deploy-<sha>-<run>-<attempt>.log` does not exist the deploy
  never started; check as for exit 1 and re-run.
- activation refused: the deployed `.env` is not `NEW`/`CAT`; re-run so
  `deploy-release.sh` writes it (do not edit `.env` by hand).

**No-hold fallback** (the push ran without `HOENN_ROLLOUT_HOLD=true`): if
"Roll the server" has not started yet, cancel the run, set both variables and
start a new run (step 6); it stops at the hold. Once it has started,
cancelling does not stop it (it runs detached); wait for the result. Exit 3
or 1: nothing visible changed, continue at step 3. Exit 5: the server is
stopped on the old configuration, continue at step 4 (skip the stop). Exit 0:
the new server is live on the un-fresh-started data and activation runs
next: stop the server immediately,
take the backup and run the fresh start (steps 4.3 and 4.4). `.env` and
`compose.yaml` already belong to the new release, so start it with
`docker compose up -d server`, require `/health/ready` 200, then verify
(step 7); no rerun is needed. Exit 4: stop the server and inspect first.

## Required protected configuration

| Name | Purpose |
|---|---|
| `RELEASE_TRUST_KEY_ID` | Public key id embedded in the envelope and trust bundle |
| `RELEASE_TRUST_PUBLIC_KEY_HEX` | 32-byte Ed25519 public key used by desktop and promotion |
| `MANIFEST_TRUST_KEY_ID` | Public key id compiled into Windows desktop manifest verification |
| `MANIFEST_TRUST_PUBLIC_KEY_HEX` | 32-byte Ed25519 public key compiled into Windows desktop |
| `HOENN_RELEASE_PRIVATE_SEED_HEX` | Runtime-only 32-byte signing seed |
| `COOP_API_BASE` | Nonsecret HTTPS API base compiled into desktop/bootstrapper |
| `AUTHENTICODE_CERT_B64` | Protected PFX bytes for the Windows MSI job; required only in `authenticode` mode |
| `AUTHENTICODE_PASSWORD` | Protected PFX password, never logged; required only in `authenticode` mode |
| `AUTHENTICODE_TIMESTAMP_URL` | Timestamp service URL; required only in `authenticode` mode |
| `VPS_HOST`, `VPS_USER`, `VPS_PORT` | SSH destination (host keys are pinned) |
| `VPS_SSH_KEY`, `VPS_KNOWN_HOSTS` | SSH identity and exact known-hosts data |
| `VPS_DEPLOY_DIR` | Absolute deployment directory on the VPS |
| `VPS_GHCR_USER`, `VPS_GHCR_TOKEN` | Read-only GHCR credentials (`read:packages`), passed over SSH stdin. **Set both** for the private image package. If both are empty the run warns and the VPS must already be logged in to GHCR as the SSH user; otherwise the pull fails and `deploy-release.sh` exits 1 with nothing changed. Setting only one fails the run |
| `HOENN_ROLLOUT_HOLD` (variable) | `true` (lowercase) stops the release job after `promote-release.sh --no-flip`, before the server is touched (first multi-world rollout); any other value rolls the server |
| `HOENN_ROLLBACK_MODE` (variable) | empty or `previous`: a failed rollout restarts the previous image (exit 3); `stopped`: it restores the files and leaves the server stopped (exit 5); anything else fails the run |

The workflow validates SSH grammars and uses `StrictHostKeyChecking yes`.
Never replace pinned host keys with an in-run key scan.
The release-key gate checks the protected private seed against the release
public key before either Windows artifact publication or runtime release.
Pushes to `main` update the runtime and server components on the VPS. The
Windows installer job also runs when installer or desktop inputs have changed
since the last published MSI; an explicit `workflow_dispatch` forces a build.
Comparison with the published MSI revision retries changes missed by a failed
or skipped run. Changes to protected workflow variables without a source change
require a manual dispatch. A successful installer run copies its verified MSI
to the private `/srv/hoenn/installers/<commit>-<run-number>/` store, checks its
SHA-256, and atomically updates `/srv/hoenn/installers/current`. Container UID
10001 receives read-only access through `setfacl`. The account page serves this
MSI only after login.
Android clients authenticate to download the ROM and matching manifest from the
signed release envelope. A new signed APK is published only when Android client
code has changed since the last published APK, or on a manual dispatch for a fresh release. The
`/srv/hoenn/android/current` marker selects the latest privately served APK and
is replaced only after its file and metadata are uploaded. The VPS needs `setfacl`
so container UID 10001 can read the APK while other local users cannot. A changed
release trust key also forces an APK rebuild. Native client and
emulator changes still require an APK; routine ROM changes do not.
For a VPS with `apt-get`, the production workflow installs the `acl` package
when `setfacl` is absent; the SSH user needs root or passwordless `sudo` for
that first install. On other hosts, install `setfacl` before publishing an APK.
Each rollout also validates and atomically installs the version-controlled
`compose.yaml` in the configured VPS deploy directory. The existing `.env`,
secrets, database, volumes, and Caddy configuration are preserved.
The installer job carries a version-controlled signing mode. During the private
pilot it is `unsigned-private-pilot`: both Authenticode steps are skipped, the
artifact name and provenance state that it is unsigned, and Windows may show an
unknown-publisher warning. Ed25519 release-envelope signing remains mandatory.
Changing to any unknown signing mode fails closed; returning to `authenticode`
requires a reviewed workflow change.

The Actions artifact is named `HoennSessions-UNSIGNED-PRIVATE-PILOT.msi` and
includes `hashes.json` and `provenance.json`. The account page downloads the
same verified bytes as `HoennSessions.msi` and shows the pilot signing warning.
For provenance verification, fetch the matching Actions artifact, confirm the
repository, commit, run id, and attempt, and compare its MSI SHA-256 with the
downloaded file and the private installer metadata. Do not forward the MSI
without that evidence.

Installer checkout, restore, and unsigned staging run before either narrow
Authenticode step receives secrets. The PFX bytes are imported into the CurrentUser\My
certificate store using a SecureString and removed in a `finally` block;
signtool receives only the certificate thumbprint and never a password or PFX
path. The installer job uses separate prepare, executable-sign, package,
MSI-sign, and finalize phases: secret-bearing phases invoke only signtool and
the certificate cleanup boundary; `dotnet build --no-restore` and artifact
upload run after the key is gone. Hashes and provenance are generated only
after the final MSI signature.

## Deployment and retry

Before building, the workflow streams `probe-release-status.sh` over the
pinned SSH connection. It classifies the requested commit as:

| State | Required remote state | Workflow action |
|---|---|---|
| `ABSENT` | no release, staging directory, or association | build, sign, upload, then promote |
| `PENDING` | association plus staging, no release | reuse the recorded image, skip rebuild/upload, promote |
| `RELEASED` | association plus release directory | reuse the recorded image, skip rebuild/upload, re-promote/roll out |
| `STALE` | staging directory without association or release | the workflow passes `--clean-stale-staging`: the unassociated `staging/<sha>` and `game-staging/<sha>` are deleted and the commit is treated as `ABSENT` |

`STALE` is a run that uploaded staging and died (failure, cancellation) before
`promote-release.sh` published the association. Nothing irreversible happened
and nothing refers to the upload, so it is safe to discard; the concurrency
group guarantees no other upload of the same commit is in flight.

### App-only releases keep the running server

An `ABSENT` push release still builds a fresh server image, but deploys the
live one instead when nothing the server depends on changed. The step "Decide
whether the live server image can be kept" reads `current` and its
`release-metadata/<id>.json`, then runs `.github/ci/classify_changes.py`
between that release and the new commit. When the classifier selects no ROM,
Rust or installer work (only `android/`, `docs/` or Markdown changed) and the
freshly assembled server catalog digest equals the live one, the new
association names the live image. `deploy-release.sh` then renders an
identical `.env` and compose file, `docker compose up -d` leaves the container
running, and players' sessions are not interrupted; activation still flips
the markers, including `android/current`. Any other change, unreadable live
metadata, or a manual `workflow_dispatch` run rolls the fresh image as
before.

Missing counterparts, malformed metadata, conflicting release/staging bytes,
and an image reference from another repository fail closed. For `ABSENT`, the
workflow builds the runtime and Linux `coop-release-tool`, writes the trust
bundle, signs the envelope, then copies the complete generation directly to
`/srv/hoenn/staging/<sha>`. It also copies the verifier and promotion scripts
to `/tmp`; the verifier is required on the VPS and receives only the public key
configuration. Promotion first publishes the immutable image association, then
moves staging into `releases/<sha>` and atomically replaces `current`; an
interrupted move can be retried against the recorded association. Production
promotion uses `--no-flip`; `current` changes only in `activate-release.sh`
after `deploy-release.sh` reports a healthy server (see "Multi-world releases").

Manual cleanup when the probe still fails closed (operator, on the VPS,
after reading `release-metadata/<sha>.json` if it exists):

- Staging without association: `rm -rf /srv/hoenn/staging/<sha>
  /srv/hoenn/game-staging/<sha>` (what `--clean-stale-staging` does).
- Association without release or staging (staging was deleted by hand):
  delete `/srv/hoenn/release-metadata/<sha>.json` only if
  `/srv/hoenn/releases/<sha>` does not exist and that image was never deployed
  (`grep COOP_IMAGE <deploy-dir>/.env*`); the rerun then builds a new image.
- Leftover `server-catalog-staging/<catalog-sha>`: delete it; never delete a
  promoted `server-catalog/<catalog-sha>` that any `release-metadata` file or
  `.env` names.
- A promoted `game/<sha>` from a failed run conflicts with a *new* run of the
  same commit (new sequence): delete `game/<sha>` only if `game/current` does
  not name it. "Re-run failed jobs" keeps the sequence and is idempotent.

Run a private re-promotion manually after a failed rollout:

```sh
HOENN_ROOT=/srv/hoenn \
COOP_RELEASE_KEY_ID=pilot-v1 \
COOP_RELEASE_PUBLIC_KEY_HEX=<public-key-hex> \
COOP_RELEASE_TOOL=/tmp/coop-release-tool \
bash deploy/coop/promote-release.sh <full-sha> \
  --image-ref ghcr.io/<owner>/hoenn-sessions-server@sha256:<64hex> \
  --image-digest sha256:<64hex>
```

An already-promoted identical release is a no-op and reuses its recorded image
association even when it is not current. A different staging copy or image
association under an existing id is rejected. To roll back client runtime,
promote an already verified older generation with its recorded association;
`.previous-release` records the marker that was live before the last successful
switch. Roll back the server with that same recorded digest using
`deploy-release.sh`; never use mutable tags or `docker compose down -v`.

## Verification

On the VPS, inspect only private operator output:

```sh
cat /srv/hoenn/current
id="$(cat /srv/hoenn/current)"
cd "/srv/hoenn/releases/$id"
COOP_RELEASE_TOOL=/tmp/coop-release-tool \
  COOP_RELEASE_KEY_ID=pilot-v1 \
  COOP_RELEASE_PUBLIC_KEY_HEX=<public-key-hex> \
  coop-release-tool verify --envelope release-envelope.json \
    --key-id "$COOP_RELEASE_KEY_ID" \
    --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX"
```

The hermetic checks are safe to run from a checkout:

```sh
cargo test --manifest-path Cargo.toml -p coop-release-tool --all-features --locked
bash deploy/coop/test-release-scripts.sh
```

Those tests use a temporary root and cover exact inventory, signature and
envelope requirements, marker migration, idempotent promotion, conflict
rejection, rollback, workflow private-transport invariants, and Compose
release-root wiring.
