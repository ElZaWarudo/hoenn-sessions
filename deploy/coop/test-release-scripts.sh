#!/usr/bin/env bash
# Hermetic private-pilot release promotion tests. No Docker, SSH, network,
# VPS path, or public artifact service is touched.
set -u
SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
cd -- "$SCRIPT_DIR"

PROMOTE=./promote-release.sh
PROBE=./probe-release-status.sh
WORKFLOW=../../.github/workflows/deploy.yml
PYTHON_BIN="${PYTHON_BIN:-python3}"
if ! "$PYTHON_BIN" -c 'pass' >/dev/null 2>&1; then
  if python -c 'pass' >/dev/null 2>&1; then
    PYTHON_BIN=python
  elif [ -x /c/Python313/python.exe ]; then
    PYTHON_BIN=/c/Python313/python.exe
  else
    echo "python3 is required for hermetic release tests" >&2
    exit 1
  fi
fi
PASS=0
FAIL=0
report() {
  if [ "$1" -eq 0 ]; then PASS=$((PASS + 1)); printf 'ok   %s\n' "$2";
  else FAIL=$((FAIL + 1)); printf 'FAIL %s\n' "$2"; [ "$#" -lt 3 ] || printf '     %s\n' "$3"; fi
}

if [ -z "${COOP_RELEASE_TOOL:-}" ]; then
  COOP_RELEASE_TOOL="$REPO_ROOT/target/debug/coop-release-tool"
  if [ ! -x "$COOP_RELEASE_TOOL" ] && [ ! -x "$COOP_RELEASE_TOOL.exe" ]; then
    (cd "$REPO_ROOT" && cargo build --quiet --locked -p coop-release-tool) || exit 1
  fi
  [ -x "$COOP_RELEASE_TOOL" ] || COOP_RELEASE_TOOL="$COOP_RELEASE_TOOL.exe"
fi
export COOP_RELEASE_TOOL

SEED=0707070707070707070707070707070707070707070707070707070707070707
export HOENN_RELEASE_PRIVATE_SEED_HEX="$SEED"
export COOP_RELEASE_KEY_ID=pilot-v1
COOP_RELEASE_PUBLIC_KEY_HEX="$("$COOP_RELEASE_TOOL" public-key)"
export COOP_RELEASE_PUBLIC_KEY_HEX

ROOT="$(mktemp -d "${TMPDIR:-/tmp}/hoenn-release-test.XXXXXX")" || exit 1
trap 'chmod -R u+w -- "$ROOT" 2>/dev/null; rm -rf -- "$ROOT"' EXIT
export HOENN_ROOT="$ROOT"

FULL_A=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
FULL_B=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
FULL_C=cccccccccccccccccccccccccccccccccccccccc
FULL_D=dddddddddddddddddddddddddddddddddddddddd
DIGEST_A=1111111111111111111111111111111111111111111111111111111111111111
DIGEST_B=2222222222222222222222222222222222222222222222222222222222222222
DIGEST_C=3333333333333333333333333333333333333333333333333333333333333333
DIGEST_D=4444444444444444444444444444444444444444444444444444444444444444
IMAGE_A="ghcr.io/example/hoenn-sessions-server@sha256:$DIGEST_A"
IMAGE_B="ghcr.io/example/hoenn-sessions-server@sha256:$DIGEST_B"
IMAGE_C="ghcr.io/example/hoenn-sessions-server@sha256:$DIGEST_C"
IMAGE_D="ghcr.io/example/hoenn-sessions-server@sha256:$DIGEST_D"
IMAGE_BASE="${IMAGE_A%@*}"
NOW="$($PYTHON_BIN -c 'import time; print(int(time.time()))')"

probe_status() {
  HOENN_ROOT="$ROOT" PYTHON_BIN="$PYTHON_BIN" bash "$PROBE" "$1" "$2"
}

# Mirrors the workflow's final image-selection branch. A prospective fresh
# digest must never replace a recorded association for PENDING/RELEASED.
select_workflow_image() {
  local state="$1" recorded_ref="$2" recorded_digest="$3" fresh_ref="$4" fresh_digest="$5"
  case "$state" in
    PENDING|RELEASED) printf '%s\n%s\n' "$recorded_ref" "$recorded_digest" ;;
    ABSENT) printf '%s\n%s\n' "$fresh_ref" "$fresh_digest" ;;
    *) return 1 ;;
  esac
}

