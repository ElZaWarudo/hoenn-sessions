# Validación pendiente de autorización

El usuario pide autorización antes de realizar pruebas. Este documento no
autoriza su ejecución y no registra resultados nuevos.

## Primera tanda propuesta: local, sin sesión en el VPS

- Ejecutar las pruebas unitarias del launcher (incluido el supervisor embebido)
  y las seis pruebas Java de protocolo/firma.
- Inspeccionar el APK generado: bibliotecas para ambas ABI, manifiesto, firma
  de depuración y ausencia de credenciales o artefactos de juego.
- Instalar el APK en Hoenn_API_36 y abrir la pantalla inicial; comprobar que
  ambas bibliotecas cargan sin crash. No iniciar sesión ni modificar partidas.

## Segunda tanda: conexión y juego, requiere autorización separada

- Confirmar previamente que el VPS admite el manifiesto generado y que la
  cuenta de prueba no está activa en el cliente Windows.
- Conectar con esa cuenta, verificar HTTPS, adquisición de lease y epoch.
- Importar la ROM con hash exacto; probar frames, controles y audio.
- Jugar hasta que sea posible guardar y producir el primer SAV desde el juego.
- Confirmar la revisión aceptada en prepare/upload/finalize, cerrar y reabrir,
  verificar una firma real pilot-v1 y recuperar el progreso guardado.
- Reconectar desde el guardado con epoch nuevo; comprobar cierre al enviar la
  app a segundo plano y rechazo de errores de identidad/compatibilidad.
- Usar entradas de botones normales; no escribir memoria para crear progreso,
  no fabricar SAV, no editar hashes y no debilitar TLS o firmas.

## Tercera tanda: presencia, requiere otra cuenta y autorización

- Usar dos clientes y cuentas distintas con el mismo manifiesto admitido.
- Llegar al exterior de Villa Raíz; comprobar avatar remoto y movimiento en
  ambos sentidos, desconexión y reaparición. Que el bridge autentique no basta.

Conservar los fallos como fallos y anotar los límites de lo observado.
Nunca reutilizar la invitación consumida ni modificar el VPS desde este flujo.
