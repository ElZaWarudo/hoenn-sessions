# Hoenn Sessions for Android

The APK includes the compatible game, mGBA 0.10.5 and the Rust cooperative client.
Open the app, enter your account and choose **Iniciar sesión y jugar**. No ROM picker
or separate runtime download is required. The rotating refresh token is encrypted
with Android Keystore, so reopening the app restores the account without asking for
the password again. **Cerrar sesión** revokes and removes that saved credential.
Save inside the game before closing.

Touch controls are drawn over the emulator with a transparent outlined layout.
Use Android's **Back** gesture to open **Menú**, then choose
**Configurar controles en pantalla** to change their size, drag each control to a
new position, hide the overlay, enable the optional fast-forward control or restore
the default layout. Fast-forward runs only while its button is held. These choices
persist on the device. After authentication the game switches to an immersive,
edge-to-edge view without floating corner buttons.

**Menú → Configurar mando** provides ten button mappings, collision swaps, stick
 deadzone (10–50%), a left-stick toggle, input testing and reset. Settings persist
on this device. Disconnected controllers and
loss of focus release held inputs. A Bluetooth/USB controller must first be paired
with Android. Android 9+ on ARM64 or x86_64 is required.

## Build locally

Install Rust via rustup, Python 3, Android Studio/JDK 21+ and SDK command-line tools.
Run from the repository root:

```powershell
$env:JAVA_HOME = 'C:/Program Files/Android/Android Studio/jbr'
.\android\build.ps1 -Test -Rom 'C:/private/game.gba' -Manifest 'C:/private/bridge_manifest.json'
```

The ROM and manifest must come from the **same deployed release**. BuildConfig
uses that manifest's ROM hash and memory addresses. The build rejects mismatching
ROM bytes; startup verifies the bundled bytes again before installing them into
private app storage. Existing saves are preserved. Without `-Manifest`, the checked
in Android manifest identifies deployed ROM SHA-256
`1935a5b99e40922fa915dcbcc28c4e210ffc4b4241747a91f586670fdfc7078d`.
Without `-Rom`, the build uses the repository's `pokeemerald.gba`.

The script installs SDK 37, NDK 27.2.12479018, CMake 3.22.1, and clean mGBA sources
at `26b7884bc25a5933960f3cdcd98bac1ae14d42e2`. Both Android native ABIs are built
with Cargo's lockfile. Release libraries support 16 KiB pages.
Output: `app/build/outputs/apk/debug/app-debug.apk` (development signature).

For a signed release, set `HOENN_ROM_PATH`, `HOENN_MANIFEST_PATH`,
`ANDROID_KEYSTORE_PATH`, and `ANDROID_KEYSTORE_PASSWORD`; use key alias
`hoenn-android` and run `android/gradlew.bat -p android assembleRelease`.
Keep the keystore backed up privately: future updates require the same signer.

## GitHub CI and delivery

The CI Emerald job builds the ROM once, generates its manifest, builds the bundled
APK, runs Java tests and Android lint, and verifies the actual packaged ROM,
manifest, native libraries, signature and alignment. A PR build proves compatibility
with its own ROM; only a matching deployed ROM can join the live server.

The production release job builds a signed APK from the exact ROM/manifest used by
the server image. Configure repository secrets `ANDROID_KEYSTORE_B64` and
`ANDROID_KEYSTORE_PASSWORD` (alias `hoenn-android`). Missing keys fail the release
before server publication. The APK and SHA-256 are stored privately on the VPS at
`/srv/hoenn/android/<commit>/`. They are outside the signed Windows envelope.
Previously promoted releases are reused, not rebuilt; APK generation follows the
same fresh-release condition.

This repository is public. APKs contain the ROM, so CI deliberately does not upload
them to public workflow artifacts or GitHub Releases. Distribute the APK through
the project's private channel. No BIOS, account, password, private key or save is
included in an APK.

## Server and trust

Endpoint: https://169-128-190-115.sslip.io. TLS and hostname validation are enabled.
Cloud saves require the pinned `pilot-v1` Ed25519 key:
`f239614d143272c416c185e4d52d95a883f7ad89cadf55e1cf1dbc9dda8fe188`.
The Android core/wire version is 0.10.5, separate from the Windows emulator
metadata in the shared ROM manifest. Updating Android does not deploy the VPS.

An APK build and a successful login are not evidence of an accepted cloud save.
Gameplay, save/resume and physical controller checks must be recorded separately.
