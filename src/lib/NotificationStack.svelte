<script lang="ts">
  /**
   * MON-51: fixed top-right overlay that renders the active notifications
   * from `notificationsStore`. Caps at VISIBLE_CAP; overflow collapses into
   * a "+N more" pill the user can click to expand. Hovering a card pauses
   * its auto-dismiss timer; the header line (if the notification has an
   * agentId) jumps to that agent's chat.
   */
  import {
    notificationsStore,
    type Notification,
  } from "./stores/notificationsStore.svelte";
  import { agentStore } from "./stores/agentStore.svelte";

  const VISIBLE_CAP = 5;

  let expanded = $state(false);

  // Newest-first. Once we're beyond the cap and not expanded, collapse the
  // tail into a "+N more" pill.
  const ordered = $derived([...notificationsStore.notifications].reverse());
  const visible = $derived(
    expanded ? ordered : ordered.slice(0, VISIBLE_CAP),
  );
  const hiddenCount = $derived(
    expanded ? 0 : Math.max(0, ordered.length - VISIBLE_CAP),
  );

  function jumpToAgent(n: Notification) {
    if (!n.agentId) return;
    agentStore.selectAgent(n.agentId);
  }
</script>

{#if ordered.length > 0}
  <div class="stack" role="region" aria-label="Notifications">
    {#each visible as notif (notif.id)}
      <div
        class="card"
        class:error={notif.level === "error"}
        class:warning={notif.level === "warning"}
        class:info={notif.level === "info"}
        role="status"
        onmouseenter={() => notificationsStore.pauseExpiry(notif.id)}
        onmouseleave={() => notificationsStore.resumeExpiry(notif.id)}
      >
        <div class="head">
          {#if notif.agentId}
            <button
              class="agent-link"
              onclick={() => jumpToAgent(notif)}
              title="Jump to {notif.agentName ?? 'agent'}"
            >
              {notif.agentName ?? "Agent"}
            </button>
          {:else}
            <span class="level-label">{notif.level}</span>
          {/if}
          {#if notif.count > 1}
            <span class="count" aria-label="occurrences">×{notif.count}</span>
          {/if}
          <button
            class="dismiss"
            onclick={() => notificationsStore.dismiss(notif.id)}
            aria-label="Dismiss notification"
            title="Dismiss"
          >×</button>
        </div>
        <div class="message">{notif.message}</div>
      </div>
    {/each}
    {#if hiddenCount > 0}
      <button class="more" onclick={() => (expanded = true)}>
        +{hiddenCount} more
      </button>
    {:else if expanded && ordered.length > VISIBLE_CAP}
      <button class="more" onclick={() => (expanded = false)}>
        Collapse
      </button>
    {/if}
  </div>
{/if}

<style>
  .stack {
    position: fixed;
    top: 12px;
    right: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(380px, calc(100vw - 24px));
    z-index: 1100;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
    pointer-events: none;
  }

  .card {
    pointer-events: auto;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 10px 12px;
    box-shadow: 0 12px 32px var(--shadow-dark);
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-left-width: 3px;
  }

  .card.error {
    border-color: var(--error-border-faint);
    border-left-color: var(--error);
    background: linear-gradient(
      90deg,
      var(--error-bg-subtle),
      var(--bg-panel) 60%
    );
  }
  .card.warning {
    border-color: var(--warning-border-subtle);
    border-left-color: var(--warning);
    background: linear-gradient(
      90deg,
      var(--warning-bg-subtle),
      var(--bg-panel) 60%
    );
  }
  .card.info {
    border-color: var(--accent-blue-border-subtle);
    border-left-color: var(--accent-blue);
    background: linear-gradient(
      90deg,
      var(--accent-blue-bg-subtle),
      var(--bg-panel) 60%
    );
  }

  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
  }

  .agent-link {
    background: transparent;
    border: none;
    padding: 0;
    font-family: inherit;
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    cursor: pointer;
    letter-spacing: 0.3px;
  }
  .agent-link:hover {
    color: var(--accent-light);
    text-decoration: underline;
  }

  .level-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-tertiary, var(--text-secondary));
    letter-spacing: 0.8px;
    text-transform: uppercase;
  }
  .card.error .level-label { color: var(--error); }
  .card.warning .level-label { color: var(--warning); }
  .card.info .level-label { color: var(--accent-blue); }

  .count {
    font-size: 10px;
    color: var(--text-secondary);
    background: var(--bg-panel-2);
    border-radius: 10px;
    padding: 1px 6px;
    letter-spacing: 0.3px;
  }

  .dismiss {
    margin-left: auto;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: 14px;
    line-height: 1;
    padding: 2px 6px;
    cursor: pointer;
    border-radius: 4px;
  }
  .dismiss:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .message {
    font-size: 12px;
    color: var(--text-primary);
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .more {
    pointer-events: auto;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    padding: 4px 12px;
    font-family: inherit;
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
    align-self: flex-end;
  }
  .more:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }
</style>
