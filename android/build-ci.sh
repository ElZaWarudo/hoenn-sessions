#!/usr/bin/env bash
# Called after the pipeline's single ROM/manifest build. Never publishes the ROM.
set -euo pipefail
cd "$(dirname "$0")/.."
variant="${1:-debug}"
case "$variant" in debug|release) ;; *) echo 'Expected debug or release' >&2; exit 1;; esac
export HOENN_ROM_PATH="$PWD/pokeemerald.gba"
export HOENN_MANIFEST_PATH="$PWD/dist/bridge_manifest.json"
test -f "$HOENN_ROM_PATH" -a -f "$HOENN_MANIFEST_PATH"
sdkmanager 'platforms;android-37.0' 'build-tools;36.0.0' 'ndk;27.2.12479018' 'cmake;3.22.1'
mkdir -p .local
if [ ! -d .local/mgba/.git ]; then
  git clone --depth 1 --branch 0.10.5 https://github.com/mgba-emu/mgba.git .local/mgba
fi
# build-rust.py also checks both this commit and checkout cleanliness.
test "$(git -C .local/mgba rev-parse HEAD)" = 26b7884bc25a5933960f3cdcd98bac1ae14d42e2
if [ "$variant" = release ]; then
  : "${ANDROID_KEYSTORE_B64:?Configure the Android release signing secret}"
  : "${ANDROID_KEYSTORE_PASSWORD:?Configure the Android release signing password}"
  export ANDROID_KEYSTORE_PATH="${RUNNER_TEMP:?}/hoenn-android.jks"
  trap 'rm -f -- "$ANDROID_KEYSTORE_PATH"' EXIT
  printf '%s' "$ANDROID_KEYSTORE_B64" | base64 --decode > "$ANDROID_KEYSTORE_PATH"
  chmod 600 "$ANDROID_KEYSTORE_PATH"
fi
bash android/gradlew -p android "assemble${variant^}" testDebugUnitTest lintRelease \
  --no-daemon --console=plain --max-workers=2 '-Dorg.gradle.jvmargs=-Xmx3g -Dfile.encoding=UTF-8'
apk="android/app/build/outputs/apk/$variant/app-$variant.apk"
python android/verify-apk.py "$apk" "$HOENN_MANIFEST_PATH"
"$ANDROID_HOME/build-tools/36.0.0/apksigner" verify --verbose "$apk"
"$ANDROID_HOME/build-tools/36.0.0/zipalign" -c -P 16 4 "$apk"
sha256sum "$apk" > "$apk.sha256"
