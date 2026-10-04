#!/usr/bin/env bash
# Hermetic private-pilot release promotion tests. No Docker, SSH, network,
# VPS path, or public artifact service is touched.
set -u
SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
cd -- "$SCRIPT_DIR"

PROMOTE=./promote-release.sh
PROMOTE_GAME=./promote-game.sh
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
# The shim below precedes system directories in PATH. Pin the interpreter's
# absolute path first so a Linux `python3` shim cannot exec itself forever.
PYTHON_BIN="$(command -v "$PYTHON_BIN")"
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

if grep -Eq 'tmpfs:[[:space:]]*\[[^]]*,[^]]*\]' "$SCRIPT_DIR/compose.yaml"; then
  report 1 "compose tmpfs options are not split by YAML flow-list commas"
else
  report 0 "compose tmpfs options are not split by YAML flow-list commas"
fi

ROOT="$(mktemp -d "${TMPDIR:-/tmp}/hoenn-release-test.XXXXXX")" || exit 1
trap 'chmod -R u+w -- "$ROOT" 2>/dev/null; rm -rf -- "$ROOT"' EXIT
export HOENN_ROOT="$ROOT"
# promote-game.sh uses python3 directly. Supply the same working Python 3
# selected above on Windows hosts where python3 is only a Store alias.
mkdir -p "$ROOT/bin"
cat > "$ROOT/bin/python3" <<'SH'
#!/usr/bin/env bash
exec "$PYTHON_BIN" "$@"
SH
chmod +x "$ROOT/bin/python3"
export PYTHON_BIN PATH="$ROOT/bin:$PATH"
timeout 10s "$ROOT/bin/python3" -c 'pass' || {
  echo "python3 test shim failed to start" >&2
  exit 1
}

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

make_game_staging() {
  local root="$1" id="$2" sequence="$3" issued="$4" variant="${5:-original}"
  local dir="$root/game-staging/$id"
  mkdir -p "$dir"
  rm -f -- "$dir/release-envelope.json"
  printf 'private-rom-%s-%s\n' "$id" "$variant" > "$dir/game.gba"
  printf '{"fixture":"%s"}\n' "$variant" > "$dir/bridge_manifest.json"
  "$COOP_RELEASE_TOOL" sign-game --release-id "$id" --sequence "$sequence" \
    --issued-at "$issued" --expires-at "$((issued + 3600))" \
    --key-id "$COOP_RELEASE_KEY_ID" --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX" \
    --output "$dir/release-envelope.json" \
    --artifact "rom=$dir/game.gba" \
    --artifact "compatibility-manifest=$dir/bridge_manifest.json" >/dev/null
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

# Game promotion precedes the Windows rollout. A failed Windows deployment
# must allow a later run to re-sign the same immutable game bytes with a fresh
# validity window, while a changed ROM or manifest must fail closed.
make_game_staging "$ROOT" "$FULL_C" 5 "$NOW"
out="$(bash "$PROMOTE_GAME" "$FULL_C" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/game/current")" = "$FULL_C" ] && \
  [ "$(cat "$ROOT/current")" = "$FULL_D" ] && [ ! -d "$ROOT/game-staging/$FULL_C" ]
report $? "game release promotes independently before Windows rollout" "$out"
cp "$ROOT/game/$FULL_C/release-envelope.json" "$ROOT/promoted-game-envelope.json"
failed_windows_deploy() { return 17; }
failed_windows_deploy; status=$?
[ "$status" -eq 17 ] && [ "$(cat "$ROOT/current")" = "$FULL_D" ] && \
  [ "$(cat "$ROOT/game/current")" = "$FULL_C" ]
report $? "failed Windows rollout leaves promoted game available" "status=$status"
make_game_staging "$ROOT" "$FULL_C" 5 "$((NOW + 10))"
out="$(bash "$PROMOTE_GAME" "$FULL_C" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/game/current")" = "$FULL_C" ] && \
  [ "$(cat "$ROOT/current")" = "$FULL_D" ] && \
  cmp -s "$ROOT/game/$FULL_C/release-envelope.json" "$ROOT/promoted-game-envelope.json" && \
  ! cmp -s "$ROOT/game-staging/$FULL_C/release-envelope.json" "$ROOT/promoted-game-envelope.json"
report $? "re-signed same game release is an idempotent retry" "$out"
make_game_staging "$ROOT" "$FULL_C" 5 "$((NOW + 20))" changed
out="$(bash "$PROMOTE_GAME" "$FULL_C" 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ "$(cat "$ROOT/game/current")" = "$FULL_C" ] && \
  cmp -s "$ROOT/game/$FULL_C/release-envelope.json" "$ROOT/promoted-game-envelope.json"
report $? "same game release rejects divergent artifact content" "$out"
rm -f -- "$ROOT/game-staging/$FULL_C/game.gba" \
  "$ROOT/game-staging/$FULL_C/bridge_manifest.json" \
  "$ROOT/game-staging/$FULL_C/release-envelope.json"
rmdir -- "$ROOT/game-staging/$FULL_C"

