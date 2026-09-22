# Private-pilot releases

The production workflow produces one immutable Windows runtime generation per
commit. `github.run_number` is the signed monotonic sequence; a release id is
the full lowercase commit SHA. Re-running a commit reuses its promoted
generation and image digest instead of creating conflicting metadata.

## What is signed and where it goes

The release tool uses the launcher’s schema-one types and fixed destinations.
Every generation contains exactly these eleven files plus
`release-envelope.json`:

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
├── release-metadata/<sha>.json # immutable release-to-image association
├── current              # regular file: <sha>\n, atomically replaced
└── .previous-release    # previous marker id for rollback
```

`current` is deliberately a regular bounded marker. The server reads it only
after authentication, resolves `releases/<id>`, and serves the signed
envelope or one fixed artifact. A legacy `current -> releases/<sha>` symlink
is accepted by `promote-release.sh` only to migrate it atomically to the
marker contract. Invalid markers, symlinks inside a generation, extra files,
missing artifacts, signature failures, and conflicting re-uploads fail closed.

Compose sets `COOP_RELEASE_ROOT=/srv/hoenn` and mounts the release parent
read-only into the server. Do not mount `current` itself: Docker resolves a
symlink at container creation and would pin the server to one generation.
The metadata file is outside the served eleven-artifact directory and has the
schema `{schema, release_id, image_ref, image_digest}`. Promotion validates
that `image_ref` ends in the exact `sha256:<64 lowercase hex>` digest, creates
the file without overwriting an existing association, and writes it before
moving staging into `releases/<sha>` or changing `current`; a failed move may
leave that validated orphan for a later matching retry. Status and retry resolve the requested
`releases/<sha>` plus its metadata; they never pair `current` with whichever
container happens to be running.

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
| `VPS_GHCR_USER`, `VPS_GHCR_TOKEN` | Optional read-only GHCR credentials, passed over SSH stdin |

The workflow validates SSH grammars and uses `StrictHostKeyChecking yes`.
Never replace pinned host keys with an in-run key scan.
The release-key gate checks the protected private seed against the release
public key before either Windows artifact publication or runtime release.
Pushes to `main` update only the runtime and server components on the VPS. The
Windows installer job runs only for an explicit `workflow_dispatch`, so normal
server deployments do not rebuild or publish a desktop installer.
Each rollout also validates and atomically installs the version-controlled
`compose.yaml` in the configured VPS deploy directory. The existing `.env`,
secrets, database, volumes, and Caddy configuration are preserved.
The installer job carries a version-controlled signing mode. During the private
pilot it is `unsigned-private-pilot`: both Authenticode steps are skipped, the
artifact name and provenance state that it is unsigned, and Windows may show an
unknown-publisher warning. Ed25519 release-envelope signing remains mandatory.
Changing to any unknown signing mode fails closed; returning to `authenticode`
requires a reviewed workflow change.

The unsigned artifact is named `HoennSessions-UNSIGNED-PRIVATE-PILOT.msi` and
is restricted to invited testers. Download it only with `hashes.json` and
`provenance.json` from the same authenticated Actions run. Before bypassing the
Windows unknown-publisher warning, confirm the repository, commit, run id, and
attempt in provenance and verify the MSI SHA-256. Do not rename or forward the
MSI separately from that evidence.

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

Missing counterparts, malformed metadata, conflicting release/staging bytes,
and an image reference from another repository fail closed. For `ABSENT`, the
workflow builds the runtime and Linux `coop-release-tool`, writes the trust
bundle, signs the envelope, then copies the complete generation directly to
`/srv/hoenn/staging/<sha>`. It also copies the verifier and promotion scripts
to `/tmp`; the verifier is required on the VPS and receives only the public key
configuration. Promotion first publishes the immutable image association, then
moves staging into `releases/<sha>` and atomically replaces `current`; an
interrupted move can be retried against the recorded association. The deployment step
rolls the digest-pinned server image with the existing readiness gate.

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
