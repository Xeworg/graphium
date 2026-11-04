# Sesiones visibles: registro, aislamiento y selector

Corte pedido después de cerrar F2. Hoy una «sesión» es un string que se usa como
cerco (`session_id`) y no tiene nombre, familia, estado ni lista de diagramas:
la interfaz no puede mostrar quién dibujó qué, y el humano no puede elegir qué
mirar cuando hay varios agentes trabajando. Este corte convierte la sesión en un
objeto visible y deja el aislamiento existente funcionando de verdad.

**No entra acá** el proceso en background (daemon detached, tray, ventana como
vista, el humano como sesión `ui`): eso es el corte siguiente y depende de éste.

Criterio de salida: con dos sesiones vivas (Pi y otro cliente MCP) la ventana
muestra dos filas en el selector con su familia y su proyecto; elegir una
**oculta** los diagramas de la otra sin cerrarlos; al terminar una sesión sus
diagramas desaparecen y la otra queda intacta; y `MAX_SESSIONS` cuenta también
las sesiones stdio.

## Specs

- **S1** (el humano, verbatim): «tendriamos que adaptar la interfaz a multiples sesiones para que los agentes no toquen el proyecto de otro agente»
- **S2** (el humano, verbatim): «por ahora sera mejor que se borren» — al terminar una sesión, sus diagramas se borran.
- **S3** (el humano, verbatim): «lo de las sesiones seria como tener en la interfaz un selector de sesiones»
- **S4** (el humano, verbatim): «ya no solo debemos reconocer la sesion si no de que tipo de agente proviene, opencode, pi, claude»
- **S5** (el humano, verbatim): «el software inicia con la sesion de un agente, luego termimina con la sesion del ultimo agente» y «tomare la recomendacion» (el humano cuenta como sesión `ui`; se implementa en el corte siguiente).
- **S6** (PRD §11.1, verbatim): «**Alcance de sesión.** Cada diagrama guarda `session_id` (socket peer). Por defecto `list_diagrams` y `clear_canvas` operan **solo sobre la sesión emisora**; `clear_canvas({ scope: "plano" })` borra todo el plano. `get_diagram`/`update_diagram`/`export_diagram` sobre un diagrama de otra sesión → `not_owner` con `hint: "ese diagrama pertenece a otra sesión MCP"`. El tope de diagramas abiertos (12) es **por ventana**.»
- **S7** (PRD RNF9, verbatim): «Máximo **8 sesiones MCP** por owner; la novena recibe `too_many_sessions`.»
- **S8** (PRD §11.1, verbatim, **se reescribe en este corte**): «Cerrar **no borra** el documento fuente: queda en memoria hasta que se cierre la ventana o se llame `clear_canvas`» — contradice S2 desde que la sesión tiene fin propio.

### Decisiones de este corte (no son citas)

- **D1 — Identidad en tres niveles, y ninguna adivina.** (1) **declarada**: la extensión de Pi pasa `env` al registrar el MCP (`GRAPHIUM_CLIENT=pi`, `GRAPHIUM_SESSION=${PI_SESSION_ID}`, `GRAPHIUM_PROJECT=${PWD}`); (2) **protocolo**: `clientInfo.name`/`version` del handshake `initialize`; (3) **inferida**: proceso padre y `cwd`. La familia resultante es `pi`, `claude`, `opencode`, `ui` u `other`. Sin señal clara se dice `other` con el proceso padre como detalle: un agente mal etiquetado es peor que uno sin etiqueta. La identidad es auto-declarada: sirve para la interfaz, **no** es frontera de seguridad (el cerco sigue siendo `not_owner` y el socket 0600).
- **D2 — Etiqueta opcional con respaldo.** `register_session({ label })` deja que el agente nombre su sesión («presupuesto API»); si no la llama, la sesión se muestra con el nombre automático (`Pi · graphium · 4824`).
- **D3 — Aislamiento estricto.** Una sesión ve y modifica **solo** sus diagramas; cruzada → `not_owner`, como hoy. El humano es la excepción: la interfaz ve todas las sesiones y todos los diagramas.
- **D4 — Fin de sesión con gracia.** Cuando el transporte de una sesión se cierra, la sesión pasa a `en gracia` durante **45 s** (constante configurable) y después se borra con sus diagramas. La gracia existe para que `/mcp reconnect` y un reinicio del cliente no borren el plano; con identidad declarada el id se recupera y no hay gracia que valga.
- **D5 — Filtrar no borra.** El selector **oculta** los diagramas de las otras sesiones; no los cierra.
- **D6 — El conteo incluye stdio.** La sesión stdio del dueño cuenta para `MAX_SESSIONS` (hoy sólo cuentan los sockets), y `too_many_sessions` se emite de verdad.

