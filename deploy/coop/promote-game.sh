#!/usr/bin/env bash
# Promote a signed, immutable game release independently of Windows.
#
# The inventory is derived from the signed descriptor: game.gba and
# bridge_manifest.json, plus release_catalog.json and worlds/<N>/{game.gba,
# bridge_manifest.json,player_transfer.json} for every signed world. The old
# two-artifact set (no worlds) still verifies so an old release can be rerun.
# For multi-world releases the region catalog is cross-checked against the
# signed per-world files and world 1 must equal the base game.
#
# Usage: promote-game.sh RELEASE_ID [--no-flip]
#   --no-flip  verify and move into game/<id> but leave game/current alone;
#              activate-release.sh flips it after a healthy server deploy.
set -euo pipefail

HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
RELEASE_ID="${1:?Usage: promote-game.sh RELEASE_ID [--no-flip]}"
shift
NO_FLIP=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-flip) NO_FLIP=1; shift ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done
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
  local dir="$1" expected_id="$2" freshness="${3:-fresh}"
  [ -d "$dir" ] && [ ! -L "$dir" ] || return 1
  [ -z "$(find "$dir" -mindepth 1 -type l -print -quit)" ] || return 1
  [ -f "$dir/release-envelope.json" ] || return 1
  if [ "$freshness" = old ]; then
    "$COOP_RELEASE_TOOL" verify-game --envelope "$dir/release-envelope.json" \
      --key-id "$COOP_RELEASE_KEY_ID" --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX" --allow-expired >/dev/null
  else
    "$COOP_RELEASE_TOOL" verify-game --envelope "$dir/release-envelope.json" \
      --key-id "$COOP_RELEASE_KEY_ID" --public-key-hex "$COOP_RELEASE_PUBLIC_KEY_HEX" >/dev/null
  fi
  python3 - "$dir" "$expected_id" <<'PY'
import base64, hashlib, json, os, pathlib, re, stat, sys
root = pathlib.Path(sys.argv[1])
envelope_path = root/'release-envelope.json'
envelope_info = envelope_path.stat()
if not stat.S_ISREG(envelope_info.st_mode) or envelope_info.st_nlink != 1 or envelope_info.st_size > 65536:
    raise SystemExit('unsafe game envelope')
descriptor = json.loads(base64.b64decode(json.loads(envelope_path.read_bytes())['payload'], validate=True))
if descriptor['release_id'] != sys.argv[2] or descriptor['platform'] != 'game':
    raise SystemExit('wrong signed game identity')
LIMITS = {'game.gba': 64*1024*1024, 'bridge_manifest.json': 1024*1024,
          'player_transfer.json': 1024*1024, 'release_catalog.json': 256*1024}
KINDS = (('rom', 'game.gba'), ('compatibility', 'bridge_manifest.json'),
         ('player-transfer', 'player_transfer.json'))
pattern = re.compile(r'world-([1-9][0-9]{0,4})-(rom|compatibility|player-transfer)\Z')
artifacts = descriptor['artifacts']
worlds = sorted({int(m.group(1)) for a in artifacts for m in [pattern.match(a['id'])] if m})
expected = [('rom', 'game.gba'), ('compatibility-manifest', 'bridge_manifest.json')]
if worlds:
    expected.append(('region-catalog', 'release_catalog.json'))
    for world in worlds:
        expected += [(f'world-{world}-{kind}', f'worlds/{world}/{name}') for kind, name in KINDS]
if [a['id'] for a in artifacts] != [identity for identity, _ in expected]:
    raise SystemExit('game descriptor artifacts are not the canonical set')
found_files, found_dirs = set(), set()
for current, dirs, files in os.walk(root):
    base = pathlib.Path(current).relative_to(root)
    found_dirs.update((base/name).as_posix() for name in dirs)
    found_files.update((base/name).as_posix() for name in files)
expected_dirs = ({'worlds'} | {f'worlds/{w}' for w in worlds}) if worlds else set()
if found_files != {'release-envelope.json'} | {path for _, path in expected} or found_dirs != expected_dirs:
    raise SystemExit('game release inventory is not exactly the signed artifacts plus envelope')
signed = {}
for artifact, (identity, name) in zip(artifacts, expected):
    path = root/name
    info = path.stat()
    limit = LIMITS[pathlib.PurePosixPath(name).name]
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or not 0 < info.st_size <= limit:
        raise SystemExit('unsafe game artifact')
    if info.st_size != artifact['size'] or hashlib.sha256(path.read_bytes()).hexdigest() != artifact['sha256']:
        raise SystemExit('game artifact hash mismatch')
    signed[identity] = (name, artifact['sha256'])
if worlds:
    try:
        catalog = json.loads((root/'release_catalog.json').read_bytes())
        entries = {entry['world_id']: entry for entry in catalog['worlds']}
        if catalog.get('schema_version') != 1 or len(entries) != len(catalog['worlds']):
            raise ValueError
    except (KeyError, TypeError, ValueError):
        raise SystemExit('region catalog is malformed')
    if sorted(entries) != worlds:
        raise SystemExit('region catalog worlds differ from the signed world artifacts')
    for world, entry in entries.items():
        for kind, prefix in (('rom', 'rom'), ('compatibility', 'bridge'), ('player-transfer', 'player_transfer')):
            name, digest = signed[f'world-{world}-{kind}']
            if entry.get(f'{prefix}_path') != name or entry.get(f'{prefix}_sha256') != digest:
                raise SystemExit(f'region catalog does not bind world {world} {kind} to its signed file')
print(descriptor['sequence'])
PY
}

if [ -e "$marker" ] || [ -L "$marker" ]; then
  [ -f "$marker" ] && [ ! -L "$marker" ] && [ "$(wc -c < "$marker")" -le 129 ] || exit 1
  current="$(cat "$marker")"
  [[ "$current" =~ ^[a-zA-Z0-9._-]{1,128}$ && "$current" != . && "$current" != .. ]] || exit 1
  current_sequence="$(verify "$root/$current" "$current" old)"
else
  current_sequence=0
fi

if [ -d "$release" ]; then
  sequence="$(verify "$release" "$RELEASE_ID")"
  if [ -d "$staging" ]; then
    [ "$(verify "$staging" "$RELEASE_ID")" = "$sequence" ] || exit 1
    diff -qr -x release-envelope.json -- "$staging" "$release" >/dev/null || exit 1
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
  sequence="$(verify "$staging" "$RELEASE_ID")"
fi
if [ "$sequence" -lt "$current_sequence" ] || { [ "$sequence" -eq "$current_sequence" ] && [ "${current:-}" != "$RELEASE_ID" ]; }; then
  echo 'error: game sequence rollback rejected' >&2
  exit 1
fi
if [ -d "$staging" ] && [ ! -d "$release" ]; then
  mv -- "$staging" "$release"
  chmod -R a+rX -- "$release"
  find "$release" -type f -exec chmod a-w {} +
elif [ -d "$staging" ]; then
  rm -rf -- "$staging"
fi
if [ "$NO_FLIP" -eq 1 ]; then
  echo "promoted game $RELEASE_ID (sequence $sequence) without activation"
  exit 0
fi
printf '%s\n' "$RELEASE_ID" > "$marker.tmp"
chmod 0644 "$marker.tmp"
mv -Tf -- "$marker.tmp" "$marker"
echo "promoted game $RELEASE_ID (sequence $sequence)"
