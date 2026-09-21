#!/usr/bin/env bash
# Classify one immutable release without inspecting the running container.
# Output is a small key/value protocol intended for the workflow status gate.
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: $0 RELEASE_ID IMAGE_BASE" >&2
  exit 2
fi

RELEASE_ID="$1"
IMAGE_BASE="$2"
HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

case "$RELEASE_ID" in
  '' | *[!0-9a-f]*) echo "error: release id must be a lowercase 40-hex commit" >&2; exit 2 ;;
esac
[ "${#RELEASE_ID}" -eq 40 ] || { echo "error: release id must be a lowercase 40-hex commit" >&2; exit 2; }
case "$IMAGE_BASE" in
  '' | *[!A-Za-z0-9_./:-]*) echo "error: image base contains unsupported characters" >&2; exit 2 ;;
esac
[ "${#IMAGE_BASE}" -le 255 ] || { echo "error: image base is too long" >&2; exit 2; }

RELEASE_DIR="$HOENN_ROOT/releases/$RELEASE_ID"
STAGING_DIR="$HOENN_ROOT/staging/$RELEASE_ID"
IMAGE_METADATA_FILE="$HOENN_ROOT/release-metadata/$RELEASE_ID.json"

present() {
  [ -e "$1" ] || [ -L "$1" ]
}

validate_directory_path() {
  local path="$1" label="$2"
  if present "$path" && { [ ! -d "$path" ] || [ -L "$path" ]; }; then
    echo "error: $label path is not a real directory" >&2
    exit 1
  fi
}

validate_directory_path "$RELEASE_DIR" release
validate_directory_path "$STAGING_DIR" staging

validate_generation() {
  local path="$1" label="$2"
  if find "$path" -type l -print -quit | grep -q .; then
    echo "error: $label generation contains a symlink" >&2
    exit 1
  fi
}

has_release=0
has_staging=0
has_metadata=0
[ -d "$RELEASE_DIR" ] && has_release=1
[ -d "$STAGING_DIR" ] && has_staging=1
if [ "$has_release" -ne 0 ]; then validate_generation "$RELEASE_DIR" release; fi
if [ "$has_staging" -ne 0 ]; then validate_generation "$STAGING_DIR" staging; fi
if present "$IMAGE_METADATA_FILE"; then
  has_metadata=1
  [ -f "$IMAGE_METADATA_FILE" ] && [ ! -L "$IMAGE_METADATA_FILE" ] || {
    echo "error: image association is not a regular file" >&2
    exit 1
  }
fi

image_ref=""
image_digest=""
if [ "$has_metadata" -eq 1 ]; then
  metadata_values="$($PYTHON_BIN - "$IMAGE_METADATA_FILE" "$RELEASE_ID" "$IMAGE_BASE" <<'PY'
import json
import pathlib
import re
import stat
import sys

path = pathlib.Path(sys.argv[1])
release_id = sys.argv[2]
image_base = sys.argv[3]
st = path.stat()
if not stat.S_ISREG(st.st_mode) or st.st_nlink != 1 or st.st_size > 16384:
    raise SystemExit("unsafe image association metadata")
try:
    data = json.loads(path.read_text(encoding="utf-8"))
except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
    raise SystemExit(f"invalid image association metadata: {exc}")
if set(data) != {"schema", "release_id", "image_ref", "image_digest"}:
    raise SystemExit("image association metadata schema mismatch")
if data.get("schema") != 1 or data.get("release_id") != release_id:
    raise SystemExit("image association metadata identity mismatch")
image_ref = data.get("image_ref")
digest = data.get("image_digest")
if not isinstance(image_ref, str) or not isinstance(digest, str):
    raise SystemExit("incomplete image association metadata")
if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
    raise SystemExit("invalid image association digest")
if len(image_ref) > 512 or not re.fullmatch(r"[A-Za-z0-9_./:@-]+", image_ref):
    raise SystemExit("invalid image association reference")
if image_ref != f"{image_base}@{digest}":
    raise SystemExit("image association does not match the expected repository")
print(image_ref)
print(digest)
PY
)"
  image_ref="$(printf '%s\n' "$metadata_values" | sed -n '1p')"
  image_digest="$(printf '%s\n' "$metadata_values" | sed -n '2p')"
fi

# A durable association is meaningful only while its corresponding staged or
# released generation exists. This prevents a stale metadata file from making
# a fresh workflow reuse an image with no bytes available to promote.
if [ "$has_metadata" -eq 0 ]; then
  if [ "$has_release" -ne 0 ] || [ "$has_staging" -ne 0 ]; then
    echo "error: release or staging exists without an image association" >&2
    exit 1
  fi
  printf 'state=ABSENT\nimage_ref=\nimage_digest=\n'
  exit 0
fi

if [ "$has_release" -eq 0 ] && [ "$has_staging" -eq 0 ]; then
  echo "error: image association exists without release or staging" >&2
  exit 1
fi

if [ "$has_release" -ne 0 ] && [ "$has_staging" -ne 0 ]; then
  if ! diff -qr -- "$RELEASE_DIR" "$STAGING_DIR" >/dev/null; then
    echo "error: release and staging generations conflict" >&2
    exit 1
  fi
  state=RELEASED
elif [ "$has_release" -ne 0 ]; then
  state=RELEASED
else
  state=PENDING
fi

printf 'state=%s\nimage_ref=%s\nimage_digest=%s\n' "$state" "$image_ref" "$image_digest"
