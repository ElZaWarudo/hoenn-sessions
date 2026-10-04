#!/usr/bin/env bash
# Activate an already promoted release after its server deploy is healthy.
#
# Promotion (promote-game.sh / promote-release.sh with --no-flip) only places
# immutable bytes; nothing is served to players until this script flips the
# markers, each with an atomic same-directory rename:
#   game/current       -> <release-id>   (requires game/<release-id>/)
#   current            -> <release-id>   (requires releases/<release-id>/;
#                                         the replaced id goes to .previous-release)
#   android/current    -> <release-id>   only when android/<release-id>/ holds an
#                                         APK whose metadata matches and whose
#                                         version_code is newer than the current one
# A marker that already names the release is left untouched, so reruns are
# idempotent and a partially completed activation can simply be repeated. A
# legacy `current -> releases/<id>` symlink (pre-marker layout) is accepted and
# replaced atomically by the regular marker, as promote-release.sh used to do.
#
# Preflight, before any marker changes: both generations are promoted, every
# marker and its .tmp path is absent or of the expected type, the Android
# decision is computed, and the deployed configuration matches the release:
# <deploy-dir>/.env (or --env-file) must carry exactly one COOP_IMAGE and one
# COOP_PHASE2_RELEASE_CATALOG_SHA256 equal to release-metadata/<id>.json
# (image_ref and, for schema 2, server_catalog_sha256). On mismatch nothing is
# flipped: the healthy server is not the release being activated.
#
# Usage: activate-release.sh <release-id> (--deploy-dir DIR | --env-file FILE)
set -euo pipefail

HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
RELEASE_ID="${1:?Usage: activate-release.sh <release-id> (--deploy-dir DIR | --env-file FILE)}"
shift
ENV_FILE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --deploy-dir) ENV_FILE="${2:?--deploy-dir needs a value}/.env"; shift 2 ;;
    --env-file) ENV_FILE="${2:?--env-file needs a value}"; shift 2 ;;
    *) echo "error: unexpected argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$ENV_FILE" ] || { echo "error: --deploy-dir or --env-file is required" >&2; exit 2; }
PYTHON_BIN="${PYTHON_BIN:-python3}"
case "$RELEASE_ID" in
  *[!0-9a-f]* | "") echo "error: release id must be lowercase hex" >&2; exit 2 ;;
esac
if [ "${#RELEASE_ID}" -lt 7 ] || [ "${#RELEASE_ID}" -gt 64 ]; then
  echo "error: release id has unexpected length" >&2
  exit 2
fi

real_dir() { [ -d "$1" ] && [ ! -L "$1" ]; }

valid_release_id() {
  case "$1" in *[!0-9a-f]* | "") return 1 ;; esac
  [ "${#1}" -ge 7 ] && [ "${#1}" -le 64 ]
}

read_marker() {
  local marker="$1" value
  if [ -L "$marker" ]; then return 1; fi
  if [ ! -e "$marker" ]; then printf ''; return 0; fi
  [ -f "$marker" ] && [ "$(wc -c < "$marker")" -le 129 ] || return 1
  value="$(cat "$marker")"
  [[ "$value" =~ ^[A-Za-z0-9._-]{1,128}$ ]] || return 1
  printf '%s' "$value"
}

# `current` may still be the legacy relative symlink releases/<id>.
read_current_marker() {
  local marker="$1" target
  if [ -L "$marker" ]; then
    target="$(readlink "$marker")"
    case "$target" in
      releases/*) target="${target#releases/}" ;;
      */releases/*) target="${target##*/releases/}" ;;
      *) return 1 ;;
    esac
    valid_release_id "$target" || return 1
    printf '%s' "$target"
    return 0
  fi
  read_marker "$marker"
}

# A marker's temporary sibling must be absent or a stale regular file;
# anything else would make the atomic rename fail halfway through activation.
check_tmp() {
  local tmp="$1.tmp"
  if [ -L "$tmp" ] || { [ -e "$tmp" ] && [ ! -f "$tmp" ]; }; then
    echo "error: $tmp is in the way (not a regular file); nothing was activated" >&2
    exit 1
  fi
}

write_marker() {
  local marker="$1" value="$2"
  rm -f -- "$marker.tmp"
  printf '%s\n' "$value" > "$marker.tmp"
  chmod 0644 "$marker.tmp"
  mv -Tf -- "$marker.tmp" "$marker"
}

real_dir "$HOENN_ROOT/game/$RELEASE_ID" || { echo "error: game release $RELEASE_ID is not promoted" >&2; exit 1; }
real_dir "$HOENN_ROOT/releases/$RELEASE_ID" || { echo "error: runtime release $RELEASE_ID is not promoted" >&2; exit 1; }
[ -f "$HOENN_ROOT/releases/$RELEASE_ID/release-envelope.json" ] || { echo "error: runtime release has no envelope" >&2; exit 1; }
[ -f "$HOENN_ROOT/game/$RELEASE_ID/release-envelope.json" ] || { echo "error: game release has no envelope" >&2; exit 1; }

# --- Preflight: deployed configuration must be this release. -----------------
METADATA="$HOENN_ROOT/release-metadata/$RELEASE_ID.json"
[ -f "$METADATA" ] && [ ! -L "$METADATA" ] || { echo "error: release $RELEASE_ID has no image association" >&2; exit 1; }
[ -f "$ENV_FILE" ] && [ ! -L "$ENV_FILE" ] || { echo "error: deployed env file not found: $ENV_FILE" >&2; exit 1; }
"$PYTHON_BIN" - "$METADATA" "$RELEASE_ID" "$ENV_FILE" <<'PY' || { echo "error: deployed configuration does not match release $RELEASE_ID; nothing was activated" >&2; exit 1; }
import json, pathlib, re, sys
metadata = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
release_id = sys.argv[2]
if metadata.get("release_id") != release_id or metadata.get("schema") not in (1, 2):
    raise SystemExit("image association identity or schema mismatch")
