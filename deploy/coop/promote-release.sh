#!/usr/bin/env bash
# Verify and atomically promote one private-pilot Windows runtime generation.
#
# The release tool verifies the signed schema-one envelope. This script then
# verifies the closed eleven-file inventory and every transport hash before it
# moves staging into the immutable release store. `current` is a bounded
# regular marker file; a legacy relative symlink is accepted once and replaced
# atomically during the next promotion.
#
# Layout (HOENN_ROOT defaults to /srv/hoenn):
#   staging/<release-id>/       private runner upload, never served
#   releases/<release-id>/      immutable eleven-file runtime + envelope
#   current                    release id marker, atomically replaced
#   .previous-release          id active before the last promotion
#
# Usage: promote-release.sh <release-id> [--image-ref FULL_REF]
#                            [--image-digest sha256:<64 lowercase hex>]
set -euo pipefail

HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
RELEASE_ID="${1:?Usage: promote-release.sh <release-id>}"
shift
IMAGE_REF=""
IMAGE_DIGEST=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --image-ref|--image)
      IMAGE_REF="${2:?$1 needs a value}"; shift 2 ;;
    --image-digest|--digest)
      IMAGE_DIGEST="${2:?$1 needs a value}"; shift 2 ;;
    -h|--help)
      sed -n '1,17p' -- "$0"; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done
RELEASE_KEY_ID="${COOP_RELEASE_KEY_ID:-${HOENN_RELEASE_TRUST_KEY_ID:-}}"
RELEASE_PUBLIC_KEY_HEX="${COOP_RELEASE_PUBLIC_KEY_HEX:-${HOENN_RELEASE_TRUST_PUBLIC_KEY_HEX:-}}"
COOP_RELEASE_TOOL="${COOP_RELEASE_TOOL:-coop-release-tool}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
if ! "$PYTHON_BIN" -c 'pass' >/dev/null 2>&1; then
  if python -c 'pass' >/dev/null 2>&1; then
    PYTHON_BIN=python
  elif [ -x /c/Python313/python.exe ]; then
    PYTHON_BIN=/c/Python313/python.exe
  else
    echo "error: Python 3 is required for envelope hash checks" >&2
    exit 2
  fi
fi

case "$RELEASE_ID" in
  *[^0-9a-f]* | "")
    echo "error: release id must be a lowercase hex identifier" >&2
    exit 2
    ;;
esac
if [ "${#RELEASE_ID}" -lt 7 ] || [ "${#RELEASE_ID}" -gt 64 ]; then
  echo "error: release id has unexpected length" >&2
  exit 2
fi
if [ -z "$RELEASE_KEY_ID" ] || [ "${#RELEASE_KEY_ID}" -gt 64 ]; then
  echo "error: COOP_RELEASE_KEY_ID is required" >&2
  exit 2
fi
case "$RELEASE_PUBLIC_KEY_HEX" in
  "" | *[!0-9a-fA-F]* )
    echo "error: COOP_RELEASE_PUBLIC_KEY_HEX must be 32-byte hex" >&2
    exit 2
    ;;
esac
if [ "${#RELEASE_PUBLIC_KEY_HEX}" -ne 64 ]; then
  echo "error: COOP_RELEASE_PUBLIC_KEY_HEX must be 32-byte hex" >&2
  exit 2
fi

