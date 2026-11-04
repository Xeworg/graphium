# f4-instalador

Instalador web estilo mnexis para Graphium: binario al PATH, extensión del
agente, `update`, uninstall, y workflow de release que despierta con el
primer tag (el tag sigue siendo decisión del humano).

## Specs

- S1: «debemos hacer un instalador web con github como este curl -fsSL
  https://github.com/Xeworg/mnexis/releases/latest/download/install.sh | bash
  pero para eso debemos hacerlo primero aca»
- S2: «debe tener una opcion que permita instalarlo tambien en el agente»
- S3: «dejar de una vez la app en el path»
- S4: «tambien quiero que se pueda actualizar luego asi que luego del primer
  tag debemos hacer que tenga la opcion de actualizarse con el mismo enlace
  que verifique si hay una version nueva»
- S5: elección del humano ante «¿El instalador incluye ya la parte de release,
  o solo lo que funciona sin tag?» → «la opcion 2» (workflow de release
  incluido; el tag sigue siendo su decisión).
- S6: «es el script tambien debe permitir a la persona seleccionar los
  agentes que este usara, que se abra detecte si debe acatualizarse y detecte
  si esta instalado en algun agente y permita al usuario actualizarlo o
  instalar nuevos» — el instalador detecta los hosts de agente presentes,
  muestra dónde ya está instalada la extensión, deja elegir en cuáles
  instalar o actualizar, y al actualizar se ocupa de los agentes que ya la
  tienen.

## Tasks

- T1 (S1, S3): `install.sh` — modo repo (compila release, instala
  `~/.local/bin/graphium`) y modo standalone pipeado (baja el tarball del
  release y verifica SHA256SUMS); `--help`. [commit: 10dfa20]
- T2 (S2, S6): `install.sh agent` — detecta los hosts Pi presentes (pi y
  gentle-shell), muestra el estado de la extensión en cada uno (versión
  instalada o «no instalada») y deja elegir cuáles instalar o actualizar:
  interactivo en una tty, `--env <name>` / `--all` sin tty. Funciona en modo
  repo y en modo release (la extensión viaja como asset del release).
  [commit: 10dfa20]
- T3 (S4, S6): `install.sh update` — consulta `releases/latest`, compara con
  la versión instalada y reinstala solo si hay una versión nueva; después
  refresca la extensión en cada agente que ya la tiene instalada. Antes del
  primer tag responde con un mensaje claro. `install` sobre una versión vieja
  anuncia la actualización. [commit: 10dfa20]
- T4 (S1, S6): `install.sh uninstall` — saca el binario y la extensión de
  cada agente que la tiene; los datos (`~/.graphium`) se conservan.
  [commit: 10dfa20]
- T5 (S5): `scripts/package-release.sh` + `.github/workflows/release.yml`
  (trigger `v*`): gates + build x86_64/aarch64 + publish con binarios,
  extensión, `install.sh` y SHA256SUMS. [commit: eb478f9]
- T6 (S1-S4): suite de tests del instalador (`scripts/test-installer.sh` +
  `tests/installer/`) contra HOME de scratch; el CI de release la corre.
  [commit: eb478f9]
- T7 (S1-S4): docs (README raíz y README de la extensión) y verificación
  local completa. [commit: 10dfa20]

## Log

- L1: «por donde quedamos» → diagnóstico: la extensión instalada en
  `~/.gentle-shell/agent/extensions/graphium` no encuentra el binario (no hay
  checkout encima ni `graphium` en el PATH): «en la otra sesion me sale esto
  Graphium: no encontré el binario. Definí GRAPHIUM_BIN o compilá con `cargo
  build --release`.»
- L2: «espera» (pausa; se retoma después).
- L3: «debemos hacer un instalador web con github como este curl -fsSL
  https://github.com/Xeworg/mnexis/releases/latest/download/install.sh | bash
  pero para eso debemos hacerlo primero aca, debe tener una opcion que permita
  instalarlo tambien en el agente y dejar de una vez la app en el path»
- L4: «la opcion 2 pero tambien quiero que se pueda actualizar luego asi que
  luego del primer tag debemos hacer que tenga la opcion de actualizarse con
  el mismo enlace que verifique si hay una version nueva»
- L5: «es el script tambien debe permitir a la persona seleccionar los
  agentes que este usara, que se abra detecte si debe acatualizarse y detecte
  si esta instalado en algun agente y permita al usuario actualizarlo o
  instalar nuevos»
- L6: hallazgo del primer run de install.sh: faltaba `mkdir -p` del
  directorio de instalación (falla en un HOME sin ~/.local/bin). Corregido
  junto con S6.
- L7: «esto es una version de pi, pero no es la version de pi real, no se si
  afecta el que este instalado en ambas si no afecta podemos agregar tambien
  una opcion para gentle shell ya que pi esta instalado en otra ruta» —
  gentle-shell y pi son hosts separados (fork instalado en otra ruta);
  tener la extensión en ambos no afecta (cada host registra su propio MCP
  y sus sesiones son independientes). El instalador vuelve a ofrecer los
  dos: pi en PI_CODING_AGENT_DIR o ~/.pi/agent, gentle-shell en
  GENTLE_SHELL_AGENT_DIR o ~/.gentle-shell/agent; --all cubre ambos.