# A new release must verify the installed game's own signed identity before
# checking the incoming release and advancing the marker.
make_game_staging "$ROOT" "$FULL_D" 6 "$NOW"
out="$(bash "$PROMOTE_GAME" "$FULL_D" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/game/current")" = "$FULL_D" ] && \
  [ -d "$ROOT/game/$FULL_C" ] && [ ! -d "$ROOT/game-staging/$FULL_D" ]
report $? "new game release promotes over a different current release" "$out"

# ---------------------------------------------------------------------------
# Multi-world releases: descriptor-derived inventories, region catalog
# cross-checks, --no-flip promotion and the separate activation step.
# ---------------------------------------------------------------------------
PROMOTE_CATALOG=./promote-server-catalog.sh
ACTIVATE=./activate-release.sh
DEPLOY=./deploy-release.sh
FULL_E=eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
FULL_F=ffffffffffffffffffffffffffffffffffffffff
FULL_G=abababababababababababababababababababab
DIGEST_E=5555555555555555555555555555555555555555555555555555555555555555
DIGEST_F=6666666666666666666666666666666666666666666666666666666666666666
DIGEST_G=7777777777777777777777777777777777777777777777777777777777777777
IMAGE_E="$IMAGE_BASE@sha256:$DIGEST_E"
IMAGE_F="$IMAGE_BASE@sha256:$DIGEST_F"
IMAGE_G="$IMAGE_BASE@sha256:$DIGEST_G"

# Writes worlds/<N>/{game.gba,bridge_manifest.json,player_transfer.json} and a
# release_catalog.json binding them. World 1 reuses the base ROM/manifest.
# mismatch=<N> writes a wrong rom_sha256 for world N into the catalog.
write_world_files() {
  local dir="$1" base_rom="$2" base_manifest="$3" variant="$4" mismatch="${5:-0}"
  "$PYTHON_BIN" - "$dir" "$base_rom" "$base_manifest" "$variant" "$mismatch" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1]); base_rom = pathlib.Path(sys.argv[2]); base_manifest = pathlib.Path(sys.argv[3])
variant, mismatch = sys.argv[4], int(sys.argv[5])
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
worlds = []
for world in (1, 2):
    d = root / "worlds" / str(world)
    d.mkdir(parents=True, exist_ok=True)
    if world == 1:
        (d / "game.gba").write_bytes(base_rom.read_bytes())
        (d / "bridge_manifest.json").write_bytes(base_manifest.read_bytes())
    else:
        (d / "game.gba").write_bytes(f"world-{world}-rom-{variant}\n".encode())
        (d / "bridge_manifest.json").write_bytes(f'{{"world":{world},"v":"{variant}"}}\n'.encode())
    (d / "player_transfer.json").write_bytes(f'{{"transfer":{world},"v":"{variant}"}}\n'.encode())
    p = f"worlds/{world}"
    worlds.append({"world_id": world, "rom_path": f"{p}/game.gba",
                   "rom_sha256": "0" * 64 if world == mismatch else sha(d / "game.gba"),
                   "bridge_path": f"{p}/bridge_manifest.json", "bridge_sha256": sha(d / "bridge_manifest.json"),
                   "player_transfer_path": f"{p}/player_transfer.json",
                   "player_transfer_sha256": sha(d / "player_transfer.json")})
(root / "release_catalog.json").write_text(json.dumps({"schema_version": 1, "worlds": worlds}, sort_keys=True) + "\n")
PY
}

world_artifact_args() {
  local dir="$1" world kind file
  printf '%s\n' "region-catalog=$dir/release_catalog.json"
  for world in 1 2; do
    for kind in rom:game.gba compatibility:bridge_manifest.json player-transfer:player_transfer.json; do
      file="${kind#*:}"
      printf '%s\n' "world-$world-${kind%%:*}=$dir/worlds/$world/$file"
    done
  done
}

make_world_staging() {
  local root="$1" id="$2" sequence="$3" expiry="$4" variant="${5:-}" mismatch="${6:-0}" dir destination index args=()
  dir="$root/staging/$id"
  mkdir -p "$dir/app" "$dir/runtime" "$dir/bridge" "$dir/trust"
  index=0
  for destination in \
    app/coop-launcher.exe runtime/mgba.exe runtime/game.gba runtime/coop-sidecar.exe \
    bridge/main.lua bridge/memory.lua bridge/protocol.lua bridge/generated_addresses.lua \
    bridge_manifest.json trust/release-trust.json THIRD_PARTY_NOTICES.txt; do
    printf 'fixture-%s-%s-%s\n' "$id" "$index" "$variant" > "$dir/$destination"
    index=$((index + 1))
  done
  write_world_files "$dir" "$dir/runtime/game.gba" "$dir/bridge_manifest.json" "$variant" "$mismatch"
  for destination in desktop-app=app/coop-launcher.exe managed-mgba=runtime/mgba.exe \
    rom=runtime/game.gba sidecar=runtime/coop-sidecar.exe bridge-main=bridge/main.lua \
    bridge-memory=bridge/memory.lua bridge-protocol=bridge/protocol.lua \
    bridge-addresses=bridge/generated_addresses.lua compatibility-manifest=bridge_manifest.json \
    trust-bundle=trust/release-trust.json notices=THIRD_PARTY_NOTICES.txt; do
    args+=(--artifact "${destination%%=*}=$dir/${destination#*=}")
  done
  while IFS= read -r destination; do args+=(--artifact "$destination"); done < <(world_artifact_args "$dir")
  "$COOP_RELEASE_TOOL" sign --release-id "$id" --sequence "$sequence" \
    --issued-at "$((expiry - 3600))" --expires-at "$expiry" --key-id pilot-v1 \
    --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX" \
    --output "$dir/release-envelope.json" "${args[@]}" >/dev/null
}

