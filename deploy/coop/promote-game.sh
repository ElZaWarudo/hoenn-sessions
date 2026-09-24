#!/usr/bin/env bash
# Promote a signed, immutable two-artifact game release independently of Windows.
set -euo pipefail

HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
RELEASE_ID="${1:?Usage: promote-game.sh RELEASE_ID}"
COOP_RELEASE_TOOL="${COOP_RELEASE_TOOL:-/tmp/coop-release-tool}"
: "${COOP_RELEASE_KEY_ID:?missing release key id}"
: "${COOP_RELEASE_PUBLIC_KEY_HEX:?missing release public key}"

[[ "$RELEASE_ID" =~ ^[a-zA-Z0-9._-]{1,128}$ && "$RELEASE_ID" != . && "$RELEASE_ID" != .. ]] || exit 2
root="$HOENN_ROOT/game"
staging="$HOENN_ROOT/game-staging/$RELEASE_ID"
release="$root/$RELEASE_ID"
marker="$root/current"
mkdir -p "$root"

verify() {
  local dir="$1" freshness="${2:-fresh}"
  [ -d "$dir" ] && [ ! -L "$dir" ] || return 1
  local files
  files="$(find "$dir" -type f -printf '%P\n' | LC_ALL=C sort)"
  [ "$files" = $'bridge_manifest.json\ngame.gba\nrelease-envelope.json' ] || return 1
  [ -z "$(find "$dir" -mindepth 1 \( -type l -o -type d \) -print -quit)" ] || return 1
  if [ "$freshness" = old ]; then
    "$COOP_RELEASE_TOOL" verify-game --envelope "$dir/release-envelope.json" \
      --key-id "$COOP_RELEASE_KEY_ID" --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX" --allow-expired >/dev/null
  else
    "$COOP_RELEASE_TOOL" verify-game --envelope "$dir/release-envelope.json" \
      --key-id "$COOP_RELEASE_KEY_ID" --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX" >/dev/null
  fi
  python3 - "$dir" "$RELEASE_ID" <<'PY'
import base64, hashlib, json, pathlib, stat, sys
root = pathlib.Path(sys.argv[1])
envelope_path = root/'release-envelope.json'
envelope_info = envelope_path.stat()
if not stat.S_ISREG(envelope_info.st_mode) or envelope_info.st_nlink != 1 or envelope_info.st_size > 65536:
    raise SystemExit('unsafe game envelope')
descriptor = json.loads(base64.b64decode(json.loads(envelope_path.read_bytes())['payload'], validate=True))
if descriptor['release_id'] != sys.argv[2] or descriptor['platform'] != 'game':
    raise SystemExit('wrong signed game identity')
for artifact, name, limit in zip(descriptor['artifacts'], ('game.gba','bridge_manifest.json'), (64*1024*1024,1024*1024)):
    path = root/name
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or not 0 < info.st_size <= limit:
        raise SystemExit('unsafe game artifact')
    if info.st_size != artifact['size'] or hashlib.sha256(path.read_bytes()).hexdigest() != artifact['sha256']:
        raise SystemExit('game artifact hash mismatch')
print(descriptor['sequence'])
PY
}

if [ -e "$marker" ] || [ -L "$marker" ]; then
  [ -f "$marker" ] && [ ! -L "$marker" ] && [ "$(wc -c < "$marker")" -le 129 ] || exit 1
  current="$(cat "$marker")"
  [[ "$current" =~ ^[a-zA-Z0-9._-]{1,128}$ && "$current" != . && "$current" != .. ]] || exit 1
  current_sequence="$(verify "$root/$current" old)"
else
  current_sequence=0
fi

if [ -d "$release" ]; then
  sequence="$(verify "$release")"
  if [ -d "$staging" ]; then
    [ "$(verify "$staging")" = "$sequence" ] || exit 1
    cmp -s -- "$staging/game.gba" "$release/game.gba" || exit 1
    cmp -s -- "$staging/bridge_manifest.json" "$release/bridge_manifest.json" || exit 1
    # Reruns re-sign with a new validity window. Both envelopes are verified;
    # the immutable game identity and artifact metadata must still agree.
    python3 - "$staging/release-envelope.json" "$release/release-envelope.json" <<'PY'
import base64, json, pathlib, sys
def descriptor(path):
    envelope = json.loads(pathlib.Path(path).read_bytes())
    value = json.loads(base64.b64decode(envelope['payload'], validate=True))
    value.pop('issued_at', None)
    value.pop('expires_at', None)
    return value
if descriptor(sys.argv[1]) != descriptor(sys.argv[2]):
    raise SystemExit('game release content differs from promoted release')
PY
  fi
else
  sequence="$(verify "$staging")"
fi
if [ "$sequence" -lt "$current_sequence" ] || { [ "$sequence" -eq "$current_sequence" ] && [ "${current:-}" != "$RELEASE_ID" ]; }; then
  echo 'error: game sequence rollback rejected' >&2
  exit 1
fi
if [ -d "$staging" ] && [ ! -d "$release" ]; then
  mv -- "$staging" "$release"
  chmod -R a+rX -- "$release"
  find "$release" -type f -exec chmod a-w {} +
fi
printf '%s\n' "$RELEASE_ID" > "$marker.tmp"
chmod 0644 "$marker.tmp"
mv -Tf -- "$marker.tmp" "$marker"
echo "promoted game $RELEASE_ID (sequence $sequence)"
