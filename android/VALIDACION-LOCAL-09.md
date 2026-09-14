# Primera tanda autorizada — 9 de septiembre de 2026

Alcance autorizado: pruebas unitarias, inspección del APK, instalación y
arranque en el emulador. Sin login, conexión al VPS ni modificación de partidas.

## Resultados

- `cargo test -p coop-launcher --lib --locked`: 149 correctas, 0 fallos,
  1 ignorada. Incluye dos pruebas del acuse de parada del supervisor embebido.
- `testDebugUnitTest`: 6 correctas, 0 fallos/errores (BridgeFrame y PinnedIdentity).
- APK `0.2-coop-validation`, versionCode 2: firma v2 Android Debug válida.
- `zipalign -c -P 16 4`: correcto. Segmentos PT_LOAD de las cuatro bibliotecas
  nativas con alineación 16384; ELF64 x86_64 y AArch64 correspondientes.
- Incluidos `libcoop_android.so` y `libhoenn.so` para ambas ABI. Asset con
  SHA-256 de ROM `06764f4afa0d8a9664f28bfbc874dd0b9421c6b932765648034621146ce7893e`.
- Inspección de entradas descomprimidas: no contraseña de la cuenta existente,
  invitación consumida, clave privada ni archivos de ROM/partida.
- Instalación adb `-r`: Success. Inicio de MainActivity: Status ok, COLD.
  Package Manager informa versionCode 2 y versión 0.2-coop-validation.
- Actividad en primer plano, proceso vivo, sin errores AndroidRuntime/libc
  fatales detectados para ese PID. La jerarquía UI muestra «Sin conexión
  comprobada», formularios vacíos, controles y el hash de ROM esperado.

APK SHA-256:
`6d0513fab0c9a90b62b51dea43ad57bcd9b16374837df32d2f8f1d5e45d6eece`.

## Incidencia del emulador

Durante el arranque en frío API 36 apareció «System UI isn't responding».
La primera instancia dejó de estar disponible. Se repitió con 3072 MiB,
4 núcleos, sin animación de arranque ni borrado de userdata. El aviso apareció
de nuevo; al elegir Wait quedó accesible la pantalla de la app. No se atribuye
este ANR a Hoenn ni se certifica que el ajuste de RAM lo haya resuelto.
El script ahora espera el servicio phone antes de habilitar datos celulares.

## No probado

No se pulsó HTTPS, registro, login, importación o juego. No se usaron
credenciales contra el servidor ni se modificaron partidas. Pendientes de
autorización separada: ROM y controles reales, HTTPS/login/lease, primer SAV
producido por el juego, subida, firma real pilot-v1, recuperación/reconexión
y presencia con dos cuentas. ARM64 se inspeccionó, no se ejecutó en un dispositivo.