make_world_game_staging() {
  local root="$1" id="$2" sequence="$3" issued="$4" variant="${5:-original}" mismatch="${6:-0}" dir args=() line
  dir="$root/game-staging/$id"
  rm -rf -- "$dir"
  mkdir -p "$dir"
  printf 'private-rom-%s-%s\n' "$id" "$variant" > "$dir/game.gba"
  printf '{"fixture":"%s"}\n' "$variant" > "$dir/bridge_manifest.json"
  write_world_files "$dir" "$dir/game.gba" "$dir/bridge_manifest.json" "$variant" "$mismatch"
  while IFS= read -r line; do args+=(--artifact "$line"); done < <(world_artifact_args "$dir")
  "$COOP_RELEASE_TOOL" sign-game --release-id "$id" --sequence "$sequence" \
    --issued-at "$issued" --expires-at "$((issued + 3600))" \
    --key-id "$COOP_RELEASE_KEY_ID" --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX" \
    --output "$dir/release-envelope.json" \
    --artifact "rom=$dir/game.gba" --artifact "compatibility-manifest=$dir/bridge_manifest.json" \
    "${args[@]}" >/dev/null
}

# Prints the digest of a staged server catalog with two pinned arrival saves.
# mode: ok | oversize | missing | extra
make_server_catalog_staging() {
  local root="$1" mode="${2:-ok}"
  "$PYTHON_BIN" - "$root" "$mode" <<'PY'
import hashlib, json, pathlib, sys
root, mode = pathlib.Path(sys.argv[1]), sys.argv[2]
tmp = root / "server-catalog-build"
saves = {}
worlds = []
for world in (1, 2):
    data = bytes([world]) * 131072
    saves[f"worlds/{world}/arrival.sav"] = data
    worlds.append({"world_id": world, "arrivals": [{"id": f"from_{3 - world}",
                   "template_sav_path": f"worlds/{world}/arrival.sav",
                   "template_sav_sha256": hashlib.sha256(data).hexdigest()}]})
raw = json.dumps({"schema_version": 3, "worlds": worlds, **({} if mode == "ok" else {"fixture": mode})}, sort_keys=True).encode()
if mode == "oversize":
    raw += b" " * (65 * 1024)
raw += b"\n"
digest = hashlib.sha256(raw).hexdigest()
stage = root / "server-catalog-staging" / digest
stage.mkdir(parents=True, exist_ok=True)
(stage / "server-build-catalog.json").write_bytes(raw)
for relative, data in saves.items():
    if mode == "missing" and relative.startswith("worlds/2/"):
        continue
    (stage / relative).parent.mkdir(parents=True, exist_ok=True)
    (stage / relative).write_bytes(data)
if mode == "extra":
    (stage / "worlds/2/notes.txt").write_text("unexpected\n")
print(digest)
PY
}

# Server catalog promotion.
CATALOG_OK="$(make_server_catalog_staging "$ROOT")"
out="$(bash "$PROMOTE_CATALOG" "$CATALOG_OK" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ -f "$ROOT/server-catalog/$CATALOG_OK/server-build-catalog.json" ] && \
  [ -f "$ROOT/server-catalog/$CATALOG_OK/worlds/2/arrival.sav" ] && \
  [ ! -w "$ROOT/server-catalog/$CATALOG_OK/server-build-catalog.json" ] && \
  [ -r "$ROOT/server-catalog/$CATALOG_OK/worlds/1/arrival.sav" ] && \
  [ ! -d "$ROOT/server-catalog-staging/$CATALOG_OK" ]
