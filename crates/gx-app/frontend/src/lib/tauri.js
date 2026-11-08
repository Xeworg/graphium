// The Tauri bridge, with the same two doors the vanilla page uses
// (`plan.js:294-331`): the global bundle `window.__TAURI__` (the app declares
// `withGlobalTauri: true`) and, when it is not there, the internal IPC the
// bundle itself speaks.
//
// The capabilities of the window allow exactly these three calls
// (`crates/gx-app/capabilities/default.json`):
// `core:event:allow-emit`, `core:event:allow-listen`, `core:event:allow-unlisten`.

const tauri = typeof window === "undefined" ? undefined : window.__TAURI__;
const internals = typeof window === "undefined" ? undefined : window.__TAURI_INTERNALS__;

/**
 * Emits an event to Rust (plan.js:304-317).
 */
export function emit(name, payload) {
  if (tauri && tauri.event && typeof tauri.event.emit === "function") {
    tauri.event.emit(name, payload);
    return;
  }
  if (internals && typeof internals.invoke === "function") {
    internals.invoke("plugin:event|emit", { event: name, payload });
    return;
  }
  console.error(`graphium: no hay puente de Tauri para emitir ${name}`);
}

/**
 * Listens to an event coming from Rust (plan.js:319-331).
 *
 * Returns nothing: the listener lives for the life of the window, the way the
 * vanilla page registers it once at load.
 */
export function listen(name, handler) {
  if (tauri && tauri.event && typeof tauri.event.listen === "function") {
    tauri.event.listen(name, handler);
    return;
  }
  if (internals && typeof internals.invoke === "function") {
    internals.invoke("plugin:event|listen", {
      event: name,
      target: { kind: "Any" },
      handler: internals.transformCallback((event) => handler(event)),
    });
  }
}

/** Whether the page runs inside the Tauri shell at all. */
export function hasBridge() {
  return Boolean(tauri || internals);
}