## Tasks

| ID | Qué | Specs | Ruta | Commit |
|---|---|---|---|---|
| T1 | Registro de sesiones en el dueño: `SessionRecord`, resolución de identidad (env → `clientInfo` → proceso) y ciclo `activa`/`en gracia`; el transporte **stdio** se registra | S4, S6, D1, D4 | worker | `374d08b` |
| T1-bis | Registrar también cada conexión de **socket** como sesión: el relay y los ids `sock-<pid>-<n>` viven en `crates/gx-app/src/relay.rs`, no en `gx-mcp` (corrección de ruta a L4) | S4, D1 | worker | pending |
| T2 | `register_session({ label? })` que devuelve la sesión propia, y `session` en las respuestas de `render_diagram`/`list_diagrams` | S3, D2 | worker | `c876604` (PR #36) |
| T3 | Fin de sesión: borrar sus diagramas al vencer la gracia, y conteo real de sesiones (stdio + sockets) con `too_many_sessions`. Incluye T1-bis | S2, S7, D4, D6 | worker | pending |
| T4 | Interfaz: selector lateral (familia con icono/color, etiqueta, proyecto, estado, contador), opción «todas», badge por diagrama, aviso cuando desaparece la sesión que estabas mirando | S1, S3, D3, D5 | worker | `8bcf26d` (PR #37) |
| T5 | PRD: **RF30** nueva (sesiones visibles y aisladas), reescritura de §11.1 «Alcance de sesión» y de RNF9, nota en §12 | S6, S7, S8 | inline | pending |
| T6 | Extensión de Pi: registrar con `env` de identidad y re-registrar si cambia el id de sesión; `verify.mjs` en 17 checks | S4, D1 | worker | `4df7703` |
| T7 | Cierre: tres PRs con merge y sin tag — motor (#35), superficie (#36), selector (#37); ramas borradas | — | parent | `541e47c` + `ea6e2df` |

## Slices de revisión

Cuatro unidades de trabajo, cada una en su commit y con los tests y la doc del
mismo comportamiento al lado:

1. **Plan y contrato**: este documento (`0fef937`) y el PRD con RF30 (`52e4906`).
2. **Identidad**: registro de sesiones en el dueño (`374d08b`) y `env` de la
   extensión de Pi (`4df7703`).
3. **Conteo y fin de sesión**: T1-bis + T3 (`09ec836`; relay, borrado con gracia,
   `too_many_sessions`) y T2a (la clave declarada, para que un reconnect
   recupere la sesión).
4. **Superficie y vista**: T2 (`register_session` y la sesión en las respuestas)
   y T4 (selector).

### Entrega por PRs

El corte va en **dos PRs**, y el límite es el que separa revisar concurrencia de
revisar una pantalla:

- **PR A — el motor de sesiones**: documento, PRD, registro con identidad, `env`
  de la extensión, cuota, borrado con gracia y la clave declarada (T1, T1-bis,
  T3, T2a, T6). Es donde vive el riesgo de concurrencia y de contrato.
- **PR B — la superficie y la vista**: `register_session` y el objeto `session`
  en las respuestas (T2b) más el selector de la interfaz (T4). Se apoya en A.

Ninguno lleva tag: el release se decide aparte.

## Evidencia esperada

- Test de la resolución de identidad: env declarado gana; `clientInfo` cuando no hay env; `other` cuando no hay ninguna de las dos.
- Test de la gracia: cerrar el transporte deja la sesión `en gracia` con sus diagramas; pasado el plazo, se borran; reconectar con el mismo id declarado no borra nada.
- Test del conteo: 1 stdio + N sockets; en `MAX_SESSIONS` la siguiente recibe `too_many_sessions`.
- Interfaz: no hay harness de tests en `crates/gx-app/frontend` (no existe `package.json` ni specs), así que el selector se verifica **con el ojo** sobre dos sesiones reales, más un test de Rust de que el payload de estado lleva las sesiones y sus diagramas. Se declara así en el PR: lo automático cubre los datos, no el dibujo.

## Log

- **L1** (el humano, verbatim): «no, tendriamos que adaptar la interfaz a multiples sesiones para que los agentes no toquen el proyecto de otro agente, y los datos que ocupemos deberiamos manejarlos por la extension, o por las llamadas al mcp hay que ver que es mejor»; «por ahora sera mejor que se borren, lo complejo aca sera detectar cuando el mcp sigue en uso por otro programa que no sea pi ya no solo debemos reconocer la sesion si no de que tipo de agente proviene, opencode, pi, claude. etc»; «lo de las sesiones seria como tener en la interfaz un selector de sesiones»; «ahora explicame como haremos para cerrar el programa ya que ahora dependeremos de que se cierre la ultima sesion, el software inicia con la sesion de un agente, luego termimina con la sesion del ultimo agente»; «tomare la recomendacion».
- **L2** (decisión de arquitectura, 2026-11-04): los datos de sesión los posee el **dueño** (MCP), la extensión solo aporta identidad por `env`, y la interfaz lee del dueño. Razones: cobertura de todos los clientes MCP sin código por agente, un solo escritor del estado, y el estado vive en el mismo proceso que dibuja. Una extensión que guardara estado obligaría a una implementación por agente y a dos escritores que se desincronizan.
- **L3** (orden, 2026-11-04): el selector **no necesita** el daemon — el dueño ya recibe varias sesiones por socket (hasta 8) —, así que va primero y solo; el background (vida del proceso, tray, ventana, sesión `ui`) es el corte siguiente.
- **L4** (estado hoy, verificado en código): `session_id` es la constante `"stdio"` en el proceso dueño (`crates/gx-mcp/src/lib.rs:272-283`) y `sock-<pid>-<n>` para cada conexión reenviada; **el relay, esos ids y `MAX_SESSIONS` viven en `crates/gx-app/src/relay.rs:16-20,117`**, no en `gx-mcp` (corregido en T1); `not_owner` existe y se emite (`contract.rs:805,856`); `owner_unavailable` y `diagram_closed` son variantes muertas.
- **L5** (T1 y T6, medido): `cargo test -p gx-mcp --lib` 106 en verde y `node extensions/pi/graphium/verify.mjs` en 17 checks. La extensión lee `ctx.sessionManager.getSessionId()` (`dist/core/session-manager.d.ts:178,246`) y usa `PI_SESSION_ID` solo como respaldo, porque esa variable existe en los subprocesos de bash/powershell, no en la extensión; los valores `env` del registro MCP son plantillas `${…}`. Decisión del worker sobre la identidad: familia y versión salen **del mismo nivel** (una familia declarada no se mezcla con una versión de protocolo).

- **L6** (cierre, medido): el corte salió en tres PRs mergeados en `main` — motor `#35` (`541e47c` incluye el merge), superficie `#36`, selector `#37` (`ea6e2df`) — con las ramas borradas y sin tag. Gates en cada uno: `cargo test --workspace`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets`; y en el selector además `node --check`. **Lo que no está verificado por máquina es el dibujo del selector**: `crates/gx-app/frontend` no tiene harness de JS, así que falta mirarlo con dos sesiones vivas.