report $? "server catalog promotes read-only into server-catalog/<sha>" "$out"
make_server_catalog_staging "$ROOT" >/dev/null
out="$(bash "$PROMOTE_CATALOG" "$CATALOG_OK" 2>&1)"; status=$?
[ "$status" -eq 0 ] && printf '%s' "$out" | grep -q 'already promoted' && [ ! -d "$ROOT/server-catalog-staging/$CATALOG_OK" ]
report $? "server catalog rerun reuses the promoted copy and drops identical staging" "$out"
out="$(bash "$PROMOTE_CATALOG" "$DIGEST_A" 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ ! -d "$ROOT/server-catalog/$DIGEST_A" ]
report $? "server catalog without staging is rejected" "$out"
bad="$(make_server_catalog_staging "$ROOT" missing)"
mv "$ROOT/server-catalog-staging/$bad" "$ROOT/server-catalog-staging/$DIGEST_B"
out="$(bash "$PROMOTE_CATALOG" "$DIGEST_B" 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'do not match' && [ ! -d "$ROOT/server-catalog/$DIGEST_B" ]
report $? "server catalog with a bad hash is rejected" "$out"
rm -rf "$ROOT/server-catalog-staging/$DIGEST_B"
bad="$(make_server_catalog_staging "$ROOT" oversize)"
out="$(bash "$PROMOTE_CATALOG" "$bad" 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q '64 KiB' && [ ! -d "$ROOT/server-catalog/$bad" ]
report $? "oversized server catalog is rejected" "$out"
rm -rf "$ROOT/server-catalog-staging/$bad"
bad="$(make_server_catalog_staging "$ROOT" missing)"
out="$(bash "$PROMOTE_CATALOG" "$bad" 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'missing' && [ ! -d "$ROOT/server-catalog/$bad" ]
report $? "server catalog with a missing arrival save is rejected" "$out"
rm -rf "$ROOT/server-catalog-staging/$bad"
bad="$(make_server_catalog_staging "$ROOT" extra)"
out="$(bash "$PROMOTE_CATALOG" "$bad" 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'unexpected' && [ ! -d "$ROOT/server-catalog/$bad" ]
report $? "server catalog with an extra file is rejected" "$out"
rm -rf "$ROOT/server-catalog-staging/$bad"

# Multi-world Windows releases.
current_before="$(cat "$ROOT/current")"
make_world_staging "$ROOT" "$FULL_E" 7 "$((NOW + 3600))"
out="$($PROMOTE "$FULL_E" --image-ref "$IMAGE_E" --image-digest "sha256:$DIGEST_E" --server-catalog-sha256 "$CATALOG_OK" --no-flip 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ -f "$ROOT/releases/$FULL_E/worlds/2/player_transfer.json" ] && \
  [ -f "$ROOT/releases/$FULL_E/release_catalog.json" ] && [ "$(cat "$ROOT/current")" = "$current_before" ] && \
  printf '%s' "$out" | grep -q 'without activation'
report $? "multi-world release promotes with --no-flip and leaves current alone" "$out"
grep -q '"schema":2' "$ROOT/release-metadata/$FULL_E.json" && grep -q "\"server_catalog_sha256\":\"$CATALOG_OK\"" "$ROOT/release-metadata/$FULL_E.json"
report $? "release metadata schema 2 records the server catalog digest"
out="$(probe_status "$FULL_E" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(printf '%s\n' "$out" | wc -l)" -eq 4 ] && \
  printf '%s\n' "$out" | grep -q '^state=RELEASED$' && \
  printf '%s\n' "$out" | grep -q "^server_catalog_sha256=$CATALOG_OK$"
report $? "probe returns four lines including the server catalog digest" "$out"
out="$(probe_status "$FULL_A" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(printf '%s\n' "$out" | wc -l)" -eq 4 ] && printf '%s\n' "$out" | grep -q '^server_catalog_sha256=$'
report $? "probe reports an empty catalog digest for a schema-1 association" "$out"
out="$(probe_status "$FULL_G" "$IMAGE_BASE" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(printf '%s\n' "$out" | wc -l)" -eq 4 ] && printf '%s\n' "$out" | grep -q '^state=ABSENT$'
report $? "probe ABSENT record has four lines" "$out"
make_world_staging "$ROOT" "$FULL_E" 7 "$((NOW + 3600))"
out="$($PROMOTE "$FULL_E" --image-ref "$IMAGE_E" --image-digest "sha256:$DIGEST_E" --server-catalog-sha256 "$CATALOG_OK" --no-flip 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ ! -d "$ROOT/staging/$FULL_E" ] && [ "$(cat "$ROOT/current")" = "$current_before" ]
report $? "multi-world rerun reuses the promoted release without flipping" "$out"
out="$($PROMOTE "$FULL_E" --server-catalog-sha256 "$DIGEST_A" --no-flip 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'server catalog association conflict'
report $? "conflicting server catalog association is rejected" "$out"

