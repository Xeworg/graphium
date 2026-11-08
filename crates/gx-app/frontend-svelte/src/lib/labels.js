// The words the plan says, verbatim from the vanilla page (CONTRACT.md §8,
// C41). The port translates nothing: every string here is the one
// `crates/gx-app/frontend/plan.js` and `index.html` already show, including
// the guillemets, the ellipsis and the plural forms.
//
// The strings that belong to one piece of markup (a button label, a panel
// title) stay in that markup; this module keeps the ones the page *builds*
// from the state: the counts, the family names, the project of a session and
// the notice that says why the plan on screen changed.

/** The family of a session as the human reads it (plan.js:1160-1166). */
export const SESSION_FAMILIES = {
  pi: "Pi",
  claude: "Claude",
  opencode: "OpenCode",
  ui: "Interfaz",
  other: "Otro",
};

/** The colour class of a family: one of the five tokens, else `other`. */
export function familyClass(family) {
  return Object.prototype.hasOwnProperty.call(SESSION_FAMILIES, family)
    ? `family-${family}`
    : "family-other";
}

/** The family of a session as the human reads it (D1). */
export function familyName(family) {
  return SESSION_FAMILIES[family] || SESSION_FAMILIES.other;
}

/** The last part of a project path: `/home/x/graphium` -> `graphium`. */
export function projectName(project) {
  const text = typeof project === "string" ? project : "";
  const parts = text.split("/").filter((part) => part !== "");
  return parts.length > 0 ? parts[parts.length - 1] : "";
}

/** The text of the count: `1 diagrama`, `3 diagramas`. */
export function diagramCountText(count) {
  return `${count} ${count === 1 ? "diagrama" : "diagramas"}`;
}

/** The text of the count of a clone: `1 clon`, `3 clones`. */
export function cloneCountText(count) {
  return `${count} ${count === 1 ? "clon" : "clones"}`;
}

/**
 * The moment of the last change, in the clock of the human (`es`, short).
 *
 * A state without a readable date shows the raw value: the panel never invents
 * a time for a diagram (plan.js:880-889).
 */
export function formatUpdated(value) {
  const when = new Date(value);
  if (Number.isNaN(when.getTime())) {
    return String(value ?? "");
  }
  return when.toLocaleString("es", { dateStyle: "short", timeStyle: "short" });
}

/**
 * Why the plan on screen changed when the session that owned it ended (S2,
 * D4; A2 of `f8-planos.md`). The two forms are the vanilla ones.
 */
export function sessionEndedNotice(ended, next) {
  if (next === null) {
    return `la sesión «${ended.label}» terminó; no queda ningún plano`;
  }
  return `la sesión «${ended.label}» terminó; muestro el plano de «${next.label}»`;
}

/** What the box of a culled object says (plan.js:2098-2108, C15). */
export function boxText(rec) {
  const title = rec.caption || rec.id;
  return `${title} — ${Math.round(rec.rect.w)} × ${Math.round(rec.rect.h)}`;
}
