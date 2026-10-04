#!/usr/bin/env bash
# Exercise the multi-world release pipeline end to end without any VPS,
# production key, registry push or artifact upload.
#
# Steps: compare the built ROM hashes with data/release_arrivals.json (exit 3
# with a recertification message when they differ) -> assemble both catalogs
# with the TEST-ONLY provisional object catalog digest -> sign Windows and game
# envelopes with an ephemeral in-memory seed and verify them -> promote the
# server catalog, game (--no-flip) and runtime release (--no-flip) into a
# temporary root -> activate -> deploy-release.sh --validate-only (when Docker
# Compose is available) -> start `coop-server --phase2-local` against the
# promoted server catalog and require /health/ready 200.
#
# Placeholder bytes stand in for the desktop app, mGBA and sidecar; ROMs,
# manifests, catalogs and arrival saves are the real build outputs.
#
# Usage: release-dryrun.sh --dist DIR --work DIR --release-tool PATH
#          --server PATH [--port N] [--arrival-verifier PATH] [--require-docker]
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
DIST=""
WORK=""
RELEASE_TOOL=""
SERVER_BIN=""
PORT=18089
ARRIVAL_VERIFIER=""
REQUIRE_DOCKER=0
PYTHON_BIN="${PYTHON_BIN:-python3}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dist) DIST="$2"; shift 2 ;;
    --work) WORK="$2"; shift 2 ;;
    --release-tool) RELEASE_TOOL="$2"; shift 2 ;;
    --server) SERVER_BIN="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    --arrival-verifier) ARRIVAL_VERIFIER="$2"; shift 2 ;;
    --require-docker) REQUIRE_DOCKER=1; shift ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$DIST" ] && [ -n "$WORK" ] && [ -x "$RELEASE_TOOL" ] && [ -x "$SERVER_BIN" ] || {
  echo "usage: $0 --dist DIR --work DIR --release-tool PATH --server PATH" >&2; exit 2; }
[ ! -e "$WORK" ] || { echo "error: refusing to reuse $WORK" >&2; exit 2; }
DIST="$(cd -- "$DIST" && pwd)"
mkdir -p "$WORK"
WORK="$(cd -- "$WORK" && pwd)"
cd -- "$REPO_ROOT"

echo "== CI ROM hashes vs attested arrival saves"
"$PYTHON_BIN" - "$DIST" data/release_arrivals.json <<'PY'
import hashlib, json, pathlib, sys
dist, attested = pathlib.Path(sys.argv[1]), json.loads(pathlib.Path(sys.argv[2]).read_text())
mismatch = []
for name, world in sorted(attested["worlds"].items()):
    built = hashlib.sha256((dist / name / "game.gba").read_bytes()).hexdigest()
    for portal, proof in sorted(world["arrivals"].items()):
        state = "match" if built == proof["rom_sha256"] else "DIFFERS"
        print(f"{name}: built {built} attested {proof['rom_sha256']} ({portal}) {state}")
        if built != proof["rom_sha256"]:
            mismatch.append(name)
if mismatch:
    print("recertify arrival saves: this build does not reproduce the attested ROM bytes for "
          + ", ".join(sorted(set(mismatch))) + " (see deploy/coop/RELEASES.md)", file=sys.stderr)
    raise SystemExit(3)
PY

echo "== assemble catalogs (TEST-ONLY provisional object catalog digest)"
assemble_args=(assemble --dist "$DIST" --out "$WORK/assembly" --test-only-provisional-object-catalog)
[ -z "$ARRIVAL_VERIFIER" ] || assemble_args+=(--arrival-verifier "$ARRIVAL_VERIFIER")
"$PYTHON_BIN" tools/coop/assemble_release_catalog.py "${assemble_args[@]}" > "$WORK/assembly.stdout"
SERVER_CATALOG="$("$PYTHON_BIN" -c 'import json,sys; print(json.load(open(sys.argv[1]))["server_build_catalog_sha256"])' "$WORK/assembly/assembly.json")"
REGION_CATALOG="$("$PYTHON_BIN" -c 'import json,sys; print(json.load(open(sys.argv[1]))["release_catalog_sha256"])' "$WORK/assembly/assembly.json")"
echo "region catalog $REGION_CATALOG"
echo "server catalog $SERVER_CATALOG"