make_world_staging "$ROOT" "$FULL_F" 8 "$((NOW + 3600))"
rm -f "$ROOT/staging/$FULL_F/worlds/2/player_transfer.json"
out="$($PROMOTE "$FULL_F" --image-ref "$IMAGE_F" --image-digest "sha256:$DIGEST_F" --no-flip 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ ! -d "$ROOT/releases/$FULL_F" ] && [ ! -e "$ROOT/release-metadata/$FULL_F.json" ]
report $? "missing signed world file is rejected" "$out"
rm -rf "$ROOT/staging/$FULL_F"
make_world_staging "$ROOT" "$FULL_F" 8 "$((NOW + 3600))"
printf 'stray\n' > "$ROOT/staging/$FULL_F/worlds/2/extra.txt"
out="$($PROMOTE "$FULL_F" --image-ref "$IMAGE_F" --image-digest "sha256:$DIGEST_F" --no-flip 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'not exactly the signed artifacts' && [ ! -d "$ROOT/releases/$FULL_F" ]
report $? "extra file in a world directory is rejected" "$out"
rm -rf "$ROOT/staging/$FULL_F"
make_world_staging "$ROOT" "$FULL_F" 8 "$((NOW + 3600))"
mkdir -p "$ROOT/staging/$FULL_F/worlds/3"
out="$($PROMOTE "$FULL_F" --image-ref "$IMAGE_F" --image-digest "sha256:$DIGEST_F" --no-flip 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'unexpected directory' && [ ! -d "$ROOT/releases/$FULL_F" ]
report $? "unsigned extra world directory is rejected" "$out"
rm -rf "$ROOT/staging/$FULL_F"
make_world_staging "$ROOT" "$FULL_F" 8 "$((NOW + 3600))" "" 2
out="$($PROMOTE "$FULL_F" --image-ref "$IMAGE_F" --image-digest "sha256:$DIGEST_F" --no-flip 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'region catalog does not bind world 2' && [ ! -d "$ROOT/releases/$FULL_F" ]
report $? "signed region catalog that disagrees with world artifacts is rejected" "$out"
rm -rf "$ROOT/staging/$FULL_F"

# Multi-world game releases.
game_before="$(cat "$ROOT/game/current")"
make_world_game_staging "$ROOT" "$FULL_E" 9 "$NOW"
out="$(bash "$PROMOTE_GAME" "$FULL_E" --no-flip 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ -f "$ROOT/game/$FULL_E/worlds/2/game.gba" ] && [ "$(cat "$ROOT/game/current")" = "$game_before" ] && \
  [ ! -d "$ROOT/game-staging/$FULL_E" ]
report $? "multi-world game release promotes with --no-flip" "$out"
make_world_game_staging "$ROOT" "$FULL_E" 9 "$((NOW + 10))"
out="$(bash "$PROMOTE_GAME" "$FULL_E" --no-flip 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/game/current")" = "$game_before" ] && [ ! -d "$ROOT/game-staging/$FULL_E" ]
report $? "re-signed multi-world game rerun is idempotent" "$out"
make_world_game_staging "$ROOT" "$FULL_F" 10 "$NOW"
printf 'stray\n' > "$ROOT/game-staging/$FULL_F/worlds/1/extra.bin"
out="$(bash "$PROMOTE_GAME" "$FULL_F" --no-flip 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ ! -d "$ROOT/game/$FULL_F" ]
report $? "multi-world game release with an extra file is rejected" "$out"
make_world_game_staging "$ROOT" "$FULL_F" 10 "$NOW"
rm -f "$ROOT/game-staging/$FULL_F/worlds/2/bridge_manifest.json"
out="$(bash "$PROMOTE_GAME" "$FULL_F" --no-flip 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ ! -d "$ROOT/game/$FULL_F" ]
report $? "multi-world game release with a missing world file is rejected" "$out"
make_world_game_staging "$ROOT" "$FULL_F" 10 "$NOW" original 2
out="$(bash "$PROMOTE_GAME" "$FULL_F" --no-flip 2>&1)"; status=$?
[ "$status" -ne 0 ] && printf '%s' "$out" | grep -q 'region catalog does not bind' && [ ! -d "$ROOT/game/$FULL_F" ]
report $? "game region catalog mismatch is rejected" "$out"
rm -rf "$ROOT/game-staging/$FULL_F"

# Activation flips all markers only after promotion, and is idempotent.
out="$(bash "$ACTIVATE" "$FULL_F" 2>&1)"; status=$?
[ "$status" -ne 0 ] && [ "$(cat "$ROOT/current")" = "$current_before" ] && [ "$(cat "$ROOT/game/current")" = "$game_before" ]
report $? "activation refuses a release that is not promoted" "$out"
mkdir -p "$ROOT/android/$FULL_E"
printf 'apk-bytes\n' > "$ROOT/android/$FULL_E/app-release.apk"
"$PYTHON_BIN" - "$ROOT/android/$FULL_E" "$FULL_E" <<'PY'
import hashlib, json, pathlib, sys
d = pathlib.Path(sys.argv[1]); apk = (d / "app-release.apk").read_bytes()
(d / "metadata.json").write_text(json.dumps({"release_id": sys.argv[2], "version_code": 7,
    "size": len(apk), "sha256": hashlib.sha256(apk).hexdigest()}))
PY
out="$(COOP_ACTIVATE_SKIP_ACL=1 bash "$ACTIVATE" "$FULL_E" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/current")" = "$FULL_E" ] && [ "$(cat "$ROOT/game/current")" = "$FULL_E" ] && \
  [ "$(cat "$ROOT/.previous-release")" = "$current_before" ] && [ "$(cat "$ROOT/android/current")" = "$FULL_E" ]
report $? "activation flips game, runtime and Android markers" "$out"
out="$(COOP_ACTIVATE_SKIP_ACL=1 bash "$ACTIVATE" "$FULL_E" 2>&1)"; status=$?
[ "$status" -eq 0 ] && [ "$(cat "$ROOT/.previous-release")" = "$current_before" ] && ! printf '%s' "$out" | grep -q 'activated'
report $? "activation rerun is an idempotent no-op" "$out"

