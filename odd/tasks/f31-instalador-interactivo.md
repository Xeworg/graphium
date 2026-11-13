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

L2 (T1-T3 landed). The installer opens the controlling terminal once, when stdin
is not a tty, and reuses that same descriptor to draw and to read: `/dev/tty` is
opened with `exec 3<>/dev/tty`, so a piped `curl | bash` shows the checklist and
accepts keys while a `docker run` without `-t` still gets the actionable message
and exits. The branches that decide «no terminal» are now only two: `/dev/tty`
could not be opened, or stdin was closed when the script started — the second
matters because a command substitution steals fd 0, so «closed» is only
observable at startup, not later. `read -r -u "$fd" name` is the other trap the
worker hit: the redirection options come before the variable names or `read`
takes `-u` as a variable. Before touching anything the run says what is installed
and what replaces it; a host that already had the extension reports
«actualizada en … (graphium, vOLD -> vNEW)», and the checklist row names presence,
the installed version and the target version. Two tests pin it:
`tests/installer/terminal_test.sh` (a piped script inside a pty reaches the
checklist; `setsid -w` gives the no-controlling-terminal message; closed stdin is
not interactive) and `tests/installer/update_story_test.sh` (a second run reports
the update and the installed `package.json` carries the new version).

L3 (the hang was a test that measured the wrong thing, not the TUI). T4's
diagnosis: the pty case had a 60 s cap, and a repo-mode `install` takes 61-64 s on
this machine because it compiles — so `timeout` killed the shell at 124 before any
assertion ran, and the runner reported «1 test(s) failed» with no FAIL line to
point at. The cap is now 300 s with `GRAPHIUM_TEST_TIMEOUT` as the override,
`timeout -k 5`, a fixed 40x100 pty, and a named diagnosis when the cap is what
hits. Nothing in the checklist was broken; what was broken was the budget of the
test that watches it, which is why it looked like a hang from the outside.

L4 (a rule the README taught that is no longer true, fixed by the parent because
it fell outside the worker's surfaces). `README.md:83` said «Sin terminal
(scripts, pipes) hay que pedir los agentes explícitos»: that is exactly the
behaviour this cut removes. The README now says the controlling terminal is the
evidence of a human, that the checklist appears in a piped run, and that only
without a controlling terminal does the installer ask for `--env`/`--all` and
finish without waiting for a key.

L5 (delivery). This branch stacks on `feature/f30-instalador-extension` (not on
`staging`) because it changes the same file and assumes its `RELEASE_VERSION` and
`GRAPHIUM_RELEASES_URL`. Slices: `6bd2dae` the document, then one commit for T1-T3
plus T4 with the two new tests. The gate before any merge is
`bash scripts/test-installer.sh` on the committed tree, with the runner's own exit
status read directly instead of through a pipe.
