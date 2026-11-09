#!/usr/bin/env bash
# The T5 bench, shell half: the real binary, on the real display, through the
# MCP door. It measures what the headless half (`bench.mjs`) cannot:
#
#   1. cold start: from launching the owner to the answer of the first painted
#      diagram (process + window + webview + `graphium://ready` + `render_ack`);
#   2. the round trip of `render_diagram` for a small and a large diagram, twice
#      each, after a warm-up call (so the numbers are the frontend's paint, not
#      the webview's cold boot);
#   3. steady-state memory while the large diagram is on screen: the summed
#      `Pss` (proportional set size) of the owner, every process of its tree and
#      every WebKit process this launch brought up (`smaps_rollup`), twice.
#
# Why Pss and not RSS: WebKitGTK splits the window into several processes
# (WebKitWebProcess, WebKitNetworkProcess, the GPU helper) and they share most
# of the mapped libraries; summing RSS would count every shared page once per
# process. Pss divides each shared mapping between its users, so the sum is the
# physical footprint of the launch. Blind spot: pages shared with processes
# outside the set (another browser, the shell) are still divided as if only this
# launch used them.
#
# Which processes are "the launch" — and why parenthood is not enough. The
# helper that hosts the page is a child of the owner on most desktops, but not
# on all of them: WebKitGTK may spawn it from a launcher that exits, and a
# sandbox wrapper puts it out of reach of `pgrep -P`, so a walk of the owner's
# descendant tree can miss the process that holds most of the drawing. The
# helpers are therefore attributed by *time*: the WebKit-family PIDs on the
# machine are snapshotted before the owner starts, and every one that appears
# afterwards belongs to this launch. A helper whose environment names a
# different `XDG_RUNTIME_DIR` is vetoed — that is another owner's window on this
# machine — while a scrubbed environment (the sandbox case) is accepted, the
# time window being then the only evidence available. The owner's descendant
# tree stays in the set and the union is deduplicated, so where the helper *is*
# a child this measures exactly what it measured before, plus nothing. The
# snapshot-diff is the portable half; the parenthood half is the fallback.
#
# The output names what it sums: `pssKb` is the attributed total, `pssOwnerKb`
# the owners, `pssWebKb` the WebKit helpers, `processes` the size of the
# attributed set (`webProcesses` of them helpers).
#
# Usage:
#   tools/bench-live.sh --binary target/debug/graphium --label svelte --run 1 \
#       [--json /tmp/bench-live-svelte-1.json]
#
# It isolates the owner in its own `XDG_RUNTIME_DIR` and `HOME` under `/tmp`,
# quits it at the end, and removes both directories. It requires a display.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
binary=""
label=""
run="1"
json=""
nodes="300"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --binary) binary="$2"; shift 2 ;;
    --label) label="$2"; shift 2 ;;
    --run) run="$2"; shift 2 ;;
    --nodes) nodes="$2"; shift 2 ;;
    --json) json="$2"; shift 2 ;;
    *) echo "bench-live: unknown argument: $1" >&2; exit 2 ;;
  esac
done
if [ -z "$binary" ] || [ -z "$label" ]; then
  echo "bench-live: --binary and --label are required" >&2
  exit 2
fi
binary=$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")
if [ ! -x "$binary" ]; then
  echo "bench-live: no binary at $binary" >&2
  exit 2
fi
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
  echo "bench-live: no DISPLAY: the render hook reports 'headless' instead of painting" >&2
  exit 2
fi

runtime=$(mktemp -d /tmp/gx-bench-run-XXXXXX)
home=$(mktemp -d /tmp/gx-bench-home-XXXXXX)
chmod 700 "$runtime"
out="$runtime/mcp.jsonl"
err="$runtime/stderr"
fifo="$runtime/in.fifo"
mkfifo "$fifo"
stdio_pid=""
webkit_before=""

owner_pids() {
  local pid
  for pid in $(pgrep -f -- '--owner' 2>/dev/null || true); do
    # `cat` opens the file, so an owner that exits between the listing and the
    # read costs an empty environment and not a shell error on stderr.
    if [ -r "/proc/$pid/environ" ] &&
      cat "/proc/$pid/environ" 2>/dev/null | tr '\0' '\n' | grep -qx "XDG_RUNTIME_DIR=$runtime"; then
      printf '%s\n' "$pid"
    fi
  done
}

# Every process that hosts part of the window's web content: the page itself
# (WebKitWebProcess), its network process and the GPU helper.
webkit_family_pids() {
  pgrep -f -- 'WebKit(Web|Network|GPU)Process' 2>/dev/null || true
}