# deploy-release.sh against a fake docker on PATH.
FAKEBIN="$ROOT/fakebin"
mkdir -p "$FAKEBIN"
cat > "$FAKEBIN/docker" <<'SH'
#!/usr/bin/env bash
envf=""
prev=""
for arg in "$@"; do
  [ "$prev" = --env-file ] && envf="$arg"
  prev="$arg"
done
image=""
[ -z "$envf" ] || image="$(sed -n 's/^COOP_IMAGE=//p' "$envf")"
printf '%s | image=%s\n' "$*" "$image" >> "$FAKE_DOCKER_LOG"
case "$1" in
  pull) [ "${FAKE_PULL_FAIL:-0}" = 1 ] && exit 1; exit 0 ;;
  login) cat >/dev/null; exit 0 ;;
  compose)
    case " $* " in
      *" exec "*) [ -n "$image" ] && [ "$image" = "${FAKE_HEALTHY_IMAGE:-}" ] && exit 0; exit 1 ;;
      *) exit 0 ;;
    esac ;;
esac
exit 0
SH
chmod +x "$FAKEBIN/docker"
DEPLOY_DIR_T="$ROOT/deploy"
mkdir -p "$DEPLOY_DIR_T"
cp compose.yaml "$DEPLOY_DIR_T/compose.yaml"
OLD_IMAGE="$IMAGE_A"
NEW_IMAGE="$IMAGE_G"
CATALOG_PATH_T="$ROOT/server-catalog/$CATALOG_OK/server-build-catalog.json"
reset_deploy_dir() {
  printf '# production settings\nCOOP_DOMAIN=coop.example.com\nCOOP_IMAGE=%s\nCOOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/old/server-build-catalog.json\nCOOP_PHASE2_RELEASE_CATALOG_SHA256=%s\nCOOP_FIREBASE_BUCKET=bucket\n' "$OLD_IMAGE" "$DIGEST_D" > "$DEPLOY_DIR_T/.env"
  chmod 0640 "$DEPLOY_DIR_T/.env"
  rm -f "$DEPLOY_DIR_T"/.env.bak.* "$DEPLOY_DIR_T"/compose.yaml.bak.* "$DEPLOY_DIR_T/.env.next"
  cp compose.yaml "$DEPLOY_DIR_T/compose.yaml"
  cp "$DEPLOY_DIR_T/.env" "$ROOT/env.before"
  cp "$DEPLOY_DIR_T/compose.yaml" "$ROOT/compose.before"
  : > "$ROOT/docker.log"
}
run_deploy() {
  PATH="$FAKEBIN:$PATH" FAKE_DOCKER_LOG="$ROOT/docker.log" HOENN_ROOT="$ROOT" HEALTH_TIMEOUT=0 HEALTH_INTERVAL=0 \
    bash "$DEPLOY" --deploy-dir "$DEPLOY_DIR_T" "$@"
}
cp compose.yaml "$ROOT/candidate-compose.yaml"
printf '# candidate change\n' >> "$ROOT/candidate-compose.yaml"

reset_deploy_dir
out="$(FAKE_HEALTHY_IMAGE="$NEW_IMAGE" run_deploy --image "$NEW_IMAGE" --catalog-path "$CATALOG_PATH_T" --catalog-sha256 "$CATALOG_OK" --compose-file "$ROOT/candidate-compose.yaml" 2>&1)"; status=$?
[ "$status" -eq 0 ] && grep -qx "COOP_IMAGE=$NEW_IMAGE" "$DEPLOY_DIR_T/.env" && \
  grep -qx "COOP_PHASE2_RELEASE_CATALOG_SHA256=$CATALOG_OK" "$DEPLOY_DIR_T/.env" && \
  grep -qx "COOP_PHASE2_RELEASE_CATALOG_PATH=$CATALOG_PATH_T" "$DEPLOY_DIR_T/.env" && \
  grep -qx 'COOP_FIREBASE_BUCKET=bucket' "$DEPLOY_DIR_T/.env" && grep -qx '# production settings' "$DEPLOY_DIR_T/.env" && \
  [ "$(grep -c '^COOP_IMAGE=' "$DEPLOY_DIR_T/.env")" -eq 1 ] && \
  cmp -s "$DEPLOY_DIR_T/compose.yaml" "$ROOT/candidate-compose.yaml" && \
  ls "$DEPLOY_DIR_T"/.env.bak.* >/dev/null 2>&1 && ls "$DEPLOY_DIR_T"/compose.yaml.bak.* >/dev/null 2>&1 && \
  [ "$(sed -n '1p' "$ROOT/docker.log" | cut -d' ' -f1)" = pull ]
report $? "deploy-release pulls first, rewrites only release keys and keeps backups" "$out"
if [ "$(stat -c %a "$DEPLOY_DIR_T/.env" 2>/dev/null)" = 640 ] || [ "$(uname -o 2>/dev/null)" = Msys ]; then
  report 0 "deploy-release preserves the .env file mode"
