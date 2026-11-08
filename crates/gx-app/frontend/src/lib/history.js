// The history of versions, as plain arithmetic (CONTRACT.md §8, C25/C26/C31/
// C32): where the indicator of a diagram or of a clone sits, what a step asks
// for and when the live drawing (or the photo of the clone) is back.
//
// Every function takes values and returns values: the component
// (`src/App.svelte`) keeps the state and the `graphium://` intents, and this
// file keeps the numbers the parity contract pins. Ported from
// `plan.js:1010-1150` (`versionNav`, `versionStep`, `viewVersion`) and
// `plan.js:1750-1877` (`updateCloneTools`, `cloneVersionStep`,
// `viewCloneVersion`).

/**
 * The absolute version the indicator of a diagram shows (plan.js:1032-1038):
 * the version in view when the store still keeps it, else the live drawing.
 * A version the history discarded cannot be browsed.
 */
export function versionAt(entry, viewed) {
  const value = viewed === null || viewed === undefined ? null : viewed;
  const kept =
    value !== null &&
    value >= entry.oldest_version &&
    value <= entry.current_version;
  return kept ? value : entry.current_version;
}

/**
 * The text of the indicator: `v3/7`, the position inside the versions the
 * store keeps (`oldest_version..=current_version`, absolute and consecutive).
 */
export function versionLabel(entry, viewed) {
  return `v${versionAt(entry, viewed) - entry.oldest_version + 1}/${entry.version_count}`;
}

/**
 * The absolute version a clone shows (plan.js:1755-1770): the version in view
 * when it is still kept, else the photo — the newest version the clone pulled,
 * not the newest of the original.
 */
export function cloneVersionAt(clone, viewed) {
  const oldest = Number(clone.oldest_version || 0);
  const current = Number(clone.current_version || oldest);
  const shown =
    viewed === null || viewed === undefined
      ? Number(clone.snapshot_version || current)
      : viewed;
  return shown >= oldest && shown <= current ? shown : current;
}

/**
 * The text of the pill of a clone: `2/2` — the compact pill of L8 drops the
 * `v` the panel item carries (plan.js:1772).
 */
export function cloneVersionLabel(clone, viewed) {
  const count = Number(clone.version_count || 0);
  if (!(count > 0)) {
    return "";
  }
  const oldest = Number(clone.oldest_version || 0);
  return `${cloneVersionAt(clone, viewed) - oldest + 1}/${count}`;
}

/**
 * Whether the history of a clone is walkable at all: the refresh is enabled
 * only while the original is on a plan (B2) and the history only exists with
 * more than one version (L8).
 */
export function cloneHistoryOpen(clone) {
  return Boolean(clone && clone.updatable && Number(clone.version_count || 0) > 1);
}
