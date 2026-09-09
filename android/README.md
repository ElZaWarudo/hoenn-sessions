# Android native preview

**Work in progress:** see [current PR status](ESTADO-PR.md) before building.
The latest Rust/JNI session changes are not yet packaged by the build script;
the historical APK validation below does not certify this source revision.

This APK is a development client, **not a completed cooperative GBA emulator**.
It embeds the mGBA 0.10.5 C core for Android x86_64 and arm64-v8a, a local game
view with touch buttons and stereo audio, and a private battery-save file.
It does not package a ROM, BIOS, account credentials, invitation, Firebase
credentials, server private keys, or existing saves.

The Android client implements the real `/v1/auth/register`, `login`, `logout`,
`/v1/sessions/acquire`, `heartbeat`, `release` and authenticated resume-package
request contracts. HTTPS uses Android's normal CA and hostname validation;
redirects and cleartext traffic are rejected. Tokens stay in process memory.
Server errors show only status codes, never response bodies or tokens.

The public signing key is pinned as `pilot-v1`:
`f239614d143272c416c185e4d52d95a883f7ad89cadf55e1cf1dbc9dda8fe188`.
Resume verification uses RFC8785 JCS and Ed25519, checks the character,
revision and build, and fails closed. Health and login responses have no
Ed25519 proof. A revision-zero account returns HTTP 404 for its absent resume
package, so its signing identity cannot yet be tested against the live server.
No synthetic snapshot is uploaded to work around that limitation.

## Exact game artifact

`app/src/main/assets/bridge_manifest.json` was extracted using:

```powershell
git show 8fa887dc04:dist/bridge_manifest.json
```

It describes `pokecrossroads-beta-1.4-e05c8286-coop-v1` and requires
`pokeemerald.gba` SHA-256:

```text
8d599e9742e14c4418e754f9a105299d99058c584f59a7a1b5ae35eb8b336eb0
```

Use the document picker to import that exact ROM. The entire file is hashed
before the native core is opened. Other files are rejected and the temporary
import is removed. Its offline save is `files/local-character.sav`, separate
from any cloud character. A new ROM build with a different digest needs a new
matching release manifest and an operator-coordinated server update. This task
does not modify the VPS or substitute another ROM.

## Android bridge boundary and outstanding work

The existing Windows Qt executable, Lua scripting UI and Windows Job Object
launcher are **not Android-compatible**. This client compiles upstream mGBA C
source at commit `26b7884bc25a5933960f3cdcd98bac1ae14d42e2` instead. It does not
claim that the Windows executable digest authenticates the Android binary.

`NativeBridge`, `BridgeFrame` and the JNI functions port the memory boundary
from `bridge/memory.lua` and `bridge/protocol.lua`: exact ABI header checks,
144-byte records, CRC32, message direction, queue capacity/counters, bounds,
and producer publication after copying bytes while the emulated CPU is stopped.
The bridge remains inert in the preview. It is not wired to the sidecar's
session, checkpoint and realtime state machines.

Remaining before cooperative play:

- Connect an Android owner to the existing sidecar lifecycle (session-ready,
  pause/reconnect, signed resume recovery, persistent epoch fencing).
- Connect authenticated realtime ticket/WebSocket presence and remote avatars.
- Port checkpoint grants, generation-matched SAV completion, canonical save
  parsing and prepare/upload/finalize. Never upload the offline preview save
  as though that transaction had occurred.
- Qualify the Android mGBA artifact/runtime identity and savestate compatibility
  separately from the pinned Windows Qt artifact. Native state files are not
  assumed to be interchangeable.
- Obtain the exact ROM and test load, controls/audio, ROM-generated saves,
  memory bridge timing, reconnect and two-player presence on real frames.

## Build and reopen on this Windows machine

Run from the repository root using PowerShell 7:

```powershell
.\android\build.ps1 -Test
.\android\open-emulator.ps1
```

The build script uses the installed Android Studio JBR by default, or
`JAVA_HOME`; the SDK is `ANDROID_HOME` or `%LOCALAPPDATA%/Android/Sdk`.
It installs the required SDK packages and retrieves the exact clean mGBA source
checkout into ignored `.local/mgba`. Gradle Wrapper 9.5.0 verifies its
distribution checksum. AGP is pinned to 9.3.1; NDK to 27.2.12479018; CMake to
3.22.1. No ARM toolchain for rebuilding the ROM is required to compile this APK.

Output: `android/app/build/outputs/apk/debug/app-debug.apk` (debug signed).
The AVD is `Hoenn_API_36`, Android 16/API 36, Google APIs, x86_64. The reopen
script waits for boot, installs the APK with adb and launches the activity.
It uses WHPX, software graphics and explicit DNS `1.1.1.1,8.8.8.8`. It disables
the AVD's Wi-Fi and enables its virtual cellular network: Wi-Fi DNS resolved
from the shell but failed for application UIDs on this emulator. HTTPS and
normal TLS validation work over the virtual cellular network. These commands
affect this AVD only, not the Windows network or the VPS.

Equivalent build after dependencies are installed:

```powershell
$env:JAVA_HOME = 'C:/Program Files/Android/Android Studio/jbr'
.\android\gradlew.bat -p android assembleDebug testDebugUnitTest
```

## Device smoke

The debug-only instrumentation runner reads one app-private credential input
streamed by adb, deletes it before registration/login, and outputs only stage
results. It never embeds credentials or fabricates a game save. Keep the
Windows DPAPI credential file outside Git (for this task it is under `.local`).

```powershell
.\android\test-device.ps1 -CredentialFile .local/android-test-account.clixml
```

For an explicitly authorized new registration only, pass `-Invitation` a
`SecureString` from `Read-Host -AsSecureString`; do not retry a consumed invite.
The runner tests native library loading, HTTPS, login, lease acquire,
heartbeat, resume lookup/signature if present, release and token revocation.
Its native-load check is not evidence of executing the missing game ROM.

## Provenance

mGBA: <https://github.com/mgba-emu/mgba/tree/26b7884bc25a5933960f3cdcd98bac1ae14d42e2>
(MPL 2.0; license included as an APK asset).
JCS: <https://github.com/erdtman/java-json-canonicalization> (Apache 2.0).
Bouncy Castle: <https://www.bouncycastle.org/licence.html>.
Do not redistribute upstream game material without the required permission.