else
  report 1 "deploy-release preserves the .env file mode" "$(stat -c %a "$DEPLOY_DIR_T/.env")"
fi

reset_deploy_dir
out="$(FAKE_HEALTHY_IMAGE="$OLD_IMAGE" run_deploy --image "$NEW_IMAGE" --catalog-path "$CATALOG_PATH_T" --catalog-sha256 "$CATALOG_OK" --compose-file "$ROOT/candidate-compose.yaml" 2>&1)"; status=$?
[ "$status" -eq 3 ] && cmp -s "$DEPLOY_DIR_T/.env" "$ROOT/env.before" && cmp -s "$DEPLOY_DIR_T/compose.yaml" "$ROOT/compose.before" && \
  grep -q "up -d --no-build server | image=$NEW_IMAGE" "$ROOT/docker.log" && \
  [ "$(grep ' up -d --no-build server ' "$ROOT/docker.log" | tail -n 1 | sed 's/.*image=//')" = "$OLD_IMAGE" ] && \
  [ ! -e "$DEPLOY_DIR_T/.env.next" ]
report $? "health failure restores .env and compose byte-for-byte and brings the old image up" "$out"

reset_deploy_dir
out="$(FAKE_HEALTHY_IMAGE=none run_deploy --image "$NEW_IMAGE" --catalog-path "$CATALOG_PATH_T" --catalog-sha256 "$CATALOG_OK" 2>&1)"; status=$?
[ "$status" -eq 4 ] && cmp -s "$DEPLOY_DIR_T/.env" "$ROOT/env.before" && printf '%s' "$out" | grep -q 'ROLLBACK FAILED'
report $? "failed rollback exits 4 with restored files" "$out"

reset_deploy_dir
out="$(FAKE_PULL_FAIL=1 FAKE_HEALTHY_IMAGE="$NEW_IMAGE" run_deploy --image "$NEW_IMAGE" --catalog-path "$CATALOG_PATH_T" --catalog-sha256 "$CATALOG_OK" 2>&1)"; status=$?
[ "$status" -eq 1 ] && cmp -s "$DEPLOY_DIR_T/.env" "$ROOT/env.before" && ! ls "$DEPLOY_DIR_T"/.env.bak.* >/dev/null 2>&1 && \
  ! grep -q ' up ' "$ROOT/docker.log" && [ ! -e "$DEPLOY_DIR_T/.env.next" ]
report $? "pull failure leaves .env untouched and never restarts" "$out"

reset_deploy_dir
out="$(FAKE_HEALTHY_IMAGE="$NEW_IMAGE" run_deploy --image "$NEW_IMAGE" --catalog-path "$CATALOG_PATH_T" --catalog-sha256 "$DIGEST_A" 2>&1)"; status=$?
[ "$status" -ne 0 ] && cmp -s "$DEPLOY_DIR_T/.env" "$ROOT/env.before" && [ ! -s "$ROOT/docker.log" ]
report $? "deploy-release rejects a catalog path/digest mismatch before any docker call" "$out"
printf 'tampered' >> "$ROOT/catalog-copy.json"
out="$(FAKE_HEALTHY_IMAGE="$NEW_IMAGE" run_deploy --image "$NEW_IMAGE" --catalog-path "$ROOT/catalog-copy.json" --catalog-sha256 "$CATALOG_OK" 2>&1)"; status=$?
[ "$status" -eq 2 ] && [ ! -s "$ROOT/docker.log" ]
report $? "deploy-release requires the promoted content-addressed catalog path" "$out"
out="$(FAKE_HEALTHY_IMAGE="$NEW_IMAGE" run_deploy --image "${NEW_IMAGE%@*}:latest" --catalog-path "$CATALOG_PATH_T" --catalog-sha256 "$CATALOG_OK" 2>&1)"; status=$?
[ "$status" -eq 2 ] && [ ! -s "$ROOT/docker.log" ]
report $? "deploy-release still refuses mutable tags" "$out"

reset_deploy_dir
out="$(run_deploy --validate-only --image "$NEW_IMAGE" --catalog-path "$CATALOG_PATH_T" --catalog-sha256 "$CATALOG_OK" --compose-file "$ROOT/candidate-compose.yaml" 2>&1)"; status=$?
[ "$status" -eq 0 ] && cmp -s "$DEPLOY_DIR_T/.env" "$ROOT/env.before" && ! grep -q -e '^pull' -e ' up ' "$ROOT/docker.log" && \
  grep -q 'config --quiet' "$ROOT/docker.log" && ! ls "$DEPLOY_DIR_T"/.env.validate.* >/dev/null 2>&1
report $? "deploy-release --validate-only checks the candidate without changes" "$out"

