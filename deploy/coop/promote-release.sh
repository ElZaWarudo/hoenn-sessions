#!/usr/bin/env bash
# Atomically promote a staged client release into the versioned store.
#
# Layout (HOENN_ROOT defaults to /srv/hoenn):
#   $HOENN_ROOT/staging/<release-id>/   uploaded here by CI, then moved
#   $HOENN_ROOT/releases/<release-id>/  immutable release directories
#   $HOENN_ROOT/current -> releases/<release-id>  (relative symlink)
#   $HOENN_ROOT/.previous-release       release id active before promotion
#
# Each release directory must contain:
#   game.gba  coop-sidecar.exe  bridge_manifest.json  release.json  SHA256SUMS
#
# Promotion verifies file presence, SHA-256 sums, that the ROM hash matches
# the canonical rom_sha256 inside bridge_manifest.json, and that release.json
# names this release, before the `current` symlink is switched. On failure the
# previous release stays live.
#
# Re-running for an already-promoted id is safe: when the released copy
# verifies (and any re-uploaded staging copy is byte-identical), the script
# ensures `current` points at it and exits 0, so a failed server rollout can
# be retried without re-uploading. A re-uploaded staging copy that differs
# from the released copy is rejected loudly instead of being silently kept.
#
# Usage: promote-release.sh <release-id>
set -euo pipefail

HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
RELEASE_ID="${1:?Usage: promote-release.sh <release-id>}"

case "$RELEASE_ID" in
  *[^0-9a-f]* | "" )
    echo "error: release id must be a lowercase hex commit SHA" >&2
    exit 2
    ;;
esac
if [ "${#RELEASE_ID}" -lt 7 ] || [ "${#RELEASE_ID}" -gt 64 ]; then
  echo "error: release id has unexpected length: $RELEASE_ID" >&2
  exit 2
fi

STAGING_DIR="$HOENN_ROOT/staging/$RELEASE_ID"
RELEASE_DIR="$HOENN_ROOT/releases/$RELEASE_ID"
CURRENT_LINK="$HOENN_ROOT/current"
REQUIRED_FILES="game.gba coop-sidecar.exe bridge_manifest.json release.json SHA256SUMS"

# Verify one release directory in place. Returns nonzero (no exit) so the
# caller can decide between fresh-promote and already-promoted paths.
verify_release_dir() {
  local dir="$1" file rom_sha manifest_sha release_commit
  for file in $REQUIRED_FILES; do
    if [ ! -s "$dir/$file" ]; then
      echo "error: required file missing or empty: $dir/$file" >&2
      return 1
    fi
  done

  # Transport hashes.
  ( cd -- "$dir" && sha256sum -c SHA256SUMS ) || return 1

  # The ROM must match the canonical hash in the generated manifest.
  rom_sha="$(sha256sum "$dir/game.gba" | awk '{print $1}')"
  manifest_sha="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["game_build"]["rom_sha256"])' "$dir/bridge_manifest.json")" || return 1
  if [ "$rom_sha" != "$manifest_sha" ]; then
    echo "error: ROM sha256 $rom_sha != manifest rom_sha256 $manifest_sha" >&2
    return 1
  fi

  # release.json must name this release: exact match, or a >=7 hex prefix
  # relationship anchored at a full 40-char SHA. Substring containment
  # (e.g. id `deadbee` inside commit `1111deadbee2222`) is rejected.
  release_commit="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["commit"])' "$dir/release.json")" || return 1
  case "$release_commit" in
    *[^0-9a-f]* | "" )
      echo "error: release.json commit is not a hex SHA: $release_commit" >&2
      return 1
      ;;
  esac
  if [ "$release_commit" = "$RELEASE_ID" ]; then
    return 0
  elif [ "${#RELEASE_ID}" -eq 40 ] && [ "${#release_commit}" -ge 7 ] && [ "${#release_commit}" -lt 40 ]; then
    case "$RELEASE_ID" in
      "$release_commit"*) return 0 ;;
    esac
  elif [ "${#release_commit}" -eq 40 ] && [ "${#RELEASE_ID}" -ge 7 ] && [ "${#RELEASE_ID}" -lt 40 ]; then
    case "$release_commit" in
      "$RELEASE_ID"*) return 0 ;;
    esac
  fi
  echo "error: release.json commit $release_commit does not name release $RELEASE_ID" >&2
  return 1
}

flip_current() {
  # Record the previously active release for conservative manual rollback.
  if [ -L "$CURRENT_LINK" ]; then
    readlink "$CURRENT_LINK" | sed 's#.*/##' > "$HOENN_ROOT/.previous-release"
  elif [ -e "$CURRENT_LINK" ]; then
    echo "error: $CURRENT_LINK exists and is not a symlink; refusing to touch it" >&2
    return 1
  fi
  # Atomic switch: build the new symlink aside, then rename over `current`.
  ln -sfn -- "releases/$RELEASE_ID" "$CURRENT_LINK.tmp"
  mv -Tf -- "$CURRENT_LINK.tmp" "$CURRENT_LINK"
  echo "promoted $RELEASE_ID -> $(readlink "$CURRENT_LINK")"
}

mkdir -p "$HOENN_ROOT/releases"

if [ -d "$RELEASE_DIR" ]; then
  verify_release_dir "$RELEASE_DIR" || exit 1
  if [ -d "$STAGING_DIR" ]; then
    verify_release_dir "$STAGING_DIR" || exit 1
    if ! diff -qr -- "$STAGING_DIR" "$RELEASE_DIR" >/dev/null; then
      echo "error: staging copy of $RELEASE_ID differs from the released copy; refusing" >&2
      exit 1
    fi
    rm -rf -- "$STAGING_DIR"
  fi
  if [ "$(readlink "$CURRENT_LINK" 2>/dev/null)" = "releases/$RELEASE_ID" ]; then
    echo "already promoted: $RELEASE_ID"
    exit 0
  fi
  flip_current
  exit 0
fi

if [ ! -d "$STAGING_DIR" ]; then
  echo "error: staging directory missing: $STAGING_DIR" >&2
  exit 1
fi
verify_release_dir "$STAGING_DIR" || exit 1

# Move staging into the immutable store (same filesystem: atomic rename),
# then make the copy read-only by convention (ownership stays the enforcer).
mv -- "$STAGING_DIR" "$RELEASE_DIR"
chmod -R a+rX -- "$RELEASE_DIR"
chmod a-w -- "$RELEASE_DIR"/game.gba "$RELEASE_DIR"/coop-sidecar.exe \
  "$RELEASE_DIR"/bridge_manifest.json "$RELEASE_DIR"/release.json \
  "$RELEASE_DIR"/SHA256SUMS
chmod a-w -- "$RELEASE_DIR"

flip_current
