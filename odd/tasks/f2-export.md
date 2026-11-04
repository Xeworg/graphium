# F2 — Export: SVG y PNG, inline y a archivo

Corte de F2 (`docs/PRD.md` §12): la tool `export_diagram` completa — el
**SVG** por MCP (inline, para pegarlo donde haga falta) y el **SVG o PNG**
escrito dentro de la raíz autorizada, con los límites de RF10-bis.

Criterio de salida (PRD §12, fila F2, verbatim): «H4, H5; suite completa de
validación/layout/render; RSS < 250 MB con 12 diagramas de 300 nodos».

## Specs

- **S1** (PRD §7 RF10, verbatim, luego del cambio de alcance L2/L8): «**RF10.** Tool `export_diagram(diagram_id?, format, opts)`, con `format: "svg" | "png"`:
  - `inline: true` → devuelve **texto por MCP**: el SVG. **Cero acceso a disco.** Camino primario para pegar donde haga falta.
  - `path: <relativo>` → escribe SOLO dentro de la raíz de exportes (`~/.graphium/exports/` o configurada): rechaza `..`, absolutos y symlinks; `png` rasteriza con `resvg`; sobrescribir exige `opts.overwrite: true`.
  - Nota de alcance (§21): **Mermaid y markdown salieron del alcance**, con RF10.1 (matriz de export por kind). No se genera DSL ajeno; el motor sigue siendo propio (**O6** — Mermaid nunca fue motor, así que O6 no cambia).»
- **S2** (PRD §7 RF10-bis, verbatim): «**RF10-bis — Límites de export (F2).** `inline` devuelve como máximo **1 MB** de texto; si el contenido excede → `ok: false, code: "too_large", size_bytes` con `hint: "usá path para escribir a archivo o opts.extract"`. `png` acepta `opts.scale` (default 1.0, techo 3.0) y `opts.max_px` (default 16 Mpx, techo 64 Mpx): si `ancho × alto × scale²` excede `max_px` → `too_large` con las dimensiones calculadas. Todo export tiene deadline de **5 s** → `export_timeout`. La raíz de exportes tiene tope de **200 MB** → `disk_quota_exceeded`.»
- **S3** (PRD §11, tabla de tools, verbatim): «| `export_diagram` | `diagram_id?`, `format: "svg"\|"png"`, `opts: { inline?: bool, path?: string, overwrite?: bool, scale?: number, max_px?: number }` | inline → `{ ok, content }`; archivo → `{ ok, path }` (solo raíz autorizada) |»
- **S4** (PRD §11.1, verbatim): «`export_diagram` sin `diagram_id`: si hay exactamente 1 abierto en la sesión → ese; si hay 0 → `unknown_diagram`; si hay ≥ 2 → `ambiguous_diagram` con `candidates[]`.» y «`get_diagram`/`update_diagram`/`export_diagram` sobre un diagrama de otra sesión → `not_owner` con `hint: "ese diagrama pertenece a otra sesión MCP"`.»
- **S5** (PRD §5, verbatim): «**H4 — Exportación inline (F2):**
  El humano pide pegar el diagrama en el README del repo. El agente llama `export_diagram(format: "svg", inline: true)` y recibe el texto (el SVG) por el canal MCP — sin escribir un archivo. Para archivo: `export_diagram(format: "svg", path: "arquitectura.svg")` escribe solo dentro de la raíz de exportes autorizada.»