RELEASE_ID="$("$PYTHON_BIN" -c 'import hashlib,sys; print(hashlib.sha1(sys.argv[1].encode()).hexdigest())' "dryrun-$SERVER_CATALOG")"
HOENN="$WORK/hoenn"
STAGE="$HOENN/staging/$RELEASE_ID"
GAME="$HOENN/game-staging/$RELEASE_ID"
mkdir -p "$STAGE/app" "$STAGE/runtime" "$STAGE/bridge" "$STAGE/trust" "$GAME"
HOENN_RELEASE_PRIVATE_SEED_HEX="$("$PYTHON_BIN" -c 'import secrets; print(secrets.token_hex(32))')"
export HOENN_RELEASE_PRIVATE_SEED_HEX
KEY_ID=dryrun-ephemeral
PUBLIC_KEY="$("$RELEASE_TOOL" public-key)"
printf 'DRY-RUN placeholder desktop\n' > "$STAGE/app/coop-launcher.exe"
printf 'DRY-RUN placeholder mgba\n' > "$STAGE/runtime/mgba.exe"
printf 'DRY-RUN placeholder sidecar\n' > "$STAGE/runtime/coop-sidecar.exe"
printf 'DRY-RUN notices\n' > "$STAGE/THIRD_PARTY_NOTICES.txt"
cp "$DIST/main/game.gba" "$STAGE/runtime/game.gba"
cp "$DIST/main/bridge_manifest.json" "$STAGE/bridge_manifest.json"
cp "$DIST/main/generated_addresses.lua" "$STAGE/bridge/generated_addresses.lua"
for lua in main memory protocol; do cp "bridge/$lua.lua" "$STAGE/bridge/$lua.lua"; done
printf '{"algorithm":"ed25519","key_id":"%s","public_key_hex":"%s","schema":1}\n' "$KEY_ID" "$PUBLIC_KEY" > "$STAGE/trust/release-trust.json"
cp "$DIST/main/game.gba" "$GAME/game.gba"
cp "$DIST/main/bridge_manifest.json" "$GAME/bridge_manifest.json"

collect() {
  local kind="$1" bundle="$2" line
  ARTIFACTS=()
  while IFS= read -r line; do ARTIFACTS+=(--artifact "$line"); done < <(
    "$PYTHON_BIN" tools/coop/assemble_release_catalog.py signing-artifacts \
      --catalog-dir "$WORK/assembly/catalog" --bundle "$bundle" --kind "$kind")
}
issued="$(date +%s)"
expires=$((issued + 3600))
echo "== sign and verify (ephemeral key $PUBLIC_KEY)"
collect windows "$STAGE"
"$RELEASE_TOOL" sign --release-id "$RELEASE_ID" --sequence 1 --issued-at "$issued" --expires-at "$expires" \
  --key-id "$KEY_ID" --public-key-hex "$PUBLIC_KEY" --output "$STAGE/release-envelope.json" \
  --artifact "desktop-app=$STAGE/app/coop-launcher.exe" --artifact "managed-mgba=$STAGE/runtime/mgba.exe" \
  --artifact "rom=$STAGE/runtime/game.gba" --artifact "sidecar=$STAGE/runtime/coop-sidecar.exe" \
  --artifact "bridge-main=$STAGE/bridge/main.lua" --artifact "bridge-memory=$STAGE/bridge/memory.lua" \
  --artifact "bridge-protocol=$STAGE/bridge/protocol.lua" \
  --artifact "bridge-addresses=$STAGE/bridge/generated_addresses.lua" \
  --artifact "compatibility-manifest=$STAGE/bridge_manifest.json" \
  --artifact "trust-bundle=$STAGE/trust/release-trust.json" \
  --artifact "notices=$STAGE/THIRD_PARTY_NOTICES.txt" "${ARTIFACTS[@]}"
"$RELEASE_TOOL" verify --envelope "$STAGE/release-envelope.json" --key-id "$KEY_ID" --public-key-hex "$PUBLIC_KEY"
collect game "$GAME"
"$RELEASE_TOOL" sign-game --release-id "$RELEASE_ID" --sequence 1 --issued-at "$issued" --expires-at "$expires" \
  --key-id "$KEY_ID" --public-key-hex "$PUBLIC_KEY" --output "$GAME/release-envelope.json" \
  --artifact "rom=$GAME/game.gba" --artifact "compatibility-manifest=$GAME/bridge_manifest.json" "${ARTIFACTS[@]}"
"$RELEASE_TOOL" verify-game --envelope "$GAME/release-envelope.json" --key-id "$KEY_ID" --public-key-hex "$PUBLIC_KEY"
unset HOENN_RELEASE_PRIVATE_SEED_HEX

echo "== promote into $HOENN and activate"
mkdir -p "$HOENN/server-catalog-staging"
cp -R "$WORK/assembly/server-catalog/$SERVER_CATALOG" "$HOENN/server-catalog-staging/"
export HOENN_ROOT="$HOENN" COOP_RELEASE_KEY_ID="$KEY_ID" COOP_RELEASE_PUBLIC_KEY_HEX="$PUBLIC_KEY" COOP_RELEASE_TOOL="$RELEASE_TOOL" PYTHON_BIN
FAKE_DIGEST="sha256:$("$PYTHON_BIN" -c 'import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest())' "$RELEASE_ID")"
bash "$SCRIPT_DIR/promote-server-catalog.sh" "$SERVER_CATALOG"
bash "$SCRIPT_DIR/promote-game.sh" "$RELEASE_ID" --no-flip
bash "$SCRIPT_DIR/promote-release.sh" "$RELEASE_ID" --image-ref "ghcr.io/dryrun/hoenn-sessions-server@$FAKE_DIGEST" \
  --image-digest "$FAKE_DIGEST" --server-catalog-sha256 "$SERVER_CATALOG" --no-flip
