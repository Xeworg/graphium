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
#      `Pss` (proportional set size) of the owner and every process of its tree
#      (`smaps_rollup`), twice.
#
# Why Pss and not RSS: WebKitGTK splits the window into several processes
# (WebKitWebProcess, WebKitNetworkProcess, the GPU helper) and they share most
# of the mapped libraries; summing RSS would count every shared page once per
# process. Pss divides each shared mapping between its users, so the sum is the
# physical footprint of the tree. Blind spots: pages shared with processes
# outside the tree (another browser, the shell) are still divided as if only the
# tree used them; and a helper that got reparented would be missed.
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

owner_pids() {
  local pid
  for pid in $(pgrep -f -- '--owner' 2>/dev/null || true); do
    if [ -r "/proc/$pid/environ" ] &&
      tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null | grep -qx "XDG_RUNTIME_DIR=$runtime"; then
      printf '%s\n' "$pid"
    fi
  done
}

cleanup() {
  # The MCP session ends when the write end of the fifo closes.
  exec 3>&- 2>/dev/null || true
  XDG_RUNTIME_DIR="$runtime" HOME="$home" "$binary" quit >/dev/null 2>&1 || true
  for _ in $(seq 1 50); do
    [ -z "$(owner_pids)" ] && break
    sleep 0.1
  done
  for pid in $(owner_pids); do kill "$pid" 2>/dev/null || true; done
  [ -n "$stdio_pid" ] && wait "$stdio_pid" 2>/dev/null || true
  case "$runtime" in /tmp/gx-bench-run-*) rm -rf "$runtime" ;; esac
  case "$home" in /tmp/gx-bench-home-*) rm -rf "$home" ;; esac
}
trap cleanup EXIT

# The fifo is opened read-write by the script (a write-only open would block
# until the owner opened its end), and it stays open for the whole measurement:
# the owner reads MCP from it and would leave on EOF.
exec 3<>"$fifo"
XDG_RUNTIME_DIR="$runtime" HOME="$home" "$binary" --stdio <"$fifo" >"$out" 2>"$err" &
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

sum_pss() {
  local total=0 pids pid
  pids=$(owner_pids)
  # Children of the owner, recursively (the WebKit helpers).
  local queue="$pids" seen=""
  while [ -n "$queue" ]; do
    pid=$(printf '%s\n' "$queue" | head -1)
    queue=$(printf '%s\n' "$queue" | grep -v "^$pid$" || true)
    case " $seen " in *" $pid "*) continue ;; esac
    seen="$seen $pid"
    if [ -r "/proc/$pid/smaps_rollup" ]; then
      total=$((total + $(awk '/^Pss:/ {sum += $2} END {print sum + 0}' "/proc/$pid/smaps_rollup")))
    fi
    for child in $(pgrep -P "$pid" 2>/dev/null || true); do
      queue=$(printf '%s\n%s' "$queue" "$child")
    done
  done
  printf '%s' "$total"
}

sum_rss() {
  local total=0 pids pid
  pids=$(owner_pids)
  local queue="$pids" seen=""
  while [ -n "$queue" ]; do
    pid=$(printf '%s\n' "$queue" | head -1)
    queue=$(printf '%s\n' "$queue" | grep -v "^$pid$" || true)
    case " $seen " in *" $pid "*) continue ;; esac
    seen="$seen $pid"
    if [ -r "/proc/$pid/status" ]; then
      total=$((total + $(awk '/^VmRSS:/ {print $2}' "/proc/$pid/status")))
    fi
    for child in $(pgrep -P "$pid" 2>/dev/null || true); do
      queue=$(printf '%s\n%s' "$queue" "$child")
    done
  done
  printf '%s' "$total"
}

small="$runtime/small.json"
large="$runtime/large.json"
node "$root/crates/gx-app/frontend-svelte/tools/diagram.mjs" 12 >"$small"
node "$root/crates/gx-app/frontend-svelte/tools/diagram.mjs" "$nodes" >"$large"

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

# Steady state: the last render is the large diagram. Give the owner a moment
# to settle (the paint, the GC of the previous drawing) and read the tree twice.
sleep 3
pss1=$(sum_pss)
rss1=$(sum_rss)
sleep 1
pss2=$(sum_pss)
rss2=$(sum_rss)
processes=$(owner_pids | wc -l | tr -d ' ')

printf 'bench-live label=%s run=%s binary=%s\n' "$label" "$run" "$binary"
printf 'bench-live cold-start(ms)=%s small(ms)=%s/%s large(ms)=%s/%s rendered=%s/%s reason=%s\n' \
  "$cold_start_ms" "$small1" "$small2" "$large1" "$large2" "$rendered_small" "$rendered_large" "${reason_small:-none}"
printf 'bench-live memory pss(kB)=%s/%s rss(kB)=%s/%s processes=%s\n' "$pss1" "$pss2" "$rss1" "$rss2" "$processes"

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
  "rssKb": [$rss1, $rss2],
  "processes": $processes
}
JSON
fi

if [ "$rendered_large" != "true" ]; then
  echo "bench-live: the large diagram did not paint (reason=${reason_small:-?})" >&2
  exit 1
fi
