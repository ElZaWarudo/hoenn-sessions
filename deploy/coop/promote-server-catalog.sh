#!/usr/bin/env bash
# Verify and promote one content-addressed server build catalog.
#
# The catalog digest is the directory name and the trust anchor: deploy-
# release.sh passes the same digest to the server as
# COOP_PHASE2_RELEASE_CATALOG_SHA256, and the server reads pinned arrival
# saves relative to the catalog's directory.
#
# Layout (HOENN_ROOT defaults to /srv/hoenn):
#   server-catalog-staging/<sha>/   private runner upload, never read
#   server-catalog/<sha>/           server-build-catalog.json and exactly the
#                                   worlds/<N>/*.sav arrival saves it pins;
#                                   read-only and readable by container uid 10001
#
# Checks: lowercase 64-hex digest, catalog <= 64 KiB with that exact SHA-256,
# schema 3, inventory exactly the catalog plus every template_sav_path (each
# under worlds/<world_id>/), each save's size and pinned SHA-256, no symlinks.
# Idempotent: an existing promoted directory is re-verified and an identical
# staging upload is discarded.
#
# Usage: promote-server-catalog.sh <catalog-sha256>
set -euo pipefail

HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
CATALOG_SHA256="${1:?Usage: promote-server-catalog.sh <catalog-sha256>}"
[ "$#" -eq 1 ] || { echo "error: unexpected arguments" >&2; exit 2; }
PYTHON_BIN="${PYTHON_BIN:-python3}"
case "$CATALOG_SHA256" in
  *[!0-9a-f]* | "") echo "error: catalog digest must be lowercase hex" >&2; exit 2 ;;
esac
[ "${#CATALOG_SHA256}" -eq 64 ] || { echo "error: catalog digest must be 64 hex chars" >&2; exit 2; }

STAGING="$HOENN_ROOT/server-catalog-staging/$CATALOG_SHA256"
TARGET_ROOT="$HOENN_ROOT/server-catalog"
TARGET="$TARGET_ROOT/$CATALOG_SHA256"

verify() {
  local dir="$1"
  [ -d "$dir" ] && [ ! -L "$dir" ] || { echo "error: $dir is not a real directory" >&2; return 1; }
  if find "$dir" -type l -print -quit | grep -q .; then
    echo "error: server catalog generation contains a symlink" >&2
    return 1
  fi
  "$PYTHON_BIN" - "$dir" "$CATALOG_SHA256" <<'PY'
import hashlib, json, os, pathlib, re, stat, sys
root = pathlib.Path(sys.argv[1])
expected_digest = sys.argv[2]
MAX_CATALOG = 64 * 1024
SAVE_SIZES = (131072, 131088)
catalog_path = root / "server-build-catalog.json"
try:
    info = catalog_path.stat()
except FileNotFoundError:
    raise SystemExit("server-build-catalog.json is missing")
if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
    raise SystemExit("server catalog is not a regular file")
if info.st_size > MAX_CATALOG:
    raise SystemExit("server catalog exceeds the 64 KiB limit")
raw = catalog_path.read_bytes()
if len(raw) > MAX_CATALOG or hashlib.sha256(raw).hexdigest() != expected_digest:
    raise SystemExit("server catalog bytes do not match the promoted digest")
try:
    catalog = json.loads(raw)
    worlds = catalog["worlds"]
    if catalog.get("schema_version") != 3 or not isinstance(worlds, list) or not worlds:
        raise ValueError
    pinned = {}
    for world in worlds:
        world_id = world["world_id"]
        if type(world_id) is not int or not 1 <= world_id <= 65535:
            raise ValueError
        for arrival in world["arrivals"]:
            path = arrival["template_sav_path"]
            digest = arrival["template_sav_sha256"]
            if (not isinstance(path, str) or not re.fullmatch(rf"worlds/{world_id}/[A-Za-z0-9_.-]+\.sav", path)
                    or not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest)
                    or path in pinned):
                raise ValueError
            pinned[path] = digest
except (KeyError, TypeError, ValueError):
    raise SystemExit("server catalog is not a schema-3 catalog with worlds/<id>/ arrival saves")
found = set()
for current, dirs, files in os.walk(root):
    base = pathlib.Path(current).relative_to(root)
    found.update((base / name).as_posix() for name in files)
if found != {"server-build-catalog.json"} | set(pinned):
    missing = sorted(set(pinned) - found)
    extra = sorted(found - set(pinned) - {"server-build-catalog.json"})
    raise SystemExit(f"server catalog inventory mismatch (missing {missing}, unexpected {extra})")
for relative, digest in pinned.items():
    path = root / relative
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size not in SAVE_SIZES:
        raise SystemExit(f"arrival save {relative} is unsafe or has an invalid size")
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise SystemExit(f"arrival save {relative} does not match its pinned digest")
PY
}

# Directories 0755 and files 0444: the server container runs as uid 10001 and
# mounts /srv/hoenn read-only, so world-readable, nobody-writable is enough.
seal() {
  find "$1" -type f -exec chmod 0444 {} +
  find "$1" -type d -exec chmod 0555 {} +
}

mkdir -p "$TARGET_ROOT"
if [ -d "$TARGET" ]; then
  verify "$TARGET" || exit 1
  if [ -d "$STAGING" ]; then
    verify "$STAGING" || exit 1
    diff -qr -- "$STAGING" "$TARGET" >/dev/null || { echo "error: staged server catalog differs from the promoted copy" >&2; exit 1; }
    chmod -R u+w -- "$STAGING"
    rm -rf -- "$STAGING"
  fi
  echo "server catalog already promoted: $CATALOG_SHA256"
  exit 0
fi
[ -d "$STAGING" ] || { echo "error: server catalog staging directory missing" >&2; exit 1; }
verify "$STAGING" || exit 1
mv -- "$STAGING" "$TARGET"
seal "$TARGET"
echo "promoted server catalog $CATALOG_SHA256"