- **S6** (PRD §8.5, verbatim): «- **Export:** `svg` = el string; `png` = `resvg`, ambos con la paleta resuelta. Sin DSL ajeno: Mermaid/markdown están fuera del alcance (§21).»
- **S7** (PRD §8.5.2, verbatim): «- El export `svg` incluye la paleta resuelta; el export `png` rasteriza con esa paleta.»
- **S8** (PRD §7 RNF4, verbatim): «**Seguridad:** sin ejecución de código del JSON, sin network externa; **escritura de Graphium limitada a `~/.graphium/`** (`settings.json` + `exports/`); **la IA solo escribe dentro de `~/.graphium/exports/`** (RF10); MCP stdio solamente.»
- **S9** (PRD §7, criterio 14, verbatim): «**no** afecta al SVG exportado (el export sale sin grilla).»
- **S10** (PRD §8.5.1, verbatim): «- **Fuera del pipeline determinista** (nunca comparados por hash): cámara (pan/zoom), grilla y sus colores, panel lateral, banner de error, geometría de ventana, `--dump-last` y **PNG** (depende de la versión de `resvg`).»
- **S11** (PRD §7 RNF1-quater, verbatim): «Presupuesto desagregado del binario en release Linux: app + Tauri **6 MB**, motores de layout **1.5 MB**, `gx-render-svg` **1.5 MB**, fuentes embebidas **3 MB**, `resvg`/PNG **1.5 MB**, margen **1.5 MB** = **15 MB**. […] el build falla si supera **15 700 000 bytes**.»
- **S12** (PRD §7, código estable, verbatim): «**Códigos de error estables** (se agregan al enum de RF5): `unknown_kind`, `unknown_diagram`, `diagram_closed`, `too_many_diagrams`, `too_many_sessions`, `not_owner`, `ambiguous_diagram`, `busy`, `owner_unavailable`, `layout_timeout`, `export_timeout`, `export_unsupported_kind`, `export_path_outside_root`, `export_exists`, `disk_quota_exceeded`, `invalid_patch`, `kind_change_not_allowed`, `stale_revision`, `deprecated_kind`.»

### Desvío autorizado (el humano, 2026-11-03)

El humano cambió el alcance **después** de escrito este documento: **Mermaid
sale del corte** («había pedido que mermaid no» → «Sacar Mermaid del corte»).
Consecuencias, todas aplicadas:

- Los formatos del export quedan **`svg` y `png`**. No hay `mermaid` ni
  `markdown`: sin Mermaid no hay bloque mermaid que embeber en markdown, y un
  markdown con un SVG embebido no lo renderiza GitHub.
- **RF10.1 (la matriz de export por kind) sale del alcance**: sin Mermaid no
  hay dialecto que un kind declare. El campo `mermaid_export` del índice de
  `get_diagram_schema` **se borra**: hoy promete una capacidad que el producto
  no tiene (dice «nativa» para `graph` sin que exista export a Mermaid).
- El motor sigue siendo propio (O6 se mantiene): esto quita un formato de
  interoperabilidad, **no** mete un motor ajeno.

## Tasks

| ID | Tarea | Specs | Ruta | Commit |
|---|---|---|---|---|
| T1 | Feature doc del corte + mirror + lista de tareas | S1-S12 | inline | `770f4f7` |
| T2 | `gx-core`: módulo `export` con el sandbox (ruta relativa, sin `..`, sin symlink), los límites (1 MB inline, 5 s, 200 MB de raíz, escala y techo de píxeles) y el seam del rasterizador | S2, S8, S12 | inline | `3877d05` |
| T3 | El índice de kinds deja de prometer `mermaid_export`; el PRD queda coherente (v1.8, §21) | S6 | inline | `881dd73` |
| T4 | `png` con `resvg`: rasteriza el SVG con la paleta resuelta, con `opts.scale` y `opts.max_px`; **medición del presupuesto de binario** (RNF1-quater: 6 760 912 de 15 700 000) | S2, S7, S10, S11 | inline | este PR |
| T5 | Tool `export_diagram`: `svg` inline (tope de 1 MB, deadline de 5 s), `svg`/`png` a archivo (sandbox, `opts.overwrite`, cuota de la raíz), `diagram_id` opcional (`unknown_diagram` / `ambiguous_diagram` / `not_owner`) | S1, S2, S3, S4, S5 | inline | este PR |
| T6 | Cierre: descripción de la tool para el agente, relay, `README`/`QUICKSTART`, suite completa en verde, clippy y fmt limpios | S3, S5 | inline | **pendiente** (rama `feature/f2-export-t6`, sin empezar) |

## Log

- **L1** (2026-11-03, arranque del corte): el humano, consultado sobre el orden de lo que falta para cerrar F2, eligió el export primero: «el export primero». El corte implementa RF10 y RF10-bis sobre el documento vigente del PRD (v1.7).

- **L2** (2026-11-03, el humano cambia el alcance): a mitad de camino, al ver el plan, dijo «había pedido que mermaid no» y confirmó «Sacar Mermaid del corte». Ya estaba escrito en el PRD (§7 RF10, nota de identidad: «Mermaid aparece solo como formato de export de texto […] nunca como motor»), pero el humano lo sacó **entero**: no se genera Mermaid ni markdown. El corte queda con `svg` y `png`, y RF10.1 (la matriz de export por kind) sale con él. El motivo de fondo —el motor es propio y Mermaid no entra como motor— sigue vigente; lo que se cae es la interoperabilidad con GitHub.

