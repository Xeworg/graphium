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

## Posterior (2026-11-12): la extensión que se instala dice qué versión es

Tres defectos aparecieron al instalar la extensión nueva de `f28-lenguajes` (la
que registra el servidor con `exposure: "direct"`) en la máquina del dueño:

1. **El release publicaba la versión del checkout, no la del release.**
   `scripts/package-release.sh` copiaba `extensions/pi/graphium` tal cual al
   tarball, así que cada release viajaba con `0.1.0` y el instalador mostraba esa
   cifra al lado de un binario más nuevo. Ahora el empaquetador estampa la versión
   del release en el `package.json` del tarball **y verifica el estampado**: un
   `sed` que no matchee tiene que romper el empaquetado, no publicar la versión
   equivocada. La extensión del árbol declara además la versión del producto
   (`0.2.0`), para que una instalación desde el repositorio no mienta.
2. **`pi` y `gentle-shell` pueden resolver al mismo directorio.** `pi` sigue a
   `PI_CODING_AGENT_DIR`, y una sesión de gentle-shell la apunta justo al agente
   de gentle-shell. Con `--all`, el instalador copiaba dos veces al mismo destino
   y decía «instalado» dos veces. La instalación se deduplica por destino y el
   segundo host lo dice: «mismo destino que `pi` (...); se instala una vez».
3. **La extensión servía un build viejo por culpa del cwd.** `findBinary` subía
   desde el cwd antes de mirar el `PATH`, así que una extensión instalada corriendo
   dentro de un checkout con un build viejo servía ese build viejo: el dueño
   recargó Pi y Graphium siguió usando un binario de 45.925.016 B en vez del
   instalado de 46.207.640 B. El orden pasó a `GRAPHIUM_BIN` → el checkout de la
   propia extensión → `PATH` → cwd al final, con la razón escrita en el código y
   en el `README`: el cwd es el proyecto del humano, no el checkout de Graphium.

Pruebas: `tests/installer/extension_package_test.sh` (el tarball lleva la versión
pedida, con una versión sintética para que el test no pueda pasar sin estampar),
`tests/installer/agents_same_dir_test.sh` (una sola copia cuando dos hosts
coinciden) y el check 23 de `extensions/pi/graphium/verify.mjs` (`findBinary`
prefiere el `PATH` antes que un build del cwd). Los tres se vieron rojos antes del
arreglo.

**Entrega:** este corte viaja con `f28-lenguajes` y `f29-componente` en el mismo
PR `staging` → `main`; las slices son los commits, y este es uno solo porque los
tres defectos se arreglan en el mismo camino (empaquetar, elegir destino, elegir
binario) y ninguno se puede probar sin los otros dos en su lugar.

### Posterior (2026-11-12, segunda tanda): el camino publicado se puede probar sin publicar

El dueño quiso ver el instalador `curl | bash` funcionando **antes** del PR. Dos
cosas lo impedían:

4. **`--version` / `GRAPHIUM_VERSION` se parseaba y se tiraba.** `main` lo leía
   para `install`, y después `install_binary` llamaba
   `install_binary_from_release ""`: la descarga iba a `/latest/download` aunque
   el humano hubiera fijado una versión. La extensión tenía el mismo agujero en
   `install_extension_from_release`, que pasaba `""` a `download_verified`. Ahora
   hay **una** `RELEASE_VERSION` que fijan `install --version`,
   `agent --version` y `GRAPHIUM_VERSION`, y la usan los dos caminos: el binario
   y la extensión son el mismo release, y bajarlos de versiones distintas deja un
   agente que no sabe con qué binario habla. `update` además baja **la versión que
   anunció**, no `/latest`, porque entre el chequeo y la descarga `/latest` puede
   moverse.
5. **No había forma de apuntar el instalador a otro lado.** `RELEASES_URL` era
   una constante; ahora es `GRAPHIUM_RELEASES_URL`, con el comentario que dice
   para qué existe: ejercitar el camino publicado contra un servidor local. El
   script no sabe la diferencia, así que la prueba es la misma que corre un
   usuario.

La prueba local completa, con los artefactos reales de `0.2.0` servidos por
`python3 -m http.server` y un `HOME` de scratch:

```
cat install.sh | env GRAPHIUM_RELEASES_URL=http://127.0.0.1:8731/releases \
  GRAPHIUM_VERSION=0.2.0 HOME=/tmp/f30-user bash
  → Bajando .../releases/download/v0.2.0/graphium-linux-x86_64.tar.gz
  → Instalado /tmp/f30-user/.local/bin/graphium            (46.207.640 B)

cat install.sh | env ... bash -s -- agent --env pi
  → Bajando .../releases/download/v0.2.0/graphium-pi-extension.tar.gz
  → Extensión instalada en /tmp/f30-user/.pi/agent/extensions  ("version": "0.2.0")
```

`tests/installer/release_version_test.sh` fija ese comportamiento de forma
hermética: arma un release falso bajo `file://` (binario, extensión y
`SHA256SUMS`) y exige que las dos descargas vayan a `download/v0.2.0` y **nunca**
a `latest/download`, además de probar `agent --version`. Se vio rojo revirtiendo
sólo el plumbing de la versión, y verde con el arreglo.