make_staging() {
  local root="$1" id="$2" sequence="$3" expiry="$4" variant="${5:-}" destination index
  mkdir -p "$root/staging/$id/app" "$root/staging/$id/runtime" \
    "$root/staging/$id/bridge" "$root/staging/$id/trust"
  index=0
  for destination in \
    app/coop-launcher.exe runtime/mgba.exe runtime/game.gba runtime/coop-sidecar.exe \
    bridge/main.lua bridge/memory.lua bridge/protocol.lua bridge/generated_addresses.lua \
    bridge_manifest.json trust/release-trust.json THIRD_PARTY_NOTICES.txt; do
    mkdir -p "$(dirname "$root/staging/$id/$destination")"
    printf 'fixture-%s-%s-%s\n' "$id" "$index" "$variant" > "$root/staging/$id/$destination"
    index=$((index + 1))
  done
  "$PYTHON_BIN" - "$root/staging/$id/trust/release-trust.json" "$COOP_RELEASE_PUBLIC_KEY_HEX" <<'PY'
import json
import pathlib
import sys
pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "algorithm": "ed25519", "key_id": "pilot-v1",
    "public_key_hex": sys.argv[2]
}, separators=(",", ":")))
PY
  "$COOP_RELEASE_TOOL" sign --release-id "$id" --sequence "$sequence" \
    --issued-at "$((expiry - 3600))" --expires-at "$expiry" --key-id pilot-v1 \
    --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX" \
    --output "$root/staging/$id/release-envelope.json" \
    --artifact "desktop-app=$root/staging/$id/app/coop-launcher.exe" \
    --artifact "managed-mgba=$root/staging/$id/runtime/mgba.exe" \
    --artifact "rom=$root/staging/$id/runtime/game.gba" \
    --artifact "sidecar=$root/staging/$id/runtime/coop-sidecar.exe" \
    --artifact "bridge-main=$root/staging/$id/bridge/main.lua" \
    --artifact "bridge-memory=$root/staging/$id/bridge/memory.lua" \
    --artifact "bridge-protocol=$root/staging/$id/bridge/protocol.lua" \
    --artifact "bridge-addresses=$root/staging/$id/bridge/generated_addresses.lua" \
    --artifact "compatibility-manifest=$root/staging/$id/bridge_manifest.json" \
    --artifact "trust-bundle=$root/staging/$id/trust/release-trust.json" \
    --artifact "notices=$root/staging/$id/THIRD_PARTY_NOTICES.txt" >/dev/null
}

mkdir -p "$ROOT/staging"
out="$(probe_status "$FULL_A" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -eq 0 ] && printf '%s\n' "$out" | grep -q '^state=ABSENT$' && printf '%s\n' "$out" | grep -q '^image_ref=$' && printf '%s\n' "$out" | grep -q '^image_digest=$'
report $? "probe classifies an untouched commit as ABSENT" "$out"
make_staging "$ROOT" "$FULL_A" 1 "$((NOW + 3600))"
out="$(COOP_RELEASE_TEST_FAIL_ASSOCIATION=1 $PROMOTE "$FULL_A" --image-ref "$IMAGE_A" --image-digest "sha256:$DIGEST_A" 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ -d "$ROOT/staging/$FULL_A" ] && [ ! -d "$ROOT/releases/$FULL_A" ] && [ ! -e "$ROOT/current" ] && [ ! -e "$ROOT/release-metadata/$FULL_A.json" ]
report $? "association creation failure leaves staging untouched" "$out"
out="$($PROMOTE "$FULL_A" --image-ref "$IMAGE_A" --image-digest "sha256:$DIGEST_A" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ -f "$ROOT/current" ] && [ ! -L "$ROOT/current" ] && [ "$(cat "$ROOT/current")" = "$FULL_A" ]
report $? "fresh exact eleven-file promotion writes marker" "$out"
[ -f "$ROOT/release-metadata/$FULL_A.json" ] && grep -q "$IMAGE_A" "$ROOT/release-metadata/$FULL_A.json"
report $? "promotion persists immutable image association before marker" "$out"
out="$(probe_status "$FULL_A" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -eq 0 ] && printf '%s\n' "$out" | grep -q '^state=RELEASED$' && printf '%s\n' "$out" | grep -q "^image_ref=$IMAGE_A$" && printf '%s\n' "$out" | grep -q "^image_digest=sha256:$DIGEST_A$"
report $? "probe classifies an immutable release as RELEASED" "$out"