- **L3** (2026-11-03, el riesgo que decide el orden): el PNG es lo único del corte que **agrega una dependencia** (`resvg`) y lo único que puede romper el **presupuesto de binario** de RNF1-quater (15 700 000 bytes; el binario de hoy mide 5 814 680 bytes). El presupuesto del PRD le reserva 1,5 MB a «`resvg`/PNG», así que la medición va **dentro** del corte (T4), no después: si el binario no entra, se decide con números y no con una sorpresa en el CI.

- **L4** (2026-11-03, dónde vive el sandbox): la política de escritura vive en `gx-core::export`, no en `gx-mcp`: son reglas (ruta relativa, sin `..`, sin symlink, cuotas) que se prueban sin MCP ni ventana. `gx-mcp` decide qué diagrama se exporta y cómo se responde; `gx-core` decide **dónde** pueden caer los bytes. La raíz es `~/.graphium/exports/` y se mueve con `GRAPHIUM_EXPORTS_ROOT` (tests y corridas sin ventana); **el agente nunca la elige**.

- **L5** (2026-11-03, el tema del export): el `svg` exportado lleva la paleta resuelta (S7) para que se vea igual fuera de la app. Se exporta con la paleta oscura (`plano`): el tema que eligió el humano vive en `~/.graphium/settings.json` y pertenece a `gx-app`, y `gx-mcp` no puede depender de él sin romper la matriz de dependencias (§9.1). Queda anotado como límite conocido del corte, no como error.

- **L6** (2026-11-03, el PNG y las fuentes): el SVG lleva `font-family:Inter` y el texto como `<text>`, así que el PNG necesita la misma fuente que el resto del dibujo: la Inter embebida, no la del sistema. Rasterizar con la fuente del sistema daría un PNG que no se parece al SVG, que es justo lo que el export promete. El PNG queda **fuera del pipeline determinista** (S10): sus bytes dependen de la versión de `resvg` y no se comparan por hash.

- **L7** (2026-11-03, el tema del export, revisado sobre L5): L5 había anotado como «límite conocido» que el export sale con la paleta oscura aunque el humano esté mirando otro tema. Ese límite se resuelve **sin** romper la matriz de dependencias (§9.1): `gx-core::export` guarda el tema del plano en un slot (`install_theme` / `plan_theme`, oscuro por defecto), igual que ya hace con los motores y el renderer. `gx-app` instala el tema con el que está pintando y `gx-mcp` lo lee al exportar; `gx-mcp` sigue **sin** depender de `gx-app`, porque el valor viaja por `gx-core`, que es la base de los dos. Si nadie instala nada, el export sale oscuro, que es exactamente lo que L5 describía. L5 queda como historia; lo que manda es L7.

- **L8** (2026-11-03, T3: el PRD se puso coherente): sacar Mermaid del corte dejaba al PRD prometiendo lo que ya no se iba a hacer: `format: "markdown" | "mermaid"` (RF10, §5 H4, §11 tabla), la matriz RF10.1 y el campo `mermaid_export` del índice de `get_diagram_schema`. El PRD sube a **v1.8** (§21) y esas líneas se reescriben a `svg`/`png`; el campo sale del índice en `gx-mcp`. S1, S3, S5 y S6 quedaron reescritas con el texto nuevo, verbatim. La prueba de T3 no es «el campo no está» sino que la entrada serializada del índice tenga **exactamente** las seis claves que el producto cumple (`disponible`, `engine`, `kind`, `limits`, `min_example`, `phase`): un índice que promete una capacidad inexistente es una mentira que el agente no tiene cómo verificar.

**Slice 1** de este corte (rama `feature/f2-export`): T1 `770f4f7` + T2 `3877d05` + T3 `881dd73` — unas 530 líneas, de las cuales 433 son `gx-core::export`. T4 (`resvg` + presupuesto de binario), T5 (tool `export_diagram`) y T6 (cierre) van en PRs propios, cada uno con su medición.

