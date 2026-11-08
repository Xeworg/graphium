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
- L4 (2026-11-07, worker T1): plomería fina entregada como refactor puro, con
  las puertas verdes y sus códigos reales: `cargo test --workspace` 0 failed,
  `cargo fmt --check` exit 0, `cargo clippy --workspace` exit 0, `npm run
  build` exit 0, `node tools/parity.mjs` 57/57 exit 0, `DISPLAY=:0
  scripts/smoke.sh` → `rendered=true reason=none` exit 0.
  Qué cambió: (1) `svelte:window onresize onkeydown` reemplaza el par manual
  `window.addEventListener("resize"/"keydown")` y con él muere el bloque de
  `removeEventListener` de esas dos; el wheel y el pointer de `#plan` siguen
  imperativos (su disposición es el test empírico de T4). (2) el cleanup de
  `onMount` limpia también `cameraTimer`, `settingsTimer`/`settingsPending` y
  `ghostTimer`: un timer que dispara después del desmontaje escribe en una
  página que ya no está. (3) el grid se publica en DOS pares derived+effect:
  `gridPaintStatic` (las 6 vars de settings) y `gridPaintCamera`
  (`--grid-step`, `--grid-opacity`, `--tx`, `--ty`, `--zoom`). (4) `state` es
  `$state.raw`, reemplazado wholesale en el listener de `graphium://state`.
  (5) `prunePlanCameras()` en `reconcileActivePlan`. (6) props muertos fuera:
  `ontoggle` de PlanPanel, `sessions` de SessionsPanel.
  Evidencia del split (medida, no argumento): sobre el bundle construido, un
  sello en `CSSStyleDeclaration.prototype.setProperty` más un wheel por CDP —
  antes: 11 escrituras por frame de pan, las 6 de settings incluidas (el
  efecto único escribía 11 vars, no 10: el documento contaba de menos);
  después: 5 por frame, y 0 de las 6 estáticas en los frames siguientes. Un
  cambio de settings sigue escribiendo las 11 (+7 de paleta = 18, idéntico a
  antes), así que los valores no cambiaron de forma: `parity.mjs` 57/57 lo
  confirma leyendo `--grid-opacity`/`--tx`/`--ty`/`--zoom` con
  `getPropertyValue`.
  Verificación de `$state.raw`: se buscaron escrituras campo por campo y no
  había ninguna fuera del listener — las tres asignaciones
  (`state.diagrams/sessions/clones = …`) **eran** el listener, y cada evento
  trae los tres arrays, así que pasaron a un solo reemplazo del objeto (misma
  semántica de reemplazo C17). Quedan 18 sitios de lectura (`find`, `filter`,
  `map`, `length`, índices), ninguna llamada mutadora sobre los arrays
  (`push`/`splice`/`sort`/…) y `CloneTools` solo lee (`clone.updatable`). El
  proxy profundo envolvía cada diagrama y cada clon del payload sin que nadie
  lo necesitara.
  Prune de `planCameras`: `prunePlanCameras()` corre al final de
  `reconcileActivePlan` — después de `showPlan`, que acaba de guardar la
  cámara del plano que deja, incluido el que terminó — y borra toda entrada
  que el state ya no lista (más `user`). Prueba: s1 visitada y paneada, s1
  desaparece del state, s1 vuelve con el mismo id y se la elige de nuevo —
  antes la cámara volvía al valor viejo (275.99/67.32 @1.331), ahora cae al
  fallback `newestCenter` (296.36/97.56 @1.21). El mapa deja de crecer con
  una entrada por sesión que el humano visitó.
- L5 (2026-11-08, orchestrator inline): T2 entregada — la cámara es CSS puro y
  el culling tiene un embudo único. (1) La regla `.diagrams { transform:
  translate(var(--tx),var(--ty)) scale(var(--zoom)) }` reemplaza
  `layerTransform`, su `$effect` y `bind:this={diagramsEl}`: las tres vars ya
  se publicaban en `#plan` con cada escritura de cámara (split de T1), así que
  el transform de cámara costó CERO JavaScript por frame. `will-change:
  transform` se movió de `.diagram` a `.diagrams` con el comentario de
  `app.css` actualizado (el bug de will-change por objeto, cerrado). (2) La
  decisión del culling es pura: `cullDecide(plane, camera, viewport)` en
  `lib/plane.js` devuelve el conjunto de ids vivos; `applyCulling` en
  `App.svelte` es el ÚNICO embudo que toda escritura de cámara atraviesa
  (pan step, wheel, camera intent, resize, inserción, pintura de clon, cambio
  de plano) y lo aplica a los registros de forma imperativa — sin `$effect`,
  que re-entraría en reactividad detrás de las escrituras DOM del frame.
  `cullRecord`/`materializeRecord` murieron: un objeto sin dibujo que la
  decisión mantiene vivo conserva su caja en la misma asignación
  (`culled = !alive || !(preview||svg)`). (3) Medida `{@html}` vs action
  (herramienta nueva `tools/measure-html.mjs`, Chromium headless sobre el
  bundle real, fixture de 300 nodos / 48 172 B de SVG, mediana de 5 muestras
  de 50 ops): action (re-monte del mismo nodo, sin parseo) 0,060 ms/monte;
  `{@html}` (re-parseo completo por monte) 1,466 ms/monte — 24,4x peor pero
  BAJO el umbral acordado de ~3 ms. ADOPTADO: los registros guardan el markup
  como CADENA (`rec.svg`/`rec.previewSvg` ya no son nodos DOM dentro del
  estado reactivo — la isla del proxy muere), el template monta con
  `{@html rec.previewSvg || rec.svg}` antes de las herramientas (mismo orden
  prepend), el parseo de llegada sigue validando el payload y leyendo
  width/height, y el action `svgMount` desapareció. Gates (códigos reales):
  `npm run build` exit 0, `node tools/parity.mjs` 57/57 exit 0, `DISPLAY=:0
  scripts/smoke.sh` → `rendered=true reason=none` exit 0, `cargo test
  --workspace` 37 suites / 0 failed, `cargo fmt --check` exit 0, `cargo
  clippy --workspace` exit 0. La delegación al worker falló tres veces por
  validación del runtime (bloque `## Allowed edit surfaces`, y el resolutor
  no ve el documento de la feature fuera del cwd del orquestador): T2 fue
  inline por orden del runtime; T3 se relanza con `workspace_root` apuntando
  al worktree.