validate_image_inputs() {
  if [ -z "$IMAGE_REF" ] && [ -z "$IMAGE_DIGEST" ]; then
    return 0
  fi
  if [ -z "$IMAGE_REF" ] || [ -z "$IMAGE_DIGEST" ]; then
    echo "error: --image-ref and --image-digest must be supplied together" >&2
    return 1
  fi
  case "$IMAGE_DIGEST" in
    sha256:*) ;;
    *) echo "error: image digest must be sha256:<64 lowercase hex>" >&2; return 1 ;;
  esac
  digest="${IMAGE_DIGEST#sha256:}"
  case "$digest" in *[!0-9a-f]* | "") echo "error: image digest must be lowercase hex" >&2; return 1 ;; esac
  [ "${#digest}" -eq 64 ] || { echo "error: image digest must be 64 hex chars" >&2; return 1; }
  case "$IMAGE_REF" in *[!A-Za-z0-9_./:@-]* | "") echo "error: image reference contains unsupported characters" >&2; return 1 ;; esac
  case "$IMAGE_REF" in
    *@$IMAGE_DIGEST)
      image_name="${IMAGE_REF%@*}"
      [ -n "$image_name" ] || { echo "error: image reference name is required" >&2; return 1; }
      ;;
    *) echo "error: image reference must end with the supplied digest" >&2; return 1 ;;
  esac
}

validate_image_inputs

STAGING_DIR="$HOENN_ROOT/staging/$RELEASE_ID"
RELEASE_DIR="$HOENN_ROOT/releases/$RELEASE_ID"
CURRENT_MARKER="$HOENN_ROOT/current"
PREVIOUS_MARKER="$HOENN_ROOT/.previous-release"
IMAGE_METADATA_ROOT="${COOP_RELEASE_IMAGE_METADATA_ROOT:-$HOENN_ROOT/release-metadata}"
IMAGE_METADATA_FILE="$IMAGE_METADATA_ROOT/$RELEASE_ID.json"
ENVELOPE_FILE="release-envelope.json"
MAX_ARTIFACT_BYTES=536870912

