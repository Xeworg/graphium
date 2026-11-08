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
- L6 (2026-11-08, worker T3): a11y y UX entregadas — los once ítems de la
  línea T3, con las dos decisiones que la tarea pedía documentadas (2 y 4).
  (1) `.panel-item-version-step` entra en la regla del focus-ring: los `◀`/`▶`
  del historial (panel y píldora del clon comparten clase) dejaban un foco
  invisible al tabular. (2) **Decisión**: el notice del switcher sale del
  `<aside>` y pasa a ser una región viva `role="status"` fuera de él,
  visualmente oculta — `.session-notice` es ahora la receta
  `position:absolute/1px/clip-path:inset(50%)`, sin `display:none`, y el
  elemento vive SIEMPRE en el árbol (sin `class:hidden`) porque una región viva
  tiene que existir antes de que su texto cambie. Por qué así y no «visible
  dentro del panel»: el `#sessions-panel` arranca cerrado
  (`aria-expanded="false"`, `plan.js:1152-1508`) y su `display:none` no
  anunciaba nada y no pintaba nada — el aviso era mudo y ciego en el estado por
  defecto. La string es la C29 tal cual (`sessionEndedNotice`), y el anuncio ya
  no depende de que el panel esté abierto. (3) `Escape` cierra y devuelve el
  foco al toggle en `PlanPanel` y `SessionsPanel`, copiando el patrón de
  `SettingsPanel` (`$effect.pre` que lee si el foco estaba adentro *antes* de
  esconder, `$effect` que lo devuelve). (4) **Decisión**: se quita
  `role="dialog"` de `#settings-panel` (queda `role="group"` + `aria-label`, y
  la gramática de disclosure en el engranaje: `aria-expanded` +
  `aria-controls`). Se rechaza el trap: el panel NO es modal — flota sobre un
  plano que el humano sigue usando (un slider lo repinta en vivo) y el panel de
  diagramas y el switcher siguen alcanzables, así que atrapar el Tab adentro
  sería una trampa de teclado en un panel que no bloquea nada; `role="dialog"`
  prometía una modalidad que el panel nunca tuvo. `tabindex="-1"` del panel
  cayó con el rol. (5) `ul#panel-list`: roving tabindex (un solo tab stop, el
  de la fila con foco, si no el del diagrama elegido, si no el primero) e
  ↑/↓/Home/End que mueven el foco; el listener va en la fila (`.panel-item-open`)
  y no en el `<ul>`, que no es un elemento interactivo. (6) Los `←`/`→` del
  historial respetan `event.defaultPrevented` y `[contenteditable]`
  (`:not([contenteditable="false"])`, con `closest` en vez de `matches`): un
  evento que otro manejador ya consumió no es el evento del historial, y donde
  el humano escribe nadie le roba las flechas. (7) `.diagram` es focusable
  (`role="group"` + `aria-label={boxText(rec)}` + `tabindex="0"`, el foco
  escribe `selectedId`): eso es lo que vuelve alcanzable por teclado el
  historial por flechas — antes había que clonar/arrastrar o elegir la fila del
  panel para seleccionar un objeto, y el `×` y la píldora del clon ya se
  revelan por foco (el `×` con su propio `:focus-visible`, la píldora con
  `.diagram.clone:focus-within`, que ahora también se cumple al enfocar el
  objeto). El plano declara `overflow: clip` además de
  `hidden` para que ningún navegador desplace el plano para "mostrar" el objeto
  enfocado (medido en Chromium: ni `hidden` ni `clip` desplazan `#plan` con un
  objeto fuera de pantalla enfocado; `clip` lo hace estructural y cae a
  `hidden` donde no exista). (8) `prefers-reduced-motion`: `panTo` pregunta
  `matchMedia` en cada llamada (el ajuste del sistema puede cambiar con la
  página abierta) y aterriza la cámara en el frame — mismo destino, zoom
  intacto (C12) —; los dos fades del chrome (`transition: opacity` de
  `.diagram-close` y `.diagram-clone-tools`) se apagan en un
  `@media (prefers-reduced-motion: reduce)`. (9) `bind:offsetWidth` en
  `CloneTools`: la píldora publica su ancho como valor reactivo y el `$effect`
  ya no mide el DOM ni mantiene la lista `void width/hidden/count/…`; el ancho
  del objeto sale del prop `width` (para un clon su rectángulo **es** el
  objeto), así que la decisión es `pillWidth > width`. (10) `setClonePlace`
  escribe `rec.rect.x/y` en el lugar y `placeCloneContent` escribe los cuatro
  campos de `rec.content` en el lugar: el spread por `pointermove` asignaba un
  rectángulo nuevo (y un content nuevo) sesenta veces por segundo de arrastre.
  (11) `$derived.by` con el índice `session_id → count` (`diagramCounts`):
  `sessionDiagramCount` era un `filter` sobre todos los diagramas y lo llamaba
  una vez por fila del switcher más la del plano en pantalla.
  Evidencia (medida, no argumento): el arnés de paridad es la suite de
  comportamiento y `tools/` no está en la superficie de edición de esta tarea
  (la paridad se queda en 57), así que la verificación funcional se corrió con
  un arnés descartable FUERA del worktree (`/tmp`, Chromium headless sobre el
  bundle real, mismo `tools/lib/harness.mjs` y mismo puente falso que
  `parity.mjs`): 16 comprobaciones, **7/16 en el bundle base y 16/16 con el
  cambio**. Las rojas del base son exactamente los ítems: `.panel-item-version-step`
  sin regla (0 reglas), `role="dialog"` todavía puesto, 2 tab stops en
  `#panel-list` en vez de 1, `Escape` sin consumir en los dos paneles, el objeto
  sin `tabindex` (`null`), el notice todavía adentro de `#sessions-panel`, y el
  `pan` animando con `prefers-reduced-motion: reduce`
  (`early` −234.38/−191.21 vs `later` −904/−737.5). Verde con el cambio:
  `role="group"` + `aria-label="ajustes del plano"`; un solo tab stop y
  ↑/↓/Home/End moviendo el foco con el tabindex siguiéndolo; `Escape` cerrando
  y devolviendo el foco a `#panel-toggle` y a `#sessions-toggle`;
  `.diagram[tabindex="0"]` cuyo foco marca la fila del panel
  (`aria-current="true"`) y `←` pide `graphium://version-view
  {diagram_id:"d-1", version:6}`; los tres guards (control positivo: el handler
  global SÍ emite antes de los guards, y no emite con `[contenteditable]`, ni
  con el evento ya consumido, ni desde `#settings-grid-opacity`; en el base ese
  bloque no podía fallar porque sin `tabindex` no había selección y el handler
  estaba inerte, por eso lleva el control positivo); el notice con caja 1×1,
  `role="status"`, `display` distinto de `none` y el texto exacto de C29; con
  emulación de menos movimiento la cámara ya está en el destino a los 30 ms
  (356/142.5 en los dos muestreos) y sin la emulación vuelve a animar. Los tres
  ítems de rendimiento son PRESERVE y se compararon contra el bundle base con
  la misma entrada: `overhang` de la píldora (`false`, 140 px de píldora contra
  368 px de objeto — igual que el reporte `C31` de la paridad), el arrastre de
  un clon (40/20 px de pantalla → `{x:640,y:420}` en `graphium://clone-move` y
  `left:640px; top:420px` en el objeto — idéntico al base) y los conteos del
  switcher (`Mi plano:1`, `graphium-1:2`, `sesión dos:1`)
  Gates (códigos reales, todos en primer plano): `npm run build` exit 0 (sin
  warnings de a11y de Svelte; los cuatro `svelte-ignore` —Escape en los dos
  asides y en el panel de ajustes, `tabindex` del objeto— están justificados en
  el markup), `node tools/parity.mjs` 57/57 exit 0, `DISPLAY=:0
  scripts/smoke.sh` → `rendered=true reason=none` exit 0 (con `cargo build -p
  gx-app` exit 0 antes, que embebe `frontend/dist`), `cargo test --workspace`
  37 suites / 0 failed, `cargo fmt --check` exit 0, `cargo clippy --workspace`
  exit 0. Sin commit y sin push: los hace el orquestador (S4). Lo que NO se
  tocó: payloads/throttles de `CONTRACT.md`, los números 59/45°/41.72/24/3/0.4/
  zoom 0.05-4.0, ninguna string C41 (la del notice incluida), y los ítems de T4
  (wheel template test, pinch, `getCoalescedEvents`) siguen fuera.