# A helper whose environment names another runtime dir belongs to another
# owner's window on this machine and is not attributed to this launch. An
# unreadable or scrubbed environment (the WebKit sandbox) is accepted: the time
# window is then the only evidence available.
belongs_to_us() {
  local pid="$1" dir
  [ -r "/proc/$pid/environ" ] || return 1
  # `cat` opens the file, so a process that vanished between the test and the
  # read costs an empty environment and not a shell error on stderr.
  dir=$(cat "/proc/$pid/environ" 2>/dev/null | tr '\0' '\n' | sed -n 's/^XDG_RUNTIME_DIR=//p' | head -1)
  [ -z "$dir" ] && return 0
  [ "$dir" = "$runtime" ]
}

# One PID per line: the owner(s) and their whole descendant tree, then every
# WebKit-family process that did not exist before the owner was launched (see
# the attribution note in the header). Deduplicated, so a helper that is both a
# child and new is summed once.
attributed_pids() {
  local -a queue
  local pid child seen=" "
  queue=($(owner_pids))
  while [ "${#queue[@]}" -gt 0 ]; do
    pid="${queue[0]}"
    queue=("${queue[@]:1}")
    [ -n "$pid" ] || continue
    case "$seen" in *" $pid "*) continue ;; esac
    seen="$seen$pid "
    printf '%s\n' "$pid"
    for child in $(pgrep -P "$pid" 2>/dev/null || true); do
      queue+=("$child")
    done
  done
  for pid in $(webkit_family_pids); do
    [ -d "/proc/$pid" ] || continue
    case " $webkit_before " in *" $pid "*) continue ;; esac
    case "$seen" in *" $pid "*) continue ;; esac
    belongs_to_us "$pid" || continue
    seen="$seen$pid "
    printf '%s\n' "$pid"
  done
}

pss_kb() {
  local v
  v=$(awk '/^Pss:/ {sum += $2} END {print sum + 0}' "/proc/$1/smaps_rollup" 2>/dev/null) || v=""
  printf '%s' "${v:-0}"
}

rss_kb() {
  local v
  v=$(awk '/^VmRSS:/ {print $2 + 0}' "/proc/$1/status" 2>/dev/null) || v=""
  printf '%s' "${v:-0}"
}

# One reading of the attributed set, as one line: totalPss ownerPss webPss
# totalRss pids webPids.
sample_memory() {
  local owners pid pss total=0 owner=0 web=0 rss=0 count=0 web_count=0
  owners=$(owner_pids)
  for pid in $(attributed_pids); do
    pss=$(pss_kb "$pid")
    total=$((total + pss))
    rss=$((rss + $(rss_kb "$pid")))
    count=$((count + 1))
    if printf '%s\n' "$owners" | grep -qx -- "$pid"; then
      owner=$((owner + pss))
    else
      web=$((web + pss))
      web_count=$((web_count + 1))
    fi
  done
  printf '%s %s %s %s %s %s' "$total" "$owner" "$web" "$rss" "$count" "$web_count"
}

cleanup() {
  local pid state
  # The MCP session ends when the write end of the fifo closes: the owner's
  # `--stdio` process holds only the read end (the launch closes fd 3 in it), so
  # this is EOF for it and it leaves. The reap below is bounded anyway — an
  # owner that does not leave must not turn the teardown into a hang, which is
  # what a copy of the read-write end inside the owner used to cause.
  exec 3>&- || true
  XDG_RUNTIME_DIR="$runtime" HOME="$home" "$binary" quit >/dev/null 2>&1 || true
  for _ in $(seq 1 50); do
    [ -z "$(owner_pids)" ] && break
    sleep 0.1
  done
  for pid in $(owner_pids); do kill "$pid" 2>/dev/null || true; done
  if [ -n "$stdio_pid" ]; then
    for _ in $(seq 1 50); do
      state=$(awk '/^State:/ {print $2}' "/proc/$stdio_pid/status" 2>/dev/null || true)
      [ -z "$state" ] && break    # gone, nothing left to reap
      [ "$state" = "Z" ] && break # exited, only the reap is missing
      sleep 0.1
    done
    kill -KILL "$stdio_pid" 2>/dev/null || true
    wait "$stdio_pid" 2>/dev/null || true
  fi
  case "$runtime" in /tmp/gx-bench-run-*) rm -rf "$runtime" ;; esac
  case "$home" in /tmp/gx-bench-home-*) rm -rf "$home" ;; esac
}
trap cleanup EXIT