case "$IMAGE_METADATA_ROOT" in
  "$RELEASE_DIR"|"$RELEASE_DIR"/*)
    echo "error: image metadata must remain outside the served release directory" >&2
    exit 2
    ;;
esac

artifact_ids=(
  desktop-app managed-mgba rom sidecar bridge-main bridge-memory
  bridge-protocol bridge-addresses compatibility-manifest trust-bundle notices
)
artifact_paths=(
  app/coop-launcher.exe runtime/mgba.exe runtime/game.gba
  runtime/coop-sidecar.exe bridge/main.lua bridge/memory.lua
  bridge/protocol.lua bridge/generated_addresses.lua bridge_manifest.json
  trust/release-trust.json THIRD_PARTY_NOTICES.txt
)

expected_inventory() {
  printf '%s\n' "$ENVELOPE_FILE"
  printf '%s\n' "${artifact_paths[@]}"
}

check_regular_file() {
  local path="$1"
  [ -f "$path" ] && [ ! -L "$path" ] && [ -s "$path" ]
}

verify_inventory() {
  local dir="$1" file actual expected actual_dirs expected_dirs
  check_regular_file "$dir/$ENVELOPE_FILE" || {
    echo "error: signed release envelope is missing or unsafe" >&2
    return 1
  }
  for file in "${artifact_paths[@]}"; do
    check_regular_file "$dir/$file" || {
      echo "error: required runtime file is missing or unsafe: $file" >&2
      return 1
    }
  done
  if find "$dir" -type l -print -quit | grep -q .; then
    echo "error: release inventory contains a symlink" >&2
    return 1
  fi
  actual="$(find "$dir" -type f -printf '%P\n' | LC_ALL=C sort)"
  expected="$(expected_inventory | LC_ALL=C sort)"
  if [ "$actual" != "$expected" ]; then
    echo "error: release inventory is not exactly the eleven fixed artifacts plus envelope" >&2
    return 1
  fi
  actual_dirs="$(find "$dir" -mindepth 1 -type d -printf '%P\n' | LC_ALL=C sort)"
  expected_dirs="$(printf '%s\n' app bridge runtime trust | LC_ALL=C sort)"
  if [ "$actual_dirs" != "$expected_dirs" ]; then
    echo "error: release inventory contains an unexpected directory" >&2
    return 1
  fi
}

verify_envelope_and_hashes() {
  local dir="$1"
  verify_inventory "$dir"
  if ! command -v "$COOP_RELEASE_TOOL" >/dev/null 2>&1 && [ ! -x "$COOP_RELEASE_TOOL" ]; then
    echo "error: signed-envelope verifier is required (set COOP_RELEASE_TOOL)" >&2
    return 1
  fi
  "$COOP_RELEASE_TOOL" verify \
    --envelope "$dir/$ENVELOPE_FILE" \
    --key-id "$RELEASE_KEY_ID" \
    --public-key-hex "$RELEASE_PUBLIC_KEY_HEX" >/dev/null
  "$PYTHON_BIN" - "$dir" "$RELEASE_ID" "$MAX_ARTIFACT_BYTES" <<'PY'
import base64
import hashlib
import json
import os
import pathlib
import stat
import sys

root = pathlib.Path(sys.argv[1])
release_id = sys.argv[2]
max_bytes = int(sys.argv[3])
expected = [
    ("desktop-app", "app/coop-launcher.exe"),
    ("managed-mgba", "runtime/mgba.exe"),
    ("rom", "runtime/game.gba"),
    ("sidecar", "runtime/coop-sidecar.exe"),
    ("bridge-main", "bridge/main.lua"),
    ("bridge-memory", "bridge/memory.lua"),
    ("bridge-protocol", "bridge/protocol.lua"),
    ("bridge-addresses", "bridge/generated_addresses.lua"),
    ("compatibility-manifest", "bridge_manifest.json"),
    ("trust-bundle", "trust/release-trust.json"),
    ("notices", "THIRD_PARTY_NOTICES.txt"),
]
envelope_path = root / "release-envelope.json"
envelope_stat = envelope_path.stat()
if not stat.S_ISREG(envelope_stat.st_mode) or envelope_stat.st_nlink != 1 or envelope_stat.st_size > 262144:
    raise SystemExit("unsafe or oversized release envelope")
with envelope_path.open("rb") as stream:
    envelope = json.load(stream)
payload = base64.b64decode(envelope["payload"], validate=True)
descriptor = json.loads(payload)
if descriptor.get("schema") != 1 or descriptor.get("platform") != "windows-x86_64":
    raise SystemExit("invalid release descriptor schema or platform")
if descriptor.get("release_id") != release_id:
    raise SystemExit("release descriptor id does not match promotion id")
actual = [(a.get("id"), a.get("sha256"), a.get("size")) for a in descriptor.get("artifacts", [])]
if [a[0] for a in actual] != [a[0] for a in expected]:
    raise SystemExit("release descriptor artifact identities are not exact")
for (artifact_id, destination), (_, expected_sha, expected_size) in zip(expected, actual):
    if not isinstance(expected_size, int) or expected_size <= 0 or expected_size > max_bytes:
        raise SystemExit(f"signed size exceeds bound for {artifact_id}")
    path = root / destination
    try:
        before = path.stat()
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
            raise SystemExit(f"unsafe artifact entry for {artifact_id}")
        if before.st_size != expected_size or before.st_size > max_bytes:
            raise SystemExit(f"transport size mismatch for {artifact_id}")
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        try:
            after = os.fstat(fd)
            if not stat.S_ISREG(after.st_mode) or after.st_nlink != 1 or after.st_size != expected_size:
                raise SystemExit(f"artifact changed before hashing for {artifact_id}")
            digest = hashlib.sha256()
            total = 0
            with os.fdopen(fd, "rb", closefd=True) as stream:
                while True:
                    chunk = stream.read(1024 * 1024)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > max_bytes or total > expected_size:
                        raise SystemExit(f"artifact exceeds signed bound for {artifact_id}")
                    digest.update(chunk)
        except BaseException:
            try:
                os.close(fd)
            except OSError:
                pass
            raise
    except FileNotFoundError as exc:
        raise SystemExit(f"missing artifact for {artifact_id}") from exc
    if total != expected_size or digest.hexdigest() != expected_sha:
        raise SystemExit(f"transport hash mismatch for {artifact_id}")
PY
}

validate_release_id_marker() {
  local value="$1"
  case "$value" in
    *[^0-9a-f]* | "") return 1 ;;
  esac
  [ "${#value}" -ge 7 ] && [ "${#value}" -le 64 ]
}

current_release_id() {
  if [ -L "$CURRENT_MARKER" ]; then
    local target
    target="$(readlink "$CURRENT_MARKER")"
    case "$target" in
      releases/*) target="${target#releases/}" ;;
      */releases/*) target="${target##*/releases/}" ;;
      *) return 1 ;;
    esac
    validate_release_id_marker "$target" || return 1
    printf '%s' "$target"
    return 0
  fi
  if [ -e "$CURRENT_MARKER" ]; then
    [ -f "$CURRENT_MARKER" ] || return 1
    [ ! -L "$CURRENT_MARKER" ] || return 1
    [ "$(wc -c < "$CURRENT_MARKER")" -le 65 ] || return 1
    local value
    value="$(cat "$CURRENT_MARKER")"
    validate_release_id_marker "$value" || return 1
    printf '%s\n' "$value" | cmp -s - "$CURRENT_MARKER" || return 1
    printf '%s' "$value"
  fi
}