# Workflow ordering and private-artifact boundary for the multi-world rollout.
line_of() { grep -n -- "$1" "$WORKFLOW" | head -n 1 | cut -d: -f1; }
l_catalog="$(line_of 'bash /tmp/promote-server-catalog.sh ')"
l_game="$(line_of 'bash /tmp/promote-game.sh .* --no-flip')"
l_release="$(line_of 'bash /tmp/promote-release.sh .*--no-flip')"
l_deploy="$(line_of 'bash /tmp/deploy-release.sh --image')"
l_activate="$(line_of 'bash /tmp/activate-release.sh')"
if [ -n "$l_catalog" ] && [ -n "$l_game" ] && [ -n "$l_release" ] && [ -n "$l_deploy" ] && [ -n "$l_activate" ] && \
   [ "$l_catalog" -lt "$l_game" ] && [ "$l_game" -lt "$l_release" ] && [ "$l_release" -lt "$l_deploy" ] && \
   [ "$l_deploy" -lt "$l_activate" ]; then
  report 0 "workflow promotes catalog, game, release, then deploys, then activates"
else
  report 1 "workflow promotes catalog, game, release, then deploys, then activates" \
    "catalog=$l_catalog game=$l_game release=$l_release deploy=$l_deploy activate=$l_activate"
fi
MULTIWORLD_WORKFLOW=../../.github/workflows/multiworld-build.yml
if grep -q 'multiworld_build_ci.py build' "$WORKFLOW" && grep -q 'assemble_release_catalog.py assemble' "$WORKFLOW" && \
   ! grep -q 'test-only-provisional-object-catalog' "$WORKFLOW" && \
   ! grep -A12 'upload-artifact@' "$WORKFLOW" | grep -q -e '\.gba' -e 'dist/multiworld' -e 'release-bundle' -e 'game-bundle' -e 'release-assembly' && \
   ! awk '/^  release-dryrun:/{f=1} f' "$MULTIWORLD_WORKFLOW" | grep -q 'upload-artifact' && \
   grep -q 'cmp -s dist/multiworld/main/game.gba pokeemerald.gba' "$WORKFLOW" && \
   grep -q 'coop-release-tool verify-game' "$WORKFLOW" && \
   grep -q 'server_catalog_sha256=' "$WORKFLOW" && grep -q 'wc -l)" -eq 4' "$WORKFLOW"; then
  report 0 "workflow builds every world, never uploads ROMs and never ships the provisional digest"
else
  report 1 "workflow builds every world, never uploads ROMs and never ships the provisional digest"
fi
if grep -q 'fresh-start' "$WORKFLOW" && ! grep -q 'coop-server fresh-start' "$WORKFLOW"; then
  report 0 "workflow documents the manual fresh-start hook without running it"
else
  report 1 "workflow documents the manual fresh-start hook without running it"
fi
for script in "$PROMOTE_CATALOG" "$ACTIVATE" "$DEPLOY" "$PROMOTE" "$PROBE"; do
  bash -n "$script" && grep -q "bash -n deploy/coop/${script#./}" "$WORKFLOW"
  report $? "workflow syntax checks ${script#./}"
done

bash -n "$PROMOTE_GAME"; status=$?
[ "$status" -eq 0 ] && grep -q 'bash -n deploy/coop/promote-game.sh' "$WORKFLOW"
report $? "workflow syntax checks game promotion script"

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
   ! grep -q '"version": "9.0.203"' "$REPO_ROOT/global.json" || \
   ! grep -q '"rollForward": "disable"' "$REPO_ROOT/global.json" || \
   grep -q 'PersistKeySet' "$WORKFLOW" || \
   grep -q 'HOENN_SIGNING_PASSWORD\|AuthenticodeCertificatePath' "$REPO_ROOT/installer/windows/build-installer.ps1" || \
   grep -Eq '(^|[^[:alnum:]])/p([^[:alnum:]]|$)' "$REPO_ROOT/installer/windows/build-installer.ps1"; then
  report 1 "workflow actions/dotnet and secure installer signing pins"
else
  report 0 "workflow actions/dotnet and secure installer signing pins"
fi
if grep -q 'WINDOWS_INSTALLER_SIGNING_MODE: unsigned-private-pilot' "$WORKFLOW" && \
   grep -A2 '^  installer:' "$WORKFLOW" | grep -q "if: needs.release.outputs.installer_build == 'true'" && \
   grep -q 'if \[ "$EVENT_NAME" = workflow_dispatch \]; then' "$WORKFLOW" && \
   grep -q 'git diff --quiet "$published_sha" HEAD --' "$WORKFLOW" && \
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
if grep -q 'scp deploy/coop/compose.yaml hoenn-vps:/tmp/hoenn-compose.yaml' "$WORKFLOW" && \
   grep -q -- '--compose-file /tmp/hoenn-compose.yaml' "$WORKFLOW" && \
   grep -q -- '--compose-file)' deploy-release.sh; then
  report 0 "workflow atomically syncs version-controlled Compose configuration"
else
  report 1 "workflow atomically syncs version-controlled Compose configuration"
fi
if grep -q 'COOP_RELEASE_ROOT: /srv/hoenn' compose.yaml && \
   grep -q '/srv/hoenn:/srv/hoenn:ro' compose.yaml; then
  report 0 "compose wires read-only release parent"
else
  report 1 "compose wires read-only release parent"
fi

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
