# Estado de Android — 14 de septiembre de 2026

Se prepara una PR en borrador con todo el trabajo pendiente, solicitada el
14 de septiembre. La primera tanda local autorizada está documentada en
[VALIDACION-LOCAL-09.md](VALIDACION-LOCAL-09.md).
No se certifica todavía el flujo multijugador completo.

## Cambios posteriores a la validación histórica

- Gradle compila y empaqueta la biblioteca Rust para x86_64 y arm64-v8a junto
  al núcleo mGBA/JNI. Las fuentes mGBA deben corresponder al commit fijado.
- El asset Android usa el manifiesto generado de la ROM compilada. Las
  direcciones y hash de BuildConfig se derivan del asset, sin overrides.
- JNI informa la versión y commit del núcleo, comprobados antes del juego.
- El cierre espera confirmación del núcleo y de las tareas del sidecar;
  no libera la lease si la parada no se puede confirmar. La pantalla sigue
  atendiendo el cierre al salir de primer plano.
- Reconexión explícita desde el guardado cloud: después de parar la ejecución
  anterior, el servidor emite un nuevo epoch y se verifica de nuevo el paquete.
- El juego produce el SAV. El callback, el grant y la generación deben
  correlacionar antes de subirlo mediante el parser/lifecycle compartido.
  La UI solo anuncia revisión cloud después de finalize aceptado.
- Se añadieron dos pruebas de confirmación de parada del supervisor embebido;
  pasaron dentro de la primera tanda autorizada (149 pruebas Rust, 6 Java).

## Evidencia actual

- Validación histórica del commit `1e68460cb2`, no de los cambios posteriores:
  `cargo test -p coop-launcher --lib --locked`
  pasó con 147 pruebas correctas y una ignorada; `cargo fmt --all -- --check`
  pasó. `assembleDebug testDebugUnitTest` pasó con seis pruebas Java correctas.
  Compilar el APK no verifica su comportamiento en ejecución.

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

## Pendiente de autorización y ejecución

- Ejecución real de esta ROM. Las pruebas unitarias, inspección del APK e
  instalación/arranque ya se realizaron; el emulador tuvo un ANR de System UI.
- Verificar cierre de Activity, limpieza de errores tempranos, desconexión y
  reconexión completa, además de las comprobaciones de identidad nativa.
- Producir un primer guardado desde el juego y comprobar subida, firma real
  `pilot-v1`, cierre y recuperación. No se fabricó un SAV para esa prueba.
- Probar presencia con dos cuentas y dos clientes. La actualización del
  manifiesto en el VPS no ha sido confirmada; esta PR no modifica el VPS.

No se incluyen ROM, ELF, partidas, tokens, contraseñas, invitaciones ni
credenciales Firebase. Las claves incluidas son públicas.
