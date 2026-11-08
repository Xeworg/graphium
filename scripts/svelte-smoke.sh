#!/usr/bin/env bash
# Smoke test of the frontend inside the real Tauri shell: paints one diagram
# through the MCP door and reports what Rust says came back.
#
# Why this is the evidence of the handshake: the render hook waits
# `READY_TIMEOUT_MS` for `graphium://ready` (ui_error if it never arrives) and
# then `RENDER_ACK_TIMEOUT_MS` for `render_ack` (ui_timeout if the page never
# confirms). `"rendered": true` is therefore the only place outside the webview
# where "the page announced itself AND confirmed the paint" is observable.
#
# Usage:
#   scripts/svelte-smoke.sh [binary]
#
#   binary  target/debug/graphium (default) — it tests whatever frontend that
#           binary embeds: the vanilla one (`cargo build -p gx-app`) or the
#           Svelte one (`scripts/svelte-tauri.sh`).
#
# `SMOKE_HOLD_SECONDS` (default 8) is how long the session stays open after the
# call, to look at the window before the owner leaves.
#
# Isolation: the owner binds its single-instance socket inside its own
# `XDG_RUNTIME_DIR` and reads/writes its settings in its own `HOME`, both under
# `/tmp`. The owner the script starts is asked to quit before leaving, the
# directories it created are removed, and it never touches the owner of the
# human (which lives in the real `XDG_RUNTIME_DIR`).
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
binary=${1:-$root/target/debug/graphium}
# How long the MCP session is kept open after the call. It only matters when a
# human (or a screenshot) wants to look at the window before the owner leaves.
hold_seconds=${SMOKE_HOLD_SECONDS:-8}

if [ ! -x "$binary" ]; then
  echo "svelte-smoke: no binary at $binary (build it first)" >&2
  exit 2
fi
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
  echo "svelte-smoke: no DISPLAY: the render hook reports 'headless' instead of painting" >&2
  exit 2
fi

runtime=$(mktemp -d /tmp/gx-smoke-run-XXXXXX)
home=$(mktemp -d /tmp/gx-smoke-home-XXXXXX)
chmod 700 "$runtime"
out="$runtime/mcp.jsonl"
in="$runtime/in.fifo"
feed_pid=""
forward_pid=""

owner_quit() {
  # `graphium quit` closes the owner through the lifecycle (S5).
  XDG_RUNTIME_DIR="$runtime" HOME="$home" "$binary" quit >/dev/null 2>&1 || true
}

# The owners this script started, and only them: a process is ours when its
# `XDG_RUNTIME_DIR` is the temp dir created above. The owner of the human lives
# in the real runtime dir, so it can never be in this list.
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
  [ -n "$feed_pid" ] && kill "$feed_pid" 2>/dev/null || true
  [ -n "$forward_pid" ] && kill "$forward_pid" 2>/dev/null || true
  owner_quit
  # The owner leaves through the lifecycle when it sees the quit line; give it
  # a moment, then terminate whatever of ours is still alive (SIGTERM is the
  # graceful door of the owner anyway, S5).
  for _ in $(seq 1 50); do
    [ -z "$(owner_pids)" ] && break
    sleep 0.1
  done
  for pid in $(owner_pids); do
    kill "$pid" 2>/dev/null || true
  done
  case "$runtime" in /tmp/gx-smoke-run-*) rm -rf "$runtime" ;; esac
  case "$home" in /tmp/gx-smoke-home-*) rm -rf "$home" ;; esac
}
trap cleanup EXIT

mkfifo "$in"

document='"diagram":{"version":1,"kind":"graph","title":"smoke","direction":"TB","nodes":[{"id":"read","label":"read file"},{"id":"ok","label":"Config"}],"edges":[{"from":"read","to":"ok","label":"utf-8"}]}'

# The three well-known lines of the transport (initialize, the notification
# and the call), the same shape the Rust tests drive in `gx-mcp`.
{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"graphium-svelte-smoke","version":"0"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  printf '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"render_diagram","arguments":{%s}}}\n' "$document"
  sleep "$hold_seconds"
} >"$in" &
feed_pid=$!

XDG_RUNTIME_DIR="$runtime" HOME="$home" "$binary" --stdio <"$in" >"$out" 2>"$runtime/stderr" &
forward_pid=$!

# Wait for the answer of the render call: the layout plus the round trip to the
# window is quick, but a cold webview may take a few seconds.
deadline=$((SECONDS + 60))
while [ "$SECONDS" -lt "$deadline" ]; do
  if grep -q '"id":2' "$out" 2>/dev/null; then
    break
  fi
  sleep 0.2
done

answer=$(grep '"id":2' "$out" 2>/dev/null | head -1 || true)
if [ -z "$answer" ]; then
  echo "svelte-smoke: no answer for the render call" >&2
  echo "--- stderr" >&2
  cat "$runtime/stderr" >&2 || true
  echo "--- stdout" >&2
  cat "$out" >&2 || true
  exit 1
fi

rendered=$(printf '%s' "$answer" | grep -o '"rendered":[a-z]*' | head -1 | cut -d: -f2 || true)
reason=$(printf '%s' "$answer" | grep -o '"reason":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
diagram=$(printf '%s' "$answer" | grep -o '"diagram_id":"[^"]*"' | head -1 | cut -d'"' -f4 || true)

echo "svelte-smoke: diagram=${diagram:-?} rendered=${rendered:-?} reason=${reason:-none}"
echo "svelte-smoke: binary=$binary"

if [ "$rendered" = "true" ]; then
  exit 0
fi
echo "svelte-smoke: the window did not confirm the paint (reason=${reason:-?})" >&2
exit 1
