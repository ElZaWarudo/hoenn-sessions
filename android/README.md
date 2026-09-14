# Android cooperative client — pending runtime validation

The client embeds mGBA 0.10.5 and the Rust launcher/sidecar in one Android
process. Touch controls, audio, JNI queues, server-issued leases/epochs,
realtime presence and canonical save transactions are implemented. They still
require end-to-end validation; see [current status](ESTADO-PR.md).

## Build on Windows

Install Rust using rustup, Android Studio/JDK and the Android SDK. For a GNU
Rust host, host GCC must be on PATH; MSYS2_ROOT selects its installation,
defaulting to %USERPROFILE%/.hoenn-tools/msys64 on this machine.

From the repository root, run:

```powershell
.\android\build.ps1
```

The script prepares NDK 27.2.12479018, CMake 3.22.1 and clean mGBA source
at commit 26b7884bc25a5933960f3cdcd98bac1ae14d42e2. Gradle preBuild invokes
build-rust.ps1 for x86_64 and arm64-v8a, packaging libcoop_android.so alongside
the C/JNI core. Cargo uses the lockfile. Android linking supports 16 KiB pages.
The core version/source commit are checked before opening an online game;
Windows executable digests are not Android native identity evidence.

Output: app/build/outputs/apk/debug/app-debug.apk (development signature).
Compilation does not run tests. In this session, obtain user authorization
before any test, device launch or server connection.

## ROM and server

Import pokeemerald.gba through the document picker. Its whole-file SHA-256 must be:

```text
06764f4afa0d8a9664f28bfbc874dd0b9421c6b932765648034621146ce7893e
```

ROM/ELF source commit: 333a5f3991607298e66f66e68ada6ad20867c0a9.
The generator produced artifacts/bridge_manifest.windows-build.json; the APK
asset is a copy of it. BuildConfig derives addresses and hashes from the asset.
The VPS must admit that same manifest. This client does not update the VPS.

Endpoint: https://169-128-190-115.sslip.io.
Pinned Ed25519 ID: pilot-v1; public key:
f239614d143272c416c185e4d52d95a883f7ad89cadf55e1cf1dbc9dda8fe188.

TLS certificates and hostnames remain validated. Resume packages require
the pinned signature and matching character, revision, build and artifact hashes.
Revision zero has no signed resume: health/login do not prove signing identity.
No ROM, BIOS, account, invitation, private key or save is packaged.

## Play, save and close

1. Import the exact ROM and select login/play with your account.
2. Rust acquires a lease and restores a verified canonical character.sav, or
   lets the game produce the first save at revision zero.
3. Java bridges memory queues to the authenticated in-process sidecar.
   Presence becomes eligible outdoors in Littleroot.
4. Save using the game menu. Java waits for the matching grant, generation and
   mGBA savedata callback before synchronizing actual Flash1M bytes. Rust validates
   the canonical container and performs prepare/upload/finalize. The UI reports
   a cloud revision only after server acceptance.
5. Reconnect starts from the cloud save after confirmed core/sidecar shutdown
   and a newer server epoch. It requires an accepted save. An unresolved
   checkpoint fails closed and preserves recovery material.
6. Close after saving. Leaving the foreground requests graceful close: the core
   runs briefly without audio/input to finish an already-ready checkpoint.
   Reopening requires login and restores the server save. Force-killing Android
   cannot guarantee graceful drain; unsaved game progress is not a cloud save.

Desktop savestates are not loaded into Android. The canonical SAV is shared;
optional Windows states are not presumed portable. Refresh tokens remain in
memory and are revoked on graceful closure. Android backup is disabled.

## Validation — authorization required in this session

```powershell
.\android\build.ps1 -Test
.\android\open-emulator.ps1
.\android\test-device.ps1 -CredentialFile .local/android-test-account.clixml
```

The emulator script boots Hoenn_API_36, installs the APK and opens the app.
It uses WHPX, software graphics and explicit virtual-cellular DNS. It never
disables TLS checks or changes the VPS.

The debug smoke checks native loading and cloud authentication/leases.
Credentials are streamed into app-private storage, removed before use and
never printed. It does not replace interactive gameplay validation.

Still to validate: ROM frames/controls/audio, first game-produced save/upload,
real pilot-v1 resume signature, close/recovery/reconnect, and two-client presence
with distinct accounts. Historical evidence is in VALIDACION.md.

mGBA is MPL 2.0 (license included in assets). JCS uses java-json-canonicalization;
Ed25519 also uses Bouncy Castle. Rust reuses the workspace protocol and save parser.
