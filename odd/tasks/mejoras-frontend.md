# mejoras-frontend — corrección, a11y y utilidad Svelte sobre el plano consolidado

Branch `feature/mejoras-frontend` from `staging` @ 9348d48. Sources of the
criteria: two analysis rounds over this exact tree (ronda 1: 39 criterios con
2 bugs; ronda 2: 3 abogados del diablo con 4 desacuerdos resueltos por el
orquestador). Persisted in project memory: `analisis-frontend-*` and
`critica-frontend-*`.

## Specs

- S1 (user, verbatim): «dale hazlo en un worktree en una rama y que te vaya
  entregando segun el roden que consideres mas adecuado» — worktree + branch +
  progressive deliveries, order = the orchestrator's call.
- S2 (source of truth): the synthesized criteria list (16 high-ratio items +
  the found bugs/gaps). The delivery order: T1 plomería → T2 cámara/culling
  estructural → T3 a11y/UX → T4 entrada/experimentos.
- S3 (every delivery, no exceptions): `parity.mjs` 57/57, `scripts/smoke.sh`
  `rendered=true`, `cargo test --workspace` green, `cargo fmt`/`clippy` clean.
  CONTRACT immutables hold: wire names/payloads/throttles, numbers
  59/45°/41.72/24/3/0.4/zoom 0.05-4.0, strings verbatim Spanish (C41), CSSOM
  vars not inline styles (C43), ≤3 live SVGs, culling re-decides every camera
  write (no hysteresis), MAX_NAME_FANOUT=5, callback props (no
  dispatcher/slots), no TS, no i18n framework, strings C41 untouched.
- S4 (orchestrator): commits per delivery after orchestrator verification;
  no push.

## Tasks (delivery order)

- T1 (S2 S3; route: worker; commit: —): plomería fina —
  `svelte:window onresize onkeydown` (matan el bug de teardown: el bloque de
  removeEventListener muere); limpiar `cameraTimer`/`settingsTimer`/
  `ghostTimer` en el mismo cleanup; partir el efecto de grid en DOS
  (`gridPaintStatic` para las 6 vars de settings + el derivado de cámara
  `--tx/--ty/--zoom/--grid-step/--grid-opacity`) — hoy 6 writes CSSOM inútiles
  por frame; `$state.raw` para `state` del wire (reemplazo wholesale, C17);
  `planCameras.delete()` al terminar una sesión en `reconcileActivePlan`;
  props muertos `ontoggle`/`sessions` fuera; wheel/pointer quedan imperativos
  en T1 (la decisión template-vs-imperativo es T4, empírica).
- T2 (S2 S3; route: worker; commit: —): cámara CSS pura y culling estructural —
  `.diagrams { transform: translate(var(--tx),var(--ty)) scale(var(--zoom)) }`
  (los vars YA se publican en `#plan`): borra `layerTransform`, el tercer
  `$effect` y `bind:this={diagramsEl}`; `will-change: transform` de `.diagram`
  a `.diagrams` (comentario vencido de app.css actualizado); funnel de
  `applyCulling`: la decisión pura `(plane, camera, viewport) → vivos` llamada
  desde UN funnel que toda escritura de cámara atraviesa — SIN `$effect`
  (re-entra reactividad tras las escrituras DOM del frame); y la medida de
  `markup + {@html}` vs el action: costo de re-parseo en el fixture de 300
  nodos; si ≤ el umbral acordado (~3 ms), adoptar `{@html}` (mata la isla del
  proxy) — si no, revertir a action y documentar por qué.
- T3 (S2 S3; route: worker; commit: —): a11y y UX — `.panel-item-version-step`
  al focus-ring; el notice del switcher con región viva FUERA del aside
  `display:none` (visually-hidden, misma string C29); Escape + foco de vuelta
  en PlanPanel y SessionsPanel (copiar el patrón de SettingsPanel); trap del
  dialog o quitar `role="dialog"` (decisión documentada); ↑/↓/Home/End +
  roving tabindex en `ul#panel-list`; guards ←/→ (`event.defaultPrevented`,
  `[contenteditable]`); `.diagram` focusable (tabindex) y foco → `selectedId`
  (el historial por flechas deja de ser inalcanzable por teclado);
  `prefers-reduced-motion` guard (pan 250 ms + fades); `bind:offsetWidth` en
  CloneTools (borra la lista `void`); `setClonePlace` escribe `rec.rect.x/y`
  directo (sin spread por frame); `$derived.by` índice sessionId→count.
- T4 (S2 S3; route: worker; commit: —): entrada y experimentos —
  `touch-action: none` + mapa de dos punteros → `zoomAtPoint` (pinch);
  `getCoalescedEvents()` batching en pointermove; el test empírico
  wheel-template-vs-imperativo en WebKitGTK (decisión documentada; si el
  template falla, imperativo queda como documentación del preventDefault).

## Log

- L1 (2026-11-07, user): «dale hazlo en un worktree en una rama y que te vaya
  entregando segun el roden que consideres mas adecuado» — after reading the
  two analysis rounds («bien recuerda todo eso ahora quiero otros 3 que te
  digan esto no lo haria con x lo haria mejor con y dale los errores que ya
  conoces para que no los busquen de nuevo, que sean autocriticos con el uso
  de svelte hay que sacarle utilidad» and «ok ahora quiero que pongas 3
  agentes a analizar partes del frontend que se pueden mejorar con svelte y
  te den sus criterios»).
- L2 (2026-11-07, orchestrator): branch opened from staging @ 9348d48. The
  delivery order (S1) is by ratio and risk: T1 plumbing (kills the teardown
  bug, pure refactor), T2 structural camera/culling (parity-provable), T3
  a11y/UX (independent), T4 empirical/experimental (wheel test + pinch +
  {@html} measurement). Known bugs fixed in order: timers teardown (T1),
  planCameras prune (T1), will-change (T2), invisible focus ◀/▶ (T3), mute
  notice (T3), reduced-motion (T3), keyboard-unreachable history (T3),
  pinch absent (T4).