metadata_values() {
  [ -f "$IMAGE_METADATA_FILE" ] && [ ! -L "$IMAGE_METADATA_FILE" ] || return 1
  "$PYTHON_BIN" - "$IMAGE_METADATA_FILE" "$RELEASE_ID" <<'PY'
import json
import pathlib
import stat
import sys
path = pathlib.Path(sys.argv[1])
release_id = sys.argv[2]
st = path.stat()
if not stat.S_ISREG(st.st_mode) or st.st_nlink != 1 or st.st_size > 16384:
    raise SystemExit("unsafe image association metadata")
data = json.loads(path.read_text(encoding="utf-8"))
if data.get("schema") != 1 or data.get("release_id") != release_id:
    raise SystemExit("image association metadata identity mismatch")
image_ref = data.get("image_ref")
digest = data.get("image_digest")
if not isinstance(image_ref, str) or not isinstance(digest, str):
    raise SystemExit("incomplete image association metadata")
if not (digest.startswith("sha256:") and len(digest) == 71 and all(c in "0123456789abcdef" for c in digest[7:])):
    raise SystemExit("invalid image association digest")
if image_ref != image_ref.strip() or any(c not in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_./:@-" for c in image_ref):
    raise SystemExit("invalid image association reference")
if not image_ref.endswith("@" + digest):
    raise SystemExit("image association reference/digest mismatch")
print(image_ref)
print(digest)
PY
}

ensure_image_association() {
  mkdir -p "$IMAGE_METADATA_ROOT"
  if [ -f "$IMAGE_METADATA_FILE" ] || [ -L "$IMAGE_METADATA_FILE" ]; then
    local values stored_ref stored_digest
    values="$(metadata_values)" || return 1
    stored_ref="$(printf '%s\n' "$values" | sed -n '1p')"
    stored_digest="$(printf '%s\n' "$values" | sed -n '2p')"
    if [ -n "$IMAGE_REF" ] && [ "$IMAGE_REF" != "$stored_ref" ]; then
      echo "error: immutable release image association conflict" >&2
      return 1
    fi
    if [ -n "$IMAGE_DIGEST" ] && [ "$IMAGE_DIGEST" != "$stored_digest" ]; then
      echo "error: immutable release image digest conflict" >&2
      return 1
    fi
    IMAGE_REF="$stored_ref"
    IMAGE_DIGEST="$stored_digest"
    return 0
  fi
  validate_image_inputs
  if [ -z "$IMAGE_REF" ]; then
    echo "error: a new release requires an image reference and digest" >&2
    return 1
  fi
  if [ "${COOP_RELEASE_TEST_FAIL_ASSOCIATION:-0}" = "1" ]; then
    echo "error: injected association creation failure" >&2
    return 1
  fi
  "$PYTHON_BIN" - "$IMAGE_METADATA_ROOT" "$IMAGE_METADATA_FILE" "$RELEASE_ID" "$IMAGE_REF" "$IMAGE_DIGEST" <<'PY'
import json
import os
import pathlib
import tempfile
import sys
root = pathlib.Path(sys.argv[1])
path = pathlib.Path(sys.argv[2])
root.mkdir(parents=True, exist_ok=True)
data = {"schema": 1, "release_id": sys.argv[3], "image_ref": sys.argv[4], "image_digest": sys.argv[5]}
payload = (json.dumps(data, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
fd, temp_name = tempfile.mkstemp(prefix=path.name + ".", suffix=".tmp", dir=root)
try:
    with os.fdopen(fd, "wb") as stream:
        stream.write(payload)
        stream.flush()
        os.fsync(stream.fileno())
    try:
        os.link(temp_name, path)
    except FileExistsError:
        raise SystemExit("image association appeared concurrently")
    dir_fd = os.open(root, os.O_RDONLY)
    try:
        os.fsync(dir_fd)
    finally:
        os.close(dir_fd)
finally:
    try:
        os.unlink(temp_name)
    except FileNotFoundError:
        pass
PY
  chmod 0644 "$IMAGE_METADATA_FILE"
}

flip_current() {
  local previous=""
  if previous="$(current_release_id 2>/dev/null)"; then
    if [ "$previous" != "$RELEASE_ID" ]; then
      printf '%s\n' "$previous" > "$PREVIOUS_MARKER.tmp"
      chmod 0644 "$PREVIOUS_MARKER.tmp"
      mv -Tf -- "$PREVIOUS_MARKER.tmp" "$PREVIOUS_MARKER"
    fi
  elif [ -e "$CURRENT_MARKER" ] || [ -L "$CURRENT_MARKER" ]; then
    echo "error: current marker is malformed or unsafe" >&2
    return 1
  fi
  printf '%s\n' "$RELEASE_ID" > "$CURRENT_MARKER.tmp"
  chmod 0644 "$CURRENT_MARKER.tmp"
  mv -Tf -- "$CURRENT_MARKER.tmp" "$CURRENT_MARKER"
  echo "promoted $RELEASE_ID ($IMAGE_DIGEST)"
}

mkdir -p "$HOENN_ROOT/releases"

if [ -d "$RELEASE_DIR" ]; then
  verify_envelope_and_hashes "$RELEASE_DIR" || exit 1
  if [ -d "$STAGING_DIR" ]; then
    verify_envelope_and_hashes "$STAGING_DIR" || exit 1
    if ! diff -qr -- "$STAGING_DIR" "$RELEASE_DIR" >/dev/null; then
      echo "error: staging copy differs from the released copy; refusing" >&2
      exit 1
    fi
  fi
  # Validate the durable association before deleting a duplicate upload.
  ensure_image_association || exit 1
  if [ -d "$STAGING_DIR" ]; then rm -rf -- "$STAGING_DIR"; fi
  if [ "$(current_release_id 2>/dev/null || true)" = "$RELEASE_ID" ] && [ ! -L "$CURRENT_MARKER" ]; then
    echo "already promoted: $RELEASE_ID ($IMAGE_DIGEST)"
    exit 0
  fi
  flip_current
  exit 0
fi

if [ ! -d "$STAGING_DIR" ]; then
  echo "error: staging directory missing" >&2
  exit 1
fi
verify_envelope_and_hashes "$STAGING_DIR" || exit 1
# Association publication is the first irreversible step. If it cannot be
# created, staging remains available and neither a release directory nor a
# current marker is exposed. A later matching retry may reuse this orphan.
ensure_image_association || exit 1
if [ "${COOP_RELEASE_TEST_FAIL_MOVE:-0}" = "1" ]; then
  echo "error: injected release move failure" >&2
  exit 1
fi
mv -- "$STAGING_DIR" "$RELEASE_DIR"
chmod -R a+rX -- "$RELEASE_DIR"
find "$RELEASE_DIR" -type f -exec chmod a-w {} +
flip_current