- **L9** (2026-11-03, el PNG entra y no se lleva el presupuesto): medido con `resvg` dentro del binario real (release, `lto`, `strip`): **6 760 912 bytes** contra el techo de **15 700 000** (antes de este corte: 5 814 680). `resvg` + `usvg` + `tiny-skia` suman **946 232 bytes ≈ 0,9 MB**, dentro de los 1,5 MB que RNF1-quater le reserva a «`resvg`/PNG». Decisiones: (1) se usa **`resvg` 0.45.1**, no 0.49, porque 0.49 pide rustc **1.85** y el workspace declara MSRV **1.82**; subir el MSRV es una promesa al que compila, no un detalle de una dependencia; (2) `default-features = false, features = ["text"]`: sin `system-fonts` (las fuentes son las embebidas, L6), sin `memmap-fonts` y sin `raster-images` (nada lee imágenes del documento) — cada feature que no se usa es binario que se paga; (3) el rasterizador vive en `gx-render-svg` detrás de la feature `png`, que solo enciende `gx-app`: un build sin ventana no paga el rasterizador; (4) los techos (escala 0 < 3, `max_px` ≤ 64 Mpx) se aplican **dentro** del rasterizador además de en los `opts`: quien arme `RasterOptions` a mano no puede saltearlos.

- **L10** (2026-11-03, el export necesita fondo): el SVG del plano es **transparente a propósito** — el objeto flota sobre la grilla y por eso `.bg` era una regla CSS sin rectángulo. Un archivo exportado así no se ve: el texto claro desaparece en una página blanca. Se agregó `SvgRenderer::render_export`, que pinta el rectángulo del lienzo con la paleta y deja **todo lo demás byte a byte igual**; el plano sigue saliendo sin fondo, así que ningún golden cambia. La prueba no es «hay un rect»: es que el rect cubra el `viewBox` completo (`only_the_export_paints_the_canvas`), porque un fondo que no llega al borde deja una franja transparente.

**Slice 2** (rama `feature/f2-export-t4`): T4 en un solo commit — `resvg` tras la feature `png`, el rasterizador, `render_export` con el fondo del lienzo, el cableado en `gx-app` y la medición de RNF1-quater. 418 líneas: es una unidad de comportamiento (el PNG), no tres.

- **L11** (2026-11-03, la tool y sus decisiones de contrato): `export_diagram` vive en `gx-mcp/src/export.rs` y no toca el plano: lee el documento guardado, lo vuelve a pintar con `render_json` (`export: true`, tema de `plan_theme()`) y responde. Reglas que quedaron fijas: `inline` y `path` son **excluyentes** (pedir los dos es un error, no una precedencia silenciosa); `png` **sin** `path` es `invalid_value` porque son bytes y no viajan como texto; `svg` sin `path` ni `inline` viaja inline, que es el camino primario de H4. El render, la rasterización y la escritura corren en un hilo bajo el deadline de 5 s (`export_timeout`), y **nada se escribe hasta que todo lo demás salió bien**: un export rechazado deja la raíz intacta (lo pinea un test por caso). Los candidatos de `ambiguous_diagram` viajan en `message`/`hint`, igual que los `known_ids` de `unknown_diagram`: el `ErrorPayload` de la casa no tiene campos extra y agregarlos a un solo código sería peor que seguirlo.

- **L12** (2026-11-03, un hallazgo del camino): el fondo del export es un método **del trait** `SvgRenderer` (`render_export`), con default en `gx-core` para que los renderers de prueba sigan compilando, y lo pinta el renderer real. Al borrar el método inherente que tenía `gx-render-svg`, el módulo de tests de `png.rs` dejó de compilar (E0599: el trait no estaba en alcance) y **el workspace entero se puso rojo**, aunque `png` no es feature por defecto: `gx-app` la enciende y la unificación de features compila ese módulo de tests igual. Se arregló con el `use` del trait y, además, los tests del PNG ahora llaman el método **por el trait** (`SvgRendererContract::render_export`), que es lo que hace el motor — probar la inherente probaba el camino que ya no existe.

**Slice 3** (rama `feature/f2-export-t5`): primero `d069f20` — el fondo del export como método del trait, con el `export: bool` de `RenderOpts` (92 líneas, sin cambio visible: `export` viene apagado); después la tool, el end-to-end real con `resvg` y estos apuntes del log. Dos commits para que la primera mitad se pueda revisar sin leer 1 100 líneas de tests.
