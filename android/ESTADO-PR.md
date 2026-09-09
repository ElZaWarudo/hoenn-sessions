# Estado del borrador — 9 de septiembre de 2026

Este borrador conserva el cliente Android y la integración cooperativa en curso.
No certifica un flujo multijugador completo ni está listo para distribuirse.

## Evidencia actual

- Validación de este borrador: `cargo test -p coop-launcher --lib --locked`
  pasó con 147 pruebas correctas y una ignorada; `cargo fmt --all -- --check`
  pasó. `assembleDebug testDebugUnitTest` pasó con seis pruebas Java correctas.
  Compilar el APK no verifica la carga de la biblioteca Rust pendiente.

- Se compiló una ROM de 32 MiB y su ELF desde el commit
  `333a5f3991607298e66f66e68ada6ad20867c0a9`, con GCC ARM 16.1.0
  en MSYS2 UCRT64 y `make -r -j4 modern`.
- SHA-256 de la ROM:
  `06764f4afa0d8a9664f28bfbc874dd0b9421c6b932765648034621146ce7893e`.
- El generador del proyecto produjo el manifiesto sin editar hashes ni
  direcciones manualmente. Su copia para despliegue está en
  `android/artifacts/bridge_manifest.windows-build.json`.
- En Windows se compilaron launcher y sidecar, se abrió mGBA oficial 0.10.5
  con una sesión del servidor HTTPS y se cargó el script mediante su consola.
  Se observó «co-op bridge authenticated with the local sidecar» y la ROM
  ejecutándose. Esto no demuestra presencia entre dos jugadores.
- La biblioteca Rust para Android x86_64 compiló en release antes de esta PR.
- `VALIDACION.md` conserva resultados del APK anterior; no valida los cambios
  posteriores de `NativeSession` y `BridgeConnection`.

## Pendiente antes de aprobar

- Integrar la compilación y empaquetado de `libcoop_android.so` para ambas ABI
  en el flujo Gradle/PowerShell. El script de compilación actual solo prepara
  el núcleo C de mGBA: no garantiza un APK ejecutable con NativeSession.
- El asset Android todavía corresponde al manifiesto anterior. Actualizarlo
  de forma coherente con la ROM y el despliegue; nunca cambiar solo el hash.
- Verificar cierre de Activity, limpieza de errores tempranos, desconexión y
  reconexión completa, además de las comprobaciones de identidad nativa.
- Producir un primer guardado desde el juego y comprobar subida, firma real
  `pilot-v1`, cierre y recuperación. No se fabricó un SAV para esa prueba.
- Probar presencia con dos cuentas y dos clientes. La actualización del
  manifiesto en el VPS no ha sido confirmada; esta PR no modifica el VPS.

No se incluyen ROM, ELF, partidas, tokens, contraseñas, invitaciones ni
credenciales Firebase. Las claves incluidas son públicas.
