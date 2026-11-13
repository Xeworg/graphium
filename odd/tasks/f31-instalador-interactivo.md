# f31-instalador-interactivo

The installer a human actually runs: one command, a TUI when a terminal exists
(even when the script arrives through a pipe), an update offered when a version is
already installed, the agents to connect, and the extension refreshed in place.
Branch `feature/f31-instalador-interactivo`, base `feature/f30-instalador-extension`.

## Specs

S1 (owner, verbatim): "no, habia pedido un instalado interactivo para el usuario
previamente que el usuario solo tenga que ejecutarlo y tenga una tui para que
pueda actualizar si ya tiene la version instalada al ejecutarlo, tambien quiero
que pueda elegir los agentes para que el usuario pueda instalar, ejemplo
opencode, pi, claude, estos solo existe uno aun no trabajamos en ese por por el
momento pi y gentle shell, si esta instalado debe actualizarlo la extension
tambien".

S2 (one command). The entry a human executes is `./install.sh` or the published
`curl -fsSL …/install.sh | bash`. Neither may require a flag to reach the
interactive path.

S3 (the TUI must survive the pipe). When the script arrives through a pipe,
stdin is the script itself, so `[[ -t 0 ]]` is false even in a real terminal. The
prompt must open the controlling terminal (`/dev/tty`) instead of concluding that
no terminal exists. Behaviour by case: tty/`/dev/tty` → checklist; no controlling
terminal → today's actionable message, never a hang.

S4 (update what is already there). Run over an existing install, the flow says
what is installed and what will replace it, and after the run the version changed.
That applies to the binary and to the extension: a host that already has the
extension gets it **updated in place**, and the message distinguishes
«actualizada» from «instalada».

S5 (choosing agents). The selector offers the registry's hosts, shows which are
present on the machine and which already have the extension (with its version),
and installs into exactly the chosen ones. `pi` and `gentle-shell` are the hosts
that exist today; `opencode` and `claude` are named by the owner as future hosts
and are **not** added until they are real targets — the registry stays the single
place where adding one is a one-line change plus its test.

S6 (the gate must run). `tests/installer/agent_prompt_test.sh` hangs in this
environment today, with and without the changes of this cut (measured by running
it against the previous installer). A TUI that cannot be tested is a TUI nobody
can trust: the hang is part of this cut, not a known issue.

## Decisions

D1 (the terminal is `/dev/tty`, not stdin). Every interactive read opens
`/dev/tty` for reading and writing when stdin is not a terminal, and only falls
back to the numeric prompt or the message when there is no controlling terminal
at all. This is what makes `curl | bash` interactive without asking the human to
download the script first.

D2 (one flow, not two). `install` keeps being the default command and ends by
offering the agent choice, as it does today; this cut makes the choice readable
and usable in the piped case instead of adding a second command.

D3 (update is not a separate verb for the human). `update` stays for scripts, but
running `install` again must be enough: the binary is replaced, and every chosen
host that already had the extension gets the new one.

D4 (no invented hosts). The registry only lists hosts the machine can actually
reach with a known directory. `opencode` and `claude` enter when their paths and
their extension formats exist, with their tests.

## Tasks

T1 `/dev/tty` for the interactive reads: open the controlling terminal for the
checklist and the numeric prompt, keep the no-terminal message, and stop treating
`[[ -t 0 ]]` as the only evidence of a human. Test first: the piped case reaches
the checklist (pty test) and the no-terminal case answers instead of hanging.
T2 the update story: read the installed versions before doing anything, say what
will be replaced, and distinguish «actualizada» from «instalada» per host.
Test first: a second run over an installed host reports an update and the
extension's `package.json` version is the new one.
T3 the selector's detail: every row says whether the host is present, whether it
already has the extension and which version. Test first: a host with an old
extension is marked as such.
T4 the hang: `tests/installer/agent_prompt_test.sh` must run to completion in this
environment, and the reason it hung is written down. Test first: the test itself,
red before and green after.

## Log

L1 (owner, verbatim): the request of S1.