# A move failure may leave a validated orphan association, but must not expose
# releases/current; the next matching retry reuses the association.
make_staging "$ROOT" "$FULL_B" 2 "$((NOW + 3600))"
out="$(COOP_RELEASE_TEST_FAIL_MOVE=1 $PROMOTE "$FULL_B" --image-ref "$IMAGE_B" --image-digest "sha256:$DIGEST_B" 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ -d "$ROOT/staging/$FULL_B" ] && [ -f "$ROOT/release-metadata/$FULL_B.json" ] && [ ! -d "$ROOT/releases/$FULL_B" ] && [ "$(cat "$ROOT/current")" = "$FULL_A" ]
report $? "move failure leaves reusable orphan association" "$out"
probe_out="$(probe_status "$FULL_B" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -eq 0 ] && printf '%s\n' "$probe_out" | grep -q '^state=PENDING$' && printf '%s\n' "$probe_out" | grep -q "^image_ref=$IMAGE_B$" && printf '%s\n' "$probe_out" | grep -q "^image_digest=sha256:$DIGEST_B$"
report $? "probe classifies association plus staging as PENDING" "$probe_out"
selected="$(select_workflow_image PENDING "$IMAGE_B" "sha256:$DIGEST_B" "$IMAGE_BASE@sha256:$DIGEST_C" "sha256:$DIGEST_C")"; status=$?
[ "$status" -eq 0 ] && [ "$(printf '%s\n' "$selected" | sed -n '1p')" = "$IMAGE_B" ] && [ "$(printf '%s\n' "$selected" | sed -n '2p')" = "sha256:$DIGEST_B" ]
report $? "PENDING reuses recorded image over a prospective fresh digest" "$selected"
out="$(probe_status "$FULL_B" "ghcr.io/other/repository" 2>&1)"; status=$?
[ "$status" -ne 0 ]
report $? "probe rejects an association from the wrong repository" "$out"
out="$($PROMOTE "$FULL_B" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/current")" = "$FULL_B" ]
report $? "matching retry completes orphan-association promotion" "$out"
out="$($PROMOTE "$FULL_A" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/current")" = "$FULL_A" ]
report $? "partial promotion rollback remains available" "$out"
out="$(probe_status "$FULL_B" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -eq 0 ] && printf '%s\n' "$out" | grep -q '^state=RELEASED$'
report $? "probe classifies a reusable non-current release" "$out"

# Existing release and conflicting staging must fail closed rather than let a
# rerun guess which bytes belong to the immutable association.
make_staging "$ROOT" "$FULL_B" 2 "$((NOW + 3600))" different
out="$(probe_status "$FULL_B" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -ne 0 ]
report $? "probe rejects conflicting release and staging generations" "$out"
rm -rf "$ROOT/staging/$FULL_B"

# Exercise each inconsistent metadata/path combination before the normal
# artifact rejection cases below.
mkdir -p "$ROOT/releases/$FULL_C"
out="$(probe_status "$FULL_C" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -ne 0 ]
report $? "probe rejects a release without an association" "$out"
rm -rf "$ROOT/releases/$FULL_C"
mkdir -p "$ROOT/release-metadata"
printf '{"schema":1,"release_id":"%s","image_ref":"%s","image_digest":"sha256:%s"}\n' "$FULL_D" "$IMAGE_D" "$DIGEST_D" > "$ROOT/release-metadata/$FULL_D.json"
out="$(probe_status "$FULL_D" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -ne 0 ]
report $? "probe rejects an association without release or staging" "$out"
rm -f "$ROOT/release-metadata/$FULL_D.json"
printf '{}\n' > "$ROOT/release-metadata/$FULL_C.json"
out="$(probe_status "$FULL_C" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -ne 0 ]
report $? "probe rejects malformed association metadata" "$out"
rm -f "$ROOT/release-metadata/$FULL_C.json"

out="$($PROMOTE "$FULL_A" 2>&1)"; status=$?
[ "$status" -eq 0 ] && printf '%s' "$out" | grep -q 'already promoted'
report $? "marker re-promotion is a no-op" "$out"

