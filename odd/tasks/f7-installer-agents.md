# F7 — instalador fácil: elección de agentes con marcado (TUI)

Instalar la extensión de Graphium en un agente sigue siendo un paso manual.
Este corte hace que el instalador pregunte qué agentes instalar, con marcado
directo, y que la lista de agentes sea un registro extensible.

## Specs

- S1 (pedido textual): «el instalador para el usuario sigue siendo manual el
  instalador debe ser facil para el usuario que el elija que agentes va a
  instalar».
- S2 (pedido textual): «si es necesario hacer una tui seria mejor hacerlo o que
  el escript te permita marcar los agentes».
- S3 (pedido textual): «de momento solo tenemos dos pero vamos a ir agregando».
- S4 (restricción del humano): «de esos worktrees los agentes no pueden hacer
  merge de la rama lo haremos luego de revisar y probar nosotros» — el worker no
  commitea ni merguea; deja el árbol listo para revisión. Integración (decisión
  del 2026-11-05): feature → `staging` (revisión y prueba ahí) → PRs encadenados
  → `main`; nunca directo a `main`.

### Derivados

- R1: registro único de agentes: una sola tabla (nombre, variable de entorno,
  directorio por defecto) de la que salen detección, status, selección e
  instalación. Hoy hay líneas hardcodeadas (`agent_env_status pi` /
  `gentle-shell` dentro de `select_agents`); deben iterar el registro. Agregar
  un agente nuevo tiene que ser una entrada en la tabla.
- R2: selección interactiva con marcado: en una TTY interactiva, un selector
  tipo checklist (flechas para moverse, espacio marca/desmarca, Enter
  confirma, `a` = todos, `q`/Esc = cancelar sin instalar). Estado inicial:
  marcados los hosts detectados en la máquina.
- R3: fallback: si la TTY no soporta el selector (sin terminfo/`tput`,
  terminal tonta) se usa el prompt numérico actual. Sin terminal, el error
  actual con `--env`/`--all` se mantiene.
- R4: no interactivo intacto: `agent --env <name> ...` y `agent --all` siguen
  funcionando igual, y los scripts de test no quedan colgados de la TUI.
- R5: `./install.sh` (instalación completa) termina ofreciendo la elección de
  agentes con el selector (R2), con opción clara de saltear; hoy solo imprime
  «Siguiente paso: ./install.sh agent».
- R6: el resultado de la selección se muestra antes de instalar (qué agentes
  quedaron marcados) y lo que se instala/salta se reporta como hoy.
- R7: tests: la suite de `tests/installer/` sigue verde; sumar al menos un test
  del fallback no interactivo y uno de que el registro y la detección
  coinciden. Los tests corren contra HOME de scratch como hoy
  (`env -u PI_CODING_AGENT_DIR`).
- R8: `usage()` y los mensajes de ayuda reflejan el nuevo flujo; docs
  (README/PRD si mencionan el instalador) actualizadas.

### Verificación

- R9: `bash scripts/test-installer.sh` verde (compila release en
  `tmp/installer-target`). El selector interactivo se prueba a mano: el humano
  lo revisa en vivo antes del merge (S4): no commitear, no merguear.

## Tasks

- T1 (R1): registro único de agentes; eliminar los hardcodeos. Commit: `6c609fd`.
- T2 (R2, R3): selector checklist con fallback numérico. Commit: `6c609fd`.
- T3 (R5, R6): `install` ofrece la elección al final, con resumen previo. Commit: `6c609fd`.
- T4 (R7): tests nuevos + suite verde. Commit: `170c6e2`. Evidencia: suite 7/7 verde (agents_test, agent_prompt_test nuevos); shellcheck no disponible en esta máquina; hallazgo del worker: fijar CARGO_HOME real baja el suite de ~13 min a ~5 s.
- T5 (R8): ayuda y docs; resumen para revisión humana. Commit: `27c29e9`. La prueba manual de la TUI queda para el humano (R9/S4).

## Log

- L1 (2026-11-05): pedido original del humano: «luego lo segundo que podemos
  trabajar es que el instalador para el usuario sigue siendo manual el
  instalador debe ser facil para el usuario que el elija que agentes va a
  instalar, si es necesario hacer una tui seria mejor hacerlo o que el escript
  te permita marcar los agentes de momento solo tenemos dos pero vamos a ir
  agregando».
- L2 (2026-11-05): el humano fijó el flujo: «de esos worktrees los agentes no
  pueden hacer merge de la rama lo haremos luego de revisar y probar nosotros».
- L3 (2026-11-05): el humano fijó la integración: feature → `staging` →
  PRs encadenados → `main`; nunca directo a `main`.
- L4 (2026-11-05): implementación completa del worker (sin commit); el
  orquestador la empaquetó en tres unidades: instalador (`6c609fd`), tests
  (`170c6e2`), docs (`27c29e9`). Pendiente abierto: la nota de `ln -s` manual
  en docs/QUICKSTART.md quedó fuera de la superficie del worker; revisar en la
  integración.
