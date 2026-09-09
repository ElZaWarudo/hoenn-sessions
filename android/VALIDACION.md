# Validación local — 8 de septiembre de 2026

Registro histórico del APK inicial. Para el estado del código posterior,
incluida la ROM ya compilada, consultar [ESTADO-PR.md](ESTADO-PR.md).

## Resultado y límite

APK: `android/app/build/outputs/apk/debug/app-debug.apk` (12.789.164 bytes).
SHA-256: `c9c365e1d3b676de8c850e9c242c5f2fb7e603bad777e0e1a57b046bfb7d723d`.

APK compilado, firmado para depuración, instalado mediante adb y abierto en
`Hoenn_API_36` (`emulator-5554`). **No es todavía un emulador GBA cooperativo
terminado.** Existe un núcleo mGBA nativo y una capa JNI para el bridge, pero
el transporte cooperativo, la recuperación de partida y las transacciones de
guardado cloud aún no están integrados en Android. Tampoco se dispone de la
ROM exacta para verificar ejecución real.

## Repositorio y herramientas

- `main` estaba limpio. `git fetch` y `git pull --ff-only origin main`
  confirmaron que ya estaba actualizado. Cambios en `codex/android-client`;
  no se hizo push ni se modificó el VPS.
- Windows 11 Home x64, build 26200; 16 GB de RAM. WHPX disponible y operativo.
  El campo heredado WindowsProductName decía Windows 10; CIM identificó el
  sistema real como Windows 11.
- Se reutilizó el JDK 25.0.2 de Android Studio y el SDK existente. Se instalaron
  herramientas CLI, NDK 27.2.12479018, CMake 3.22.1 e imagen Google APIs de
  Android 16/API 36 para x86_64. Gradle 9.5.0 y AGP 9.3.1.
- `assembleDebug` y `testDebugUnitTest`: correctos. Seis pruebas unitarias,
  cero errores: vector Ed25519 RFC8032, clave equivocada, alteración de mensaje
  y firma, claves débiles, key ID no admitido, CRC/dirección del bridge, epoch
  incorrecto, secuencia cero y cabecera ABI alterada.
- `apksigner verify --print-certs`: correcto, certificado Android Debug.
- Se ejecutó además `android/build.ps1 -Test`: correcto. `open-emulator.ps1`
  instaló y abrió el APK. La actividad quedó en primer plano y el botón de
  HTTPS mostró «HTTPS válido · servidor preparado. Firma pilot-v1 pendiente
  de un paquete firmado».
- El APK contiene ambas bibliotecas JNI, el manifiesto público y la licencia
  mGBA. Se inspeccionaron todas sus entradas descomprimidas: ni la contraseña
  de prueba ni la invitación están presentes. El directorio privado de entrada
  del runner quedó vacío.

## Prueba real desde Android

El runner de depuración ejecutó los endpoints reales del servidor:

```text
native_load=PASS
bridge_out_of_bounds=REJECTED
https_readiness=PASS
registration=PASS
login=PASS
lease_acquire=PASS revision=0
lease_heartbeat=PASS
pinned_server_signature=UNTESTED no snapshot (HTTP 404, revision 0)
lease_release=PASS
logout=PASS
game_presence_cloud_save=UNTESTED matching ROM unavailable
```

La invitación proporcionada se consumió en un registro correcto. La cuenta es
una cuenta de prueba local; su contraseña aleatoria está cifrada con Windows DPAPI
en `.local/android-test-account.clixml`, ignorado por Git. El archivo de entrada
en Android se eliminó antes de realizar el registro; los tokens se revocaron
mediante logout. No se incluyen credenciales en el APK.

Hubo dos fallos de DNS anteriores al registro. La conectividad IP funcionaba,
pero el Wi-Fi virtual no resolvía nombres para procesos de aplicación. Se
configuraron DNS explícitos y se utilizó la conexión celular del AVD. No se
desactivaron la confianza de certificados ni la comprobación del hostname.

La respuesta de health no está firmada y la cuenta nueva no tiene snapshot.
Por ello **no se ha verificado una firma real `pilot-v1` contra el servidor**;
solo se ha probado la implementación criptográfica con vectores de prueba y
rechazos negativos. No se inventó un guardado para obtener una firma.

## Artefacto bloqueante

El manifiesto incluido procede de
`git show 8fa887dc04:dist/bridge_manifest.json` y requiere `pokeemerald.gba`:

```text
8d599e9742e14c4418e754f9a105299d99058c584f59a7a1b5ae35eb8b336eb0
```

La búsqueda en `C:/Users`, OneDrive, Descargas, Documentos, Escritorio,
`C:/temp` y `C:/LDPlayer` no encontró el juego ni `pokeemerald.elf`.
Los pequeños multiboot del repositorio y los fixtures de upstream mGBA no son
esa ROM y no se utilizaron para simular una prueba del juego.

No se probaron carga del juego, botones/audio con la ROM, presencia entre
jugadores, escritura de un SAV por el juego, restauración ni subida de guardado.
La prueba de carga de biblioteca nativa no demuestra esas funciones.

## Repetir

Desde la raíz del repositorio, en PowerShell 7:

```powershell
.\android\build.ps1 -Test
.\android\open-emulator.ps1
.\android\test-device.ps1 -CredentialFile .local/android-test-account.clixml
```

El tercer comando vuelve a iniciar sesión con la cuenta existente; no necesita
ni debe reutilizar la invitación. El segundo comando instala
`android/app/build/outputs/apk/debug/app-debug.apk` y abre
`io.hoenn.sessions/.MainActivity`.

El inventario de lo implementado y del trabajo pendiente del bridge está en
[README.md](README.md).
