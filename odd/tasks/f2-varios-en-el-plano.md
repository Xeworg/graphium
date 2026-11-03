# F2 — Varios diagramas en el plano (primer corte)

Primer corte de F2 (`docs/PRD.md` §12): el plano deja de mostrar **un**
diagrama por vez y pasa a ser el canvas que la decisión v1.5 del PRD promete:
varios objetos con posición, panel lateral para elegirlos y cerrarlos, y la
cámara que va hasta donde el humano (o el agente) apunta.

Corte elegido por el humano el 2026-11-03 entre las cuatro opciones de F2
(«varios en el plano» ganó a export, los 10 kinds y el ruteo con canales).

Criterio de salida (PRD §14, criterio 13, verbatim): «Con dos diagramas
abiertos, ninguno se superpone; al pedir un tercero la cámara se desplaza
hasta él, elegirlo en el panel lateral centra la cámara en él, y cerrarlo
desde el panel lo saca del plano sin romper la sesión MCP.» Más el tope de
memoria de F2: «RSS < 250 MB con 12 diagramas de 300 nodos».

## Specs

- **S1** (PRD §7, RF26): «Cada diagrama abierto es un objeto con posición. Panel lateral izquierdo con la lista de abiertos (título, kind, actualizado) y botón de cerrar por ítem; al elegir un ítem, la **cámara se desplaza hasta ese diagrama**.»
- **S2** (PRD §7, RF27): «Un diagrama nuevo aparece **en el centro de la vista actual**; si ya hay otro diagrama ahí, se separa en cascada hasta encontrar hueco — **nunca se superponen**. Al crearse con el programa abierto, la cámara hace **pan hasta el diagrama nuevo**. `opts.focus: {id, depth?}` centra un nodo y opcionalmente su vecindad (depth N, default 1).»
- **S3** (PRD §7, RF28): «Cerrar un diagrama (panel lateral o `×` sobre el objeto) lo saca del plano y **libera su slot**, no su identificador: el `diagram_id` queda **invalidado y nunca se reutiliza** (RF28.1, §11.1); cerrar **no** cierra la ventana ni mata la sesión MCP. Cerrar la ventana entera tampoco (§8.6).»
- **S4** (PRD §11.1, RF28.1): «El id es opaco, **monótono y nunca se reutiliza** (`d-0001`, `d-0002`, …): cerrar un diagrama lo **invalida**, no lo devuelve al pool (lo que se libera es el slot del plano, no el identificador).» y «Cerrar **no borra** el documento fuente: queda en memoria hasta que se cierre la ventana o se llame `clear_canvas`; `list_diagrams` devuelve solo los abiertos.»
- **S5** (PRD §7.2, geometría del objeto y cascada): «El objeto de un diagrama es su SVG más **24 px de padding**, en coordenadas del plano.» y «Se coloca con su **centro en el centro de la vista actual**, conservando el zoom del humano (nunca > 100% al insertar).» y «Si su rectángulo se solapa con otro (±24 px), **cascada** con offset `(48, 48)` px hasta 8 pasos; al agotarse, columna nueva a la derecha del bounding box global + 48 px.» y «Si el destino queda fuera del viewport → **pan animado de 250 ms** hasta él.»
- **S6** (PRD RNF1-ter y §7.2): «El canvas mantiene **como máximo 3 SVGs en el DOM**: los diagramas fuera del viewport con `zoom < 0.4` se sustituyen por un recuadro con título y tamaño, y se re-materializan al entrar en vista.» y «por debajo de `zoom 0.3` la grilla se oculta (muaré) y los diagramas se ven como recuadros con título (culling, §7.3).»
- **S7** (PRD §11.1, RF13): «Varios diagramas simultáneos en el plano: tope configurable (default 12) con error claro `too_many_diagrams`; **sin barra de pestañas**.»
- **S8** (PRD §8.6.1, estado del frontend): «Al crear o recrear el webview, Rust emite `graphium://state` con la lista completa `{diagram_id, title, kind, updated_at, pos, bbox, session_id}` y el panel lateral se construye **solo** desde ese evento. `list_diagrams` lee el mismo `DiagramStore`.»
- **S9** (PRD §11.1, capacidad): «**F1 (capacidad 1).** Un segundo `render_diagram` **crea un diagrama nuevo, cierra el anterior** y responde `{ ok, diagram_id, replaced: "<id anterior>" | null }`.» — en F2 eso deja de valer: el render **suma** al plano y `replaced` queda `null` salvo `opts.replace_id`.
- **S10** (PRD F2, salida): «RSS < 250 MB con 12 diagramas de 300 nodos.»

## Tasks

| ID | Tarea | Specs | Ruta | Commit |
|---|---|---|---|---|
| T1 | Feature doc de F2 (este corte) + mirror + lista de tareas | S1-S10 | inline | este commit |
| T2 | Store de N diagramas: posición y bbox por diagrama, cierre que invalida el id, tope 12 con `too_many_diagrams`, `list` solo de abiertos | S3, S4, S7, S8, S9 | inline | `a458d91` — 4 tests en `store.rs`, 215 en verde |
| T3 | Colocación y cascada puras en Rust: centro de la vista, padding 24, solape ±24, offset `(48, 48)` hasta 8, columna nueva | S1, S2, S5 | inline | `1a1656f` — 6 tests en `placement.rs`, 221 en verde |
| T4 | `render_diagram` suma al plano; `graphium://state` con `pos` y `bbox` | S8, S9, S7 | inline | `e065aed` — 4 tests nuevos, 225 en verde |
| T5 | Panel lateral: lista, selección que desplaza la cámara, cierre por ítem y `×` sobre el objeto, pan animado de 250 ms. Incluye: el zoom deja de cambiar cuando el plano ya tiene diagramas | S1, S2, S3, S5 | worker | — |
| T6 | `opts.focus: {id, depth?}`: encuadra el nodo y su vecindad | S2 | inline | — |
| T7 | Culling: 3 SVGs como máximo en el DOM y recuadros con título; RNF10 medido con 12 diagramas | S6, S10 | inline | — |

## Log

- **L1** (2026-11-03, arranque de F2): el humano eligió el corte «varios diagramas en el plano + panel lateral» entre las cuatro opciones de F2. Motivo declarado en la pregunta: es la promesa central del plano (decisión v1.5 del PRD) y hoy el producto abre un diagrama por vez.

- **L2** (2026-11-03, verificación de T4 y un ajuste de la cámara): con tres diagramas en el plano se ve el efecto de T8 — cada render encuadraba el diagrama nuevo, así que el zoom del humano se reajustaba en cada inserción y los otros objetos se achicaban. El PRD pide «pan hasta el diagrama nuevo» **conservando** el zoom (§7.2), así que el encuadre queda para el **primer** diagrama del plano y para cuando el humano elige un ítem; a partir del segundo, la cámara hace pan hasta el objeto nuevo sin tocar el zoom. Entra en T5.