# The fifo is opened read-write by the script (a write-only open would block
# until the owner opened its end), and it stays open for the whole measurement:
# the owner reads MCP from it and would leave on EOF. The owner must not inherit
# fd 3 (`3>&-` below): a copy of the read-write end inside the owner keeps the
# fifo open forever, so closing fd 3 here would never be EOF for the owner and
# the teardown would wait on a process that cannot leave.
exec 3<>"$fifo"
# The attribution snapshot, taken while no owner of this bench exists yet.
webkit_before="$(webkit_family_pids | tr '\n' ' ')"
XDG_RUNTIME_DIR="$runtime" HOME="$home" "$binary" --stdio <"$fifo" 3>&- >"$out" 2>"$err" &
stdio_pid=$!

now_ms() { date +%s%N | cut -b1-13; }

wait_answer() {
  local id="$1" deadline=$((SECONDS + 90))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if grep -q "\"id\":$id" "$out" 2>/dev/null; then
      return 0
    fi
    sleep 0.005
  done
  echo "bench-live: no answer for call $id" >&2
  tail -5 "$err" >&2 || true
  return 1
}

answer_field() {
  local id="$1" field="$2"
  grep "\"id\":$id" "$out" | head -1 | grep -o "\"$field\":[^,}]*" | head -1 | cut -d: -f2 | tr -d '"' || true
}

# call <id> <document-file>: prints the round trip in ms and leaves the answer
# in $out. The first call also measures the cold start of the whole owner.
cold_start_ms=0
call() {
  local id="$1" file="$2" started finished
  started=$(now_ms)
  printf '{"jsonrpc":"2.0","id":%s,"method":"tools/call","params":{"name":"render_diagram","arguments":%s}}\n' \
    "$id" "$(cat "$file")" >&3
  wait_answer "$id"
  finished=$(now_ms)
  if [ "$id" = "2" ]; then cold_start_ms=$((finished - started)); fi
  printf '%s' "$((finished - started))"
}

small="$runtime/small.json"
large="$runtime/large.json"
node "$root/crates/gx-app/frontend/tools/diagram.mjs" 12 >"$small"
node "$root/crates/gx-app/frontend/tools/diagram.mjs" "$nodes" >"$large"

# The MCP handshake.
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"graphium-bench","version":"0"}}}' >&3
printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}' >&3

# The warm-up call doubles as the cold-start measurement: it is the first time
# the window, the webview and the page do their whole job.
call 2 "$small" >/dev/null
small1=$(call 3 "$small")
small2=$(call 4 "$small")
large1=$(call 5 "$large")
large2=$(call 6 "$large")

rendered_small=$(answer_field 3 rendered)
reason_small=$(answer_field 3 reason)
rendered_large=$(answer_field 5 rendered)
reason_large=$(answer_field 5 reason)

# Steady state: the last render is the large diagram. Give the launch a moment
# to settle (the paint, the GC of the previous drawing) and read the attributed
# set twice, a second apart. The two counts are read with the first sample.
sleep 3
read -r pss1 owner1 web1 rss1 processes web_processes <<<"$(sample_memory)"
sleep 1
read -r pss2 owner2 web2 rss2 _processes2 _web_processes2 <<<"$(sample_memory)"

printf 'bench-live label=%s run=%s binary=%s\n' "$label" "$run" "$binary"
printf 'bench-live cold-start(ms)=%s small(ms)=%s/%s large(ms)=%s/%s rendered=%s/%s reason=%s\n' \
  "$cold_start_ms" "$small1" "$small2" "$large1" "$large2" "$rendered_small" "$rendered_large" "${reason_small:-none}"
printf 'bench-live memory pss(kB)=%s/%s owner(kB)=%s/%s web(kB)=%s/%s rss(kB)=%s/%s processes=%s webProcesses=%s\n' \
  "$pss1" "$pss2" "$owner1" "$owner2" "$web1" "$web2" "$rss1" "$rss2" "$processes" "$web_processes"

if [ -n "$json" ]; then
  cat >"$json" <<JSON
{
  "label": "$label",
  "run": "$run",
  "binary": "$binary",
  "nodes": $nodes,
  "coldStartMs": $cold_start_ms,
  "smallMs": [$small1, $small2],
  "largeMs": [$large1, $large2],
  "rendered": { "small": "$rendered_small", "large": "$rendered_large" },
  "reason": "${reason_small:-none}",
  "reasonLarge": "${reason_large:-none}",
  "pssKb": [$pss1, $pss2],
  "pssOwnerKb": [$owner1, $owner2],
  "pssWebKb": [$web1, $web2],
  "rssKb": [$rss1, $rss2],
  "processes": $processes,
  "webProcesses": $web_processes
}
JSON
fi

if [ "$rendered_large" != "true" ]; then
  echo "bench-live: the large diagram did not paint (reason=${reason_small:-?})" >&2
  exit 1
fi