make_staging "$ROOT" "$FULL_C" 3 "$((NOW + 3600))"
rm -f "$ROOT/staging/$FULL_C/release-envelope.json"
out="$($PROMOTE "$FULL_C" --image-ref "$IMAGE_C" --image-digest "sha256:$DIGEST_C" 2>&1)"; status=$?
[ "$status" -ne 0 ]
report $? "signed envelope is required" "$out"
rm -rf "$ROOT/staging/$FULL_C"
make_staging "$ROOT" "$FULL_B" 2 "$((NOW + 3600))"
rm -f "$ROOT/staging/$FULL_B/runtime/mgba.exe"
out="$($PROMOTE "$FULL_B" --image-ref "$IMAGE_B" --image-digest "sha256:$DIGEST_B" 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ "$(cat "$ROOT/current")" = "$FULL_A" ]
report $? "missing fixed artifact is rejected" "$out"
rm -rf "$ROOT/staging/$FULL_B"

make_staging "$ROOT" "$FULL_B" 2 "$((NOW + 3600))"
out="$($PROMOTE "$FULL_B" --image-ref "$IMAGE_B" --image-digest "sha256:$DIGEST_B" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/current")" = "$FULL_B" ]
report $? "second release promotion succeeds" "$out"

# Re-uploading an existing release with a conflicting immutable image is
# rejected even when the eleven files are byte-for-byte identical.
make_staging "$ROOT" "$FULL_A" 1 "$((NOW + 3600))"
out="$($PROMOTE "$FULL_A" --image-ref "$IMAGE_B" --image-digest "sha256:$DIGEST_B" 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'association conflict'
report $? "conflicting release image association is rejected" "$out"
rm -rf "$ROOT/staging/$FULL_A"

# Legacy current symlink is migrated to a bounded regular marker.
rm -rf "$ROOT/current"
if ln -s "releases/$FULL_B" "$ROOT/current" 2>/dev/null && [ -L "$ROOT/current" ]; then
  out="$($PROMOTE "$FULL_B" 2>&1)"; status=$?
  [ "$status" -eq 0 ] && [ ! -L "$ROOT/current" ] && [ "$(cat "$ROOT/current")" = "$FULL_B" ]
  report $? "legacy current symlink migrates atomically" "$out"
else
  # Git for Windows without symlink privileges creates a directory for ln -s;
  # retain the Linux assertion while recording this local platform limitation.
  rm -rf "$ROOT/current"
  printf '%s\n' "$FULL_B" > "$ROOT/current"
  report 0 "legacy current symlink migration (platform skipped)"
fi

make_staging "$ROOT" "$FULL_B" 2 "$((NOW + 3600))" different
out="$($PROMOTE "$FULL_B" --image-ref "$IMAGE_B" --image-digest "sha256:$DIGEST_B" 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'differs'
report $? "conflicting re-upload is rejected" "$out"
rm -rf "$ROOT/staging/$FULL_B"

out="$($PROMOTE "$FULL_A" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/current")" = "$FULL_A" ] && [ "$(cat "$ROOT/.previous-release")" = "$FULL_B" ]
report $? "rollback promotion records previous release" "$out"

# The size gate rejects a sparse file before read_bytes/read allocation. The
# signed descriptor remains small; only the transport entry is oversized.
make_staging "$ROOT" "$FULL_C" 3 "$((NOW + 3600))"
truncate -s 536870913 "$ROOT/staging/$FULL_C/runtime/mgba.exe"
out="$($PROMOTE "$FULL_C" --image-ref "$IMAGE_C" --image-digest "sha256:$DIGEST_C" 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ ! -d "$ROOT/releases/$FULL_C" ]
report $? "oversized sparse artifact is rejected before hashing" "$out"
rm -rf "$ROOT/staging/$FULL_C"

# Metadata survives a failed marker flip; retry uses the stored association
# without rebuilding or re-signing the bundle.
make_staging "$ROOT" "$FULL_D" 4 "$((NOW + 3600))"
printf 'not-a-release\n' > "$ROOT/current"
out="$($PROMOTE "$FULL_D" --image-ref "$IMAGE_D" --image-digest "sha256:$DIGEST_D" 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ -f "$ROOT/release-metadata/$FULL_D.json" ]
report $? "failed marker flip preserves release image association" "$out"
printf '%s\n' "$FULL_A" > "$ROOT/current"
out="$($PROMOTE "$FULL_D" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/current")" = "$FULL_D" ] && printf '%s' "$out" | grep -q "$DIGEST_D"
report $? "retry reuses durable association after failed rollout" "$out"

