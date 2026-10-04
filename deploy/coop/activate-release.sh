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
# idempotent and a partially completed activation can simply be repeated.
#
# Usage: activate-release.sh <release-id>
set -euo pipefail

HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
RELEASE_ID="${1:?Usage: activate-release.sh <release-id>}"
[ "$#" -eq 1 ] || { echo "error: unexpected arguments" >&2; exit 2; }
PYTHON_BIN="${PYTHON_BIN:-python3}"
case "$RELEASE_ID" in
  *[!0-9a-f]* | "") echo "error: release id must be lowercase hex" >&2; exit 2 ;;
esac
if [ "${#RELEASE_ID}" -lt 7 ] || [ "${#RELEASE_ID}" -gt 64 ]; then
  echo "error: release id has unexpected length" >&2
  exit 2
fi

real_dir() { [ -d "$1" ] && [ ! -L "$1" ]; }

read_marker() {
  local marker="$1" value
  if [ -L "$marker" ]; then return 1; fi
  if [ ! -e "$marker" ]; then printf ''; return 0; fi
  [ -f "$marker" ] && [ "$(wc -c < "$marker")" -le 129 ] || return 1
  value="$(cat "$marker")"
  [[ "$value" =~ ^[A-Za-z0-9._-]{1,128}$ ]] || return 1
  printf '%s' "$value"
}

write_marker() {
  local marker="$1" value="$2"
  printf '%s\n' "$value" > "$marker.tmp"
  chmod 0644 "$marker.tmp"
  mv -Tf -- "$marker.tmp" "$marker"
}

real_dir "$HOENN_ROOT/game/$RELEASE_ID" || { echo "error: game release $RELEASE_ID is not promoted" >&2; exit 1; }
real_dir "$HOENN_ROOT/releases/$RELEASE_ID" || { echo "error: runtime release $RELEASE_ID is not promoted" >&2; exit 1; }
[ -f "$HOENN_ROOT/releases/$RELEASE_ID/release-envelope.json" ] || { echo "error: runtime release has no envelope" >&2; exit 1; }
[ -f "$HOENN_ROOT/game/$RELEASE_ID/release-envelope.json" ] || { echo "error: game release has no envelope" >&2; exit 1; }

game_current="$(read_marker "$HOENN_ROOT/game/current")" || { echo "error: game/current is malformed" >&2; exit 1; }
if [ "$game_current" != "$RELEASE_ID" ]; then
  write_marker "$HOENN_ROOT/game/current" "$RELEASE_ID"
  echo "activated game $RELEASE_ID"
fi

current="$(read_marker "$HOENN_ROOT/current")" || { echo "error: current marker is malformed" >&2; exit 1; }
if [ "$current" != "$RELEASE_ID" ]; then
  if [ -n "$current" ]; then
    write_marker "$HOENN_ROOT/.previous-release" "$current"
  fi
  write_marker "$HOENN_ROOT/current" "$RELEASE_ID"
  echo "activated runtime $RELEASE_ID (previous ${current:-none})"
fi

android="$HOENN_ROOT/android"
if real_dir "$android/$RELEASE_ID" && [ -f "$android/$RELEASE_ID/metadata.json" ] && [ -f "$android/$RELEASE_ID/app-release.apk" ]; then
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
)" || { echo "error: Android APK metadata check failed" >&2; exit 1; }
  if [ "$decision" = flip ]; then
    printf '%s\n' "$RELEASE_ID" > "$android/current.tmp"
    chmod 0600 "$android/current.tmp"
    if command -v setfacl >/dev/null 2>&1; then
      setfacl -m u:10001:r-- "$android/current.tmp"
    elif [ "${COOP_ACTIVATE_SKIP_ACL:-0}" != 1 ]; then
      rm -f -- "$android/current.tmp"
      echo "error: setfacl is required to expose android/current to the server" >&2
      exit 1
    fi
    mv -Tf -- "$android/current.tmp" "$android/current"
    echo "activated Android APK $RELEASE_ID"
  fi
fi
echo "release $RELEASE_ID is active"