[ ! -e "$HOENN/current" ] && [ ! -e "$HOENN/game/current" ] || { echo "error: --no-flip changed a marker" >&2; exit 1; }
bash "$SCRIPT_DIR/activate-release.sh" "$RELEASE_ID"
[ "$(cat "$HOENN/current")" = "$RELEASE_ID" ] && [ "$(cat "$HOENN/game/current")" = "$RELEASE_ID" ]
probe="$(bash "$SCRIPT_DIR/probe-release-status.sh" "$RELEASE_ID" ghcr.io/dryrun/hoenn-sessions-server)"
printf '%s\n' "$probe"
printf '%s\n' "$probe" | grep -qx "server_catalog_sha256=$SERVER_CATALOG"
CATALOG_PATH="$HOENN/server-catalog/$SERVER_CATALOG/server-build-catalog.json"

echo "== deploy-release.sh --validate-only"
if docker compose version >/dev/null 2>&1; then
  DEPLOY="$WORK/deploy"
  mkdir -p "$DEPLOY/secrets"
  cp "$SCRIPT_DIR/compose.yaml" "$SCRIPT_DIR/Caddyfile" "$SCRIPT_DIR/backup.sh" "$SCRIPT_DIR/init-database.sh" "$DEPLOY/"
  for secret in database_admin_password database_password database_url firebase-service-account.json \
    signing_key invite_pepper bootstrap_invite; do printf 'dry-run\n' > "$DEPLOY/secrets/$secret"; done
  printf 'COOP_DOMAIN=dryrun.invalid\nCOOP_FIREBASE_BUCKET=dryrun\nCOOP_IMAGE=ghcr.io/dryrun/old@sha256:%064d\n' 0 > "$DEPLOY/.env"
  bash "$SCRIPT_DIR/deploy-release.sh" --validate-only --deploy-dir "$DEPLOY" \
    --image "ghcr.io/dryrun/hoenn-sessions-server@$FAKE_DIGEST" \
    --catalog-path "$CATALOG_PATH" --catalog-sha256 "$SERVER_CATALOG" --compose-file "$SCRIPT_DIR/compose.yaml"
elif [ "$REQUIRE_DOCKER" -eq 1 ]; then
  echo "error: docker compose is required for the validate-only check" >&2
  exit 1
else
  echo "docker compose unavailable; skipped validate-only check"
fi

echo "== coop-server --phase2-local health on 127.0.0.1:$PORT"
alphabet_secret() { "$PYTHON_BIN" -c 'import secrets,string; print("".join(secrets.choice(string.ascii_lowercase+string.digits) for _ in range(24)))'; }
log="$WORK/server.log"
env -u COOP_SERVER_BIND_ADDR \
  COOP_SERVER_MODE=phase2-local COOP_PHASE2_STORAGE_MODE=phase2-local \
  COOP_PHASE2_INVITE_PEPPER="dryrun-pepper-$(alphabet_secret)" \
  COOP_PHASE2_SIGNING_KEY_HEX="$(printf '07%.0s' $(seq 32))" COOP_PHASE2_SIGNING_KEY_ID=local-test-key \
  COOP_PHASE2_BOOTSTRAP_INVITATION="dryrun-invite-$(alphabet_secret)" \
  COOP_PHASE2_RELEASE_CATALOG_PATH="$CATALOG_PATH" COOP_PHASE2_RELEASE_CATALOG_SHA256="$SERVER_CATALOG" \
  "$SERVER_BIN" --phase2-local --bind "127.0.0.1:$PORT" --release-fixture-root "$HOENN" > "$log" 2>&1 &
server_pid=$!
trap 'kill "$server_pid" 2>/dev/null || true' EXIT
ready=0
for _ in $(seq 120); do
  if ! kill -0 "$server_pid" 2>/dev/null; then
    echo "error: coop-server exited early" >&2; cat "$log" >&2; exit 1
  fi
  code="$("$PYTHON_BIN" - "$PORT" <<'PY'
import sys, urllib.request
try:
    with urllib.request.urlopen(f"http://127.0.0.1:{sys.argv[1]}/health/ready", timeout=1) as r:
        print(r.status)
except Exception:
    print(0)
PY
)"
  if [ "$code" = 200 ]; then ready=1; break; fi
  sleep 0.5
done
[ "$ready" -eq 1 ] || { echo "error: /health/ready never returned 200" >&2; cat "$log" >&2; exit 1; }
echo "coop-server ready (200) with server catalog $SERVER_CATALOG"
echo "dry-run OK: release $RELEASE_ID, region catalog $REGION_CATALOG, server catalog $SERVER_CATALOG"
