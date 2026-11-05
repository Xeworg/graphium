# F6 — temas completos: todos los colores configurables + 4 temas nuevos

El look del plano hoy solo permite cambiar fondo y grilla; el resto de los
colores (nodos, aristas, acento, UI) vienen fijos del tema elegido. Este
corte abre todos los colores y agrega cuatro temas nuevos con otros nombres.

## Specs

- S1 (pedido textual): «que hagas temas donde se cambie todo el fondo y los colores».
- S2 (pedido textual): «ya que actualmente tiene dos temas claro y oscuro y se le
  puede cambiar el fondo por aparte, entonces seria mejor poder modificar esas
  opciones que no se pueden modificar».
- S3 (pedido textual): «crea un tema inspirado en opencode, uno en claude, uno en
  pi y uno en gentle ai dales otro nombre».
- S4 (restricción del humano): «de esos worktrees los agentes no pueden hacer
  merge de la rama lo haremos luego de revisar y probar nosotros» — el worker no
  commitea ni merguea; deja el árbol listo para revisión.

### Derivados de S1/S2

- R1: el tema define la paleta completa: fondo, grilla, relleno de nodo, borde
  de nodo, texto de nodo, arista, texto de arista, acento y los tokens de UI
  del panel (hoy `[data-theme]` en `style.css`).
- R2: todo color de la R1 es modificable desde el panel de ajustes, incluidos
  los que hoy no se pueden tocar (colores de nodo, arista y acento). Los
  pickers de color siguen la convención existente de fondo y grilla.
- R3: un color modificado a mano queda guardado en `settings.json` y sobrevive
  al reinicio; el tema elegido sigue siendo la base (elegir otro tema resetea
  los overrides a su paleta).
- R4: un color inválido en `settings.json` cae al valor del tema (política
  existente de `settings.rs`: un typo no tira la ventana).

### Derivados de S3

- R5: cuatro temas nuevos, con nombres que no son de marca (la UI va en
  minúsculas, como `plano` y `claro`):
  - `brasa` — inspirado en opencode: oscuro tipo terminal, acento ámbar/naranja.
  - `arcilla` — inspirado en Claude: crema cálido, acento terracota.
  - `aurora` — inspirado en Pi: fondo azul profundo, acento violeta/celeste.
  - `neblina` — inspirado en Gentle AI: gris suave de baja saturación, acento
    verde azulado.
  Los valores exactos los decide el implementador cuidando contraste legible
  (texto de nodo vs relleno, arista vs fondo) en cada tema.
- R6: arquitectura: extender `Theme` en `crates/gx-core/src/render.rs` con los
  cuatro variantes (serde snake_case), su `Palette` en `Palette::for_theme`,
  los presets en `crates/gx-app/src/settings.rs`, la lista de radios del panel
  (`index.html`/`plan.js`) y los tokens `[data-theme="…"]` de `style.css`.

### Contrato de export

- R7: `export_diagram` dibuja con la paleta efectiva (tema + overrides del
  humano). Hoy solo viaja el `Theme`; agregar el override como campo opcional
  de `RenderOpts` (aditivo, no rompe consumidores).

### Verificación

- R8: `settings_panel.rs` (contrato Rust↔JS de settings) cubre los campos
  nuevos; cada tema nuevo tiene al menos un test de paleta (colores distintos
  entre sí y contraste texto/relleno); los tests existentes siguen verdes.
- R9: verificación de la UI es `node --check plan.js` + tests de contrato Rust
  + ojo humano (no hay harness JS). El humano revisa en vivo antes del merge
  (S4): no commitear, no merguear.

## Tasks

- T1 (R6, R5): `Theme` + `Palette` de los cuatro temas nuevos en gx-core. Commit: pendiente.
- T2 (R1, R2, R3, R4): overrides de color en `Settings` + pickers del panel + persistencia. Commit: pendiente.
- T3 (R6): presets, radios del panel y tokens CSS de UI para los seis temas. Commit: pendiente.
- T4 (R7): paleta efectiva en el export (RenderOpts aditivo). Commit: pendiente.
- T5 (R8): tests de contrato y de paleta; suite verde. Commit: pendiente.
- T6 (R9): `node --check`, docs (PRD RF22/RF25), resumen para revisión humana. Commit: pendiente.

## Log

- L1 (2026-11-05): pedido original del humano: «bien hay dos cosas que quiero
  trabajar, para eso podemos usar worktrees y agentes, quiero, que hagas temas
  donde se cambie todo el fondo y los colores, ya que actualmente tiene dos
  temas claro y oscuro y se le puede cambiar el fondo por aparte, entonces
  seria mejor poder modificar esas opciones que no se pueden modificar, y crea
  un tema inspirado en opencode, uno en claude, uno en pi y uno en gentle ai
  dales otro nombre».
- L2 (2026-11-05): el humano fijó el flujo: «de esos worktrees los agentes no
  pueden hacer merge de la rama lo haremos luego de revisar y probar nosotros».
