<script>
  // The switcher of plans (S1, S3, D3, D5; A2 of `f8-planos.md`): its own
  // button in the bottom-left corner, its own panel above it, «Mi plano»
  // first and then one row per session. Ported from
  // `crates/gx-app/frontend/plan.js:1152-1508` and `index.html:49-63`.
  //
  // Everything it says comes from `graphium://state` (CONTRACT.md §8, C28/C29):
  // the colour of the family, the label of the session, its project, whether it
  // is `activa` or `en gracia` and how many diagrams it has; the plan on screen
  // is the one marked. The notice explains why the plan on screen changed when
  // the session that owned it ended.
  import { USER_PLAN, USER_PLAN_ID } from "./contract.js";
  import {
    cloneCountText,
    diagramCountText,
    familyClass,
    familyName,
    projectName,
  } from "./labels.js";

  let {
    rows = [],
    clones = [],
    activePlanId = null,
    notice = "",
    onchoose,
  } = $props();

  // The panel of sessions starts closed, like the vanilla page
  // (`index.html:61-63`: `aria-expanded="false"`, `class="… hidden"`).
  let open = $state(false);

  const chosen = (planId) => planId !== null && planId === activePlanId;
</script>

<button
  id="sessions-toggle"
  class="sessions-toggle"
  type="button"
  aria-label="mostrar u ocultar las sesiones"
  aria-expanded={open}
  aria-controls="sessions-panel"
  title="sesiones"
  onclick={() => (open = !open)}>◉</button
>

<aside id="sessions-panel" class="sessions-panel" class:hidden={!open} aria-label="sesiones">
  <h2 class="panel-title">sesiones</h2>
  <div id="session-list" class="session-list" role="group" aria-label="planos">
    <!-- The plan of the human (B1; S4): always first, even with no session at
         all. Nobody else draws there, so its dot is not a family but the
         accent of the theme. -->
    <button
      type="button"
      class="session-row user-plan"
      class:selected={chosen(USER_PLAN_ID)}
      aria-pressed={chosen(USER_PLAN_ID) ? "true" : "false"}
      title="tu plano: los diagramas que clonaste para analizar"
      onclick={() => onchoose?.(USER_PLAN)}
    >
      <span class="session-dot user-plan-dot"></span>
      <span class="session-body">
        <span class="session-head">
          <span class="session-label">{USER_PLAN.label}</span>
          <span class="session-count" title={cloneCountText(clones.length)}>{clones.length}</span>
        </span>
        <span class="session-meta">
          <span class="session-family">plano del usuario</span>
        </span>
      </span>
    </button>

    {#each rows as { session, count } (session.id)}
      {@const chosenNow = chosen(session.id)}
      {@const project = projectName(session.project)}
      <button
        type="button"
        class="session-row"
        class:selected={chosenNow}
        class:in-grace={session.state === "in_grace"}
        aria-pressed={chosenNow ? "true" : "false"}
        title={session.label}
        onclick={() => onchoose?.({ id: session.id, label: session.label })}
      >
        <span class="session-dot {familyClass(session.family)}"></span>
        <span class="session-body">
          <span class="session-head">
            <span class="session-label">{session.label}</span>
            <span class="session-count" title={diagramCountText(count)}>{count}</span>
          </span>
          <span class="session-meta">
            <span class="session-family">{familyName(session.family)}</span>
            <span class="session-project" title={project || undefined}
              >{project || "sin proyecto"}</span
            >
            <span class="session-state">{session.state === "in_grace" ? "en gracia" : "activa"}</span>
          </span>
        </span>
      </button>
    {/each}
  </div>
  <p id="session-notice" class="session-notice" class:hidden={notice === ""} role="status">
    {notice}
  </p>
</aside>
