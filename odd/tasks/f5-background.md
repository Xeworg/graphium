# f5-background

Daemon detached con tray: el proceso vive mientras haya sesiones, la ventana
es solo una vista y el humano cuenta como sesión `ui`. Diseño decidido por el
humano el 2026-11-04 (registro graphium-background-cut); este corte lo
implementa.

## Specs

- S1: «deberiamos iniciar en el porta papeles [tray] y el proceso morir
  cuando ya no hay sesiones tal como lo habia especificado» (2026-11-04,
  tras cerrar la ventana y ver que el proceso moría).
- S2 (diseño del humano, 2026-11-04): daemon detached `graphium --owner`
  que posee el store y la ventana, lanzado con `setsid()` y stdio a
  `/dev/null`; el `graphium --stdio` que Pi spawnea pasa a ser solo
  *forwarder*: hace `probe`, si no hay dueño lanza el daemon y espera el
  socket.
- S3 (diseño): ventana como vista — no existe al arrancar; se muestra cuando
  el usuario le da click al logo de la bandeja y cuando llega un diagrama
  (`show()` + `unminimize()` + `set_focus()`); cerrarla hace `prevent_close` +
  `hide()` (ocultar, NO minimizar) y el proceso queda vivo en la bandeja.
- S4 (diseño): máquina de estados sin dueño → dueño headless → dueño con
  ventana → drenando (gracia) → saliendo.
- S5 (diseño): cierre — el proceso vive mientras haya sesiones; al terminar
  la última, gracia de 30-60 s, borrar sus diagramas, drenar las tools en
  vuelo y salir con exit 0; salidas explícitas: tray «Salir» /
  `graphium quit` / SIGTERM manejado (si no, queda el socket sucio).
- S6 (diseño): el humano cuenta como sesión `ui` — ventana visible = sesión
  viva, ocultarla la termina («mirando = en uso», no se cierra el plano bajo
  sus pies).
- S7 (diseño, trampas): gracia obligatoria (si no, cada `/mcp reconnect`
  mata el programa); el conteo incluye stdio (RF30); no salir con una tool
  en vuelo; el forwarder muere con su transporte; el socket sucio se
  detecta por `connect()`.
- S8 (Tauri 2.12.5 verificado): cerrar la última ventana dispara
  `RunEvent::ExitRequested{code:None}` → `api.prevent_exit()`;
  `prevent_exit` sin salida explícita deja un proceso que solo muere con
  SIGKILL (tauri#13511); `AppHandle` es Clone+Send y
  `WebviewWindowBuilder::build()` se puede llamar desde el hilo del socket;
  mejor `hide`+`show` que destruir y reconstruir (tauri#9307); en macOS el
  click del dock necesita manejar `RunEvent::Reopen`; en Windows/Linux una
  ventana oculta es invisible sin tray; `backgroundThrottling` puede perder
  el `render_ack` de 2 s al reabrir tras horas.

## Tasks

- T1 (S2, S5): `graphium --owner` — modo daemon: `setsid()`, stdio a
  `/dev/null`, posesión del store y del socket; SIGTERM manejado; comando
  `graphium quit`. [commit: pendiente]
- T2 (S2, S7): `--stdio` forwarder — `probe` del socket; sin dueño lanza el
  daemon detached y espera; reenvía MCP por el socket; muere con su
  transporte. [commit: pendiente]
- T3 (S3, S8): ventana como vista — no se crea al arrancar; al primer
  diagrama `show()`+`unminimize()`+`set_focus()`; cerrar = `prevent_close` +
  `hide()`; `hide`+`show` nunca reconstruir. [commit: pendiente]
- T4 (S1, S6, S8): tray y sesión `ui` — icono en bandeja con «Salir»
  (`tauri features` + `RunEvent::Reopen` en macOS); ventana visible = sesión
  `ui` viva; ocultarla la termina. [commit: pendiente]
- T5 (S4, S5, S7): ciclo de vida — conteo de sesiones activas (stdio
  incluida), gracia y borrado de diagramas al vencer, drenado de tools en
  vuelo, exit 0; socket sucio detectado por `connect()`. [commit: pendiente]
- T6 (S1-S8): verificación en vivo (dos sesiones, cerrar ventana, quit) y
  docs (PRD RF30/RF31, README). [commit: pendiente]
## Log

- L1: «bien mientras esto se va compiando, ya puedo cerrar la ventana sin
  matar el proceso?» → «el proceso de graphium, cuando abri pi me abrio pero
  puedo cerrar la ventana y esta se puede volver a abrir cuando pida un
  diagrama ya la cerre haz un diagrama para probar»
- L2: prueba en vivo: con la ventana cerrada, `render_diagram` responde
  `unknown_kind: ''` dos veces (a investigar; puede ser el binario viejo
  corriendo o el envelope del harness) — y confirma que el proceso sigue
  vivo como servidor MCP aunque la ventana esté cerrada.
- L3: «en ese caso es era del cambio anterior deberiamos iniciar en el porta
  papeles y el proceso morir cuando ya no hay cesiones tal como lo habia
  especificado» — autorización del corte de background con el diseño ya
  decidido (S2-S8).
- L4: «hagamos el tag y asi lo pruebo de una vez» — PR #38 mergeado y tag
  v0.1.0 pusheado; el workflow de release quedó corriendo en segundo plano.
- L5: «que al iniciar no abra la interfaz principal que quede en la bandeja
  y se muestre cuando el usuario le de click al logo si cerramos desde la
  ventana se mantendra vivo en la bandeja, si una ia envia un diagrama la
  ventana se muestra con el diagrama» — afina S3: el click del tray también
  muestra la ventana; cerrarla deja el proceso vivo en la bandeja.
- L6: verificación en vivo (2026-11-04): daemon spawnado por forwarder,
  render crea la ventana con el diagrama (`rendered: true`), `graphium quit`
  sale limpio; el mismo escenario con ventana creada sale limpio también.
  Un primer zombie no se reprodujo: en esa corrida el dueño **viejo** (binario
  pre-f5 de la sesión de Pi, mismo socket) seguía vivo y el archivo del
  socket fue borrado a mano — dos oyentes en el mismo camino. Queda como
  punto de observación: nadie debe borrar el socket con un dueño vivo.