if grep -q 'github.run_number' "$WORKFLOW" && \
   grep -q 'mGBA-build-2026-09-19-win64-9139-3a5bc24629867576b0fb576a5d5a21d3b3d6b576.7z' "$WORKFLOW" && \
   grep -q 'ea7cc0e8632cd80d28bdb55e37aacc58b2b018f564209f790e8cc3caed8c002b' "$WORKFLOW" && \
   grep -q 'release-envelope.json' "$WORKFLOW" && \
   grep -q 'upload-artifact' "$WORKFLOW" && \
   grep -q 'HOENN_MANIFEST_TRUST_KEY_ID' "$WORKFLOW" && \
   grep -q 'release-key-gate' "$WORKFLOW" && \
   grep -q 'release-metadata' "$PROMOTE" && \
   grep -q 'release-metadata' "$PROBE" && \
   grep -q 'RELEASE_DIR=.*releases/\$RELEASE_ID' "$PROBE" && \
   grep -q 'RELEASED_IMAGE' "$WORKFLOW" && \
   grep -q 'COOP_RELEASE_TEST_FAIL_ASSOCIATION' "$PROMOTE" && \
   grep -q 'probe-release-status.sh' "$WORKFLOW" && \
   grep -q 'PENDING|RELEASED' "$WORKFLOW" && \
   grep -q 'state=ABSENT' "$PROBE" && \
   grep -q 'state=PENDING' "$PROBE" && \
   grep -q 'state=RELEASED' "$PROBE" && \
   grep -q -- '-Phase SignExecutables' "$WORKFLOW" && \
   grep -q -- '-Phase SignMsi' "$WORKFLOW" && \
   ! grep -q 'upload.*pokeemerald.gba' "$WORKFLOW" && \
   ! grep -q 'docker inspect' "$WORKFLOW"; then
  report 0 "workflow pins sequence, mGBA, and private artifact boundary"
else
  report 1 "workflow pins sequence, mGBA, and private artifact boundary"
fi
if grep -q 'actions/[^ ]*@v[0-9]' "$WORKFLOW" || \
   ! grep -q 'dotnet-version: 9.0.203' "$WORKFLOW" || \
   grep -q 'PersistKeySet' "$WORKFLOW" || \
   grep -q 'HOENN_SIGNING_PASSWORD\|AuthenticodeCertificatePath' "$REPO_ROOT/installer/windows/build-installer.ps1" || \
   grep -Eq '(^|[^[:alnum:]])/p([^[:alnum:]]|$)' "$REPO_ROOT/installer/windows/build-installer.ps1"; then
  report 1 "workflow actions/dotnet and secure installer signing pins"
else
  report 0 "workflow actions/dotnet and secure installer signing pins"
fi
if grep -q 'WINDOWS_INSTALLER_SIGNING_MODE: unsigned-private-pilot' "$WORKFLOW" && \
   [ "$(grep -c "if: env.WINDOWS_INSTALLER_SIGNING_MODE == 'authenticode'" "$WORKFLOW")" -eq 2 ] && \
   grep -q '\$arguments.UnsignedPrivatePilot = \$true' "$WORKFLOW" && \
   grep -q 'HoennSessions-UNSIGNED-PRIVATE-PILOT.msi' "$WORKFLOW" && \
   grep -q 'Verify finalized installer before upload' "$WORKFLOW" && \
   grep -q 'HOENN_CI_RUN_ATTEMPT' "$WORKFLOW" && \
   grep -q 'hoenn-private-pilot-installer-\${{ env.WINDOWS_INSTALLER_SIGNING_MODE }}' "$WORKFLOW" && \
   grep -q 'private-pilot-only' "$REPO_ROOT/installer/windows/build-installer.ps1" && \
   grep -q 'HoennSessions-UNSIGNED-PRIVATE-PILOT.msi' "$REPO_ROOT/installer/windows/build-installer.ps1"; then
  report 0 "workflow labels and isolates unsigned private-pilot installer mode"
else
  report 1 "workflow labels and isolates unsigned private-pilot installer mode"
fi
if grep -q 'COOP_RELEASE_ROOT: /srv/hoenn' compose.yaml && \
   grep -q '/srv/hoenn:/srv/hoenn:ro' compose.yaml; then
  report 0 "compose wires read-only release parent"
else
  report 1 "compose wires read-only release parent"
fi

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