image = metadata.get("image_ref")
catalog = metadata.get("server_catalog_sha256") if metadata["schema"] == 2 else None
if not isinstance(image, str) or not re.fullmatch(r"[A-Za-z0-9_./:@-]+@sha256:[0-9a-f]{64}", image):
    raise SystemExit("image association has no digest-pinned image")
if metadata["schema"] == 2 and not (isinstance(catalog, str) and re.fullmatch(r"[0-9a-f]{64}", catalog)):
    raise SystemExit("image association has no server catalog digest")
values = {"COOP_IMAGE": [], "COOP_PHASE2_RELEASE_CATALOG_SHA256": []}
for line in pathlib.Path(sys.argv[3]).read_text(encoding="utf-8").splitlines():
    key, sep, value = line.partition("=")
    if sep and key in values:
        values[key].append(value)
def single(key):
    found = values[key]
    if len(found) != 1:
        raise SystemExit(f"{key} must appear exactly once in the deployed env file (found {len(found)})")
    return found[0]
deployed_image = single("COOP_IMAGE")
if deployed_image != image:
    raise SystemExit(f"deployed COOP_IMAGE {deployed_image} is not the release image {image}")
deployed_catalog = single("COOP_PHASE2_RELEASE_CATALOG_SHA256")
if catalog is not None and deployed_catalog != catalog:
    raise SystemExit(f"deployed COOP_PHASE2_RELEASE_CATALOG_SHA256 {deployed_catalog} is not the release catalog {catalog}")
if catalog is None:
    print("warning: schema-1 association records no server catalog; only COOP_IMAGE was compared", file=sys.stderr)
PY

# --- Preflight: every marker is readable before the first one changes. -------
game_current="$(read_marker "$HOENN_ROOT/game/current")" || { echo "error: game/current is malformed; nothing was activated" >&2; exit 1; }
current="$(read_current_marker "$HOENN_ROOT/current")" || { echo "error: current marker is malformed; nothing was activated" >&2; exit 1; }
legacy_current=0
[ ! -L "$HOENN_ROOT/current" ] || legacy_current=1
read_marker "$HOENN_ROOT/.previous-release" >/dev/null || { echo "error: .previous-release is malformed; nothing was activated" >&2; exit 1; }
check_tmp "$HOENN_ROOT/game/current"
check_tmp "$HOENN_ROOT/current"
check_tmp "$HOENN_ROOT/.previous-release"

android="$HOENN_ROOT/android"
decision=none
if real_dir "$android/$RELEASE_ID" && [ -f "$android/$RELEASE_ID/metadata.json" ] && [ -f "$android/$RELEASE_ID/app-release.apk" ]; then
  if [ -L "$android/current" ] || { [ -e "$android/current" ] && [ ! -f "$android/current" ]; }; then
    echo "error: android/current is not a regular file; nothing was activated" >&2
    exit 1
  fi
  check_tmp "$android/current"
  decision="$("$PYTHON_BIN" - "$android" "$RELEASE_ID" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
release_id = sys.argv[2]
candidate = root / release_id
proposed = json.loads((candidate / "metadata.json").read_text())
apk = candidate / "app-release.apk"
digest = hashlib.sha256()
with apk.open("rb") as stream:
    for block in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(block)
if proposed.get("release_id") != release_id or apk.stat().st_size != proposed.get("size") or digest.hexdigest() != proposed.get("sha256"):
    raise SystemExit("APK metadata does not match its release")
marker = root / "current"
current_id = marker.read_text().strip() if marker.is_file() else ""
if current_id == release_id:
    print("current")
    raise SystemExit(0)
current_metadata = root / current_id / "metadata.json" if current_id else None
current_version = json.loads(current_metadata.read_text())["version_code"] if current_metadata and current_metadata.is_file() else 0
print("flip" if proposed["version_code"] > current_version else "older")
PY
)" || { echo "error: Android APK metadata check failed; nothing was activated" >&2; exit 1; }
  if [ "$decision" = flip ] && ! command -v setfacl >/dev/null 2>&1 && [ "${COOP_ACTIVATE_SKIP_ACL:-0}" != 1 ]; then
    echo "error: setfacl is required to expose android/current to the server; nothing was activated" >&2
    exit 1
  fi
fi

# --- Flip. --------------------------------------------------------------------
if [ "$game_current" != "$RELEASE_ID" ]; then
  write_marker "$HOENN_ROOT/game/current" "$RELEASE_ID"
  echo "activated game $RELEASE_ID"
fi

if [ "$current" != "$RELEASE_ID" ]; then
  if [ -n "$current" ]; then
    write_marker "$HOENN_ROOT/.previous-release" "$current"
  fi
  write_marker "$HOENN_ROOT/current" "$RELEASE_ID"
  echo "activated runtime $RELEASE_ID (previous ${current:-none})"
elif [ "$legacy_current" -eq 1 ]; then
  write_marker "$HOENN_ROOT/current" "$RELEASE_ID"
  echo "migrated legacy current symlink to a regular marker for $RELEASE_ID"
fi

if [ "$decision" = flip ]; then
  rm -f -- "$android/current.tmp"
  printf '%s\n' "$RELEASE_ID" > "$android/current.tmp"
  chmod 0600 "$android/current.tmp"
  if command -v setfacl >/dev/null 2>&1; then
    setfacl -m u:10001:r-- "$android/current.tmp"
  fi
  mv -Tf -- "$android/current.tmp" "$android/current"
  echo "activated Android APK $RELEASE_ID"
fi
echo "release $RELEASE_ID is active"
