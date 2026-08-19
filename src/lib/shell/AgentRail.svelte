<script lang="ts">
  /**
   * Left rail — the agent roster. Agents are persistent and project-agnostic
   * (a conversation happens in a project; the agent doesn't live in one), so
   * the roster is a flat list. Project-grouped browsing lives in the
   * Conversations dock panel.
   */
  import type { Agent } from "$lib/types";
  import { invoke } from "$lib/api";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import AgentRow from "./AgentRow.svelte";
  import EditAgentDialog from "$lib/EditAgentDialog.svelte";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";

  interface Props {
    onextract?: () => void;
  }
  let { onextract }: Props = $props();

  let collapsed = $derived(agentStore.sidebarCollapsed);

  // --- Roster management: context menu + confirm/edit dialogs ---
  let menu = $state<{ x: number; y: number; agent: Agent } | null>(null);
  let editing = $state<Agent | null>(null);
  let confirm = $state<{ kind: "archive" | "delete"; agent: Agent } | null>(null);

  function openMenu(e: MouseEvent, agent: Agent) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, agent };
  }

  async function saveTemplate(agent: Agent) {
    const now = new Date().toISOString();
    try {
      await invoke("db_save_agent_template", {
        template: {
          id: `tpl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          name: agent.shadow?.shadowName ?? agent.name,
          provider: agent.provider ?? null,
          model: agent.model ?? null,
          thinkingLevel: agent.thinkingLevel ?? null,
          cwd: agent.cwd ?? null,
          shadowName: agent.shadow?.shadowName ?? agent.name,
          shadowTitle: agent.shadow?.shadowTitle ?? null,
          shadowGrade: agent.shadow?.shadowGrade ?? null,
          createdAt: now,
          updatedAt: now,
        },
      });
    } catch (e) {
      console.error("save template failed:", e);
    }
  }

  function runConfirm() {
    const c = confirm;
    confirm = null;
    if (!c) return;
    if (c.kind === "archive") agentStore.archiveAgent(c.agent.id);
    else agentStore.deleteAgent(c.agent.id);
  }

</script>

{#if collapsed}
  <div class="rail collapsed">
    <button class="tab" title="Show agents" onclick={() => agentStore.toggleSidebarCollapsed()}>
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M6 3l5 5-5 5" />
      </svg>
    </button>
    <button class="tab" title="Create agent" onclick={() => onextract?.()}>
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M8 3v10M3 8h10" />
      </svg>
    </button>
  </div>
{:else}
  <aside class="rail">
    <div class="rail-head">
      <span class="title">Agents</span>
      <span class="count mono">{agentStore.agents.filter((a) => !a.archivedAt).length}</span>
      <div class="grow"></div>
      <div class="filter" role="tablist" aria-label="Roster filter">
        <button
          class="filter-btn"
          class:active={!agentStore.sidebarShowAll}
          onclick={() => agentStore.setSidebarShowAll(false)}
        >Active</button>
        <button
          class="filter-btn"
          class:active={agentStore.sidebarShowAll}
          onclick={() => agentStore.setSidebarShowAll(true)}
        >All</button>
      </div>
      <button class="collapse" title="Hide agents" onclick={() => agentStore.toggleSidebarCollapsed()}>
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M10 3l-5 5 5 5" />
        </svg>
      </button>
    </div>

    <button class="extract" onclick={() => onextract?.()}>
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M8 3v10M3 8h10" />
      </svg>
      New agent
    </button>

    <div class="rail-body">
      {#if agentStore.agents.length === 0}
        <div class="rail-empty">No agents yet. Create one to begin.</div>
      {:else}
        <div class="rows">
          {#each agentStore.agents as agent (agent.id)}
            <AgentRow
              {agent}
              oncontextmenu={openMenu}
              onarchive={(a) => (confirm = { kind: "archive", agent: a })}
              onresume={(a) => agentStore.summonAgent(a.id)}
            />
          {/each}
        </div>
      {/if}
    </div>
  </aside>
{/if}

{#if menu}
  <button class="ctx-scrim" aria-label="Close menu" onclick={() => (menu = null)} oncontextmenu={(e) => { e.preventDefault(); menu = null; }}></button>
  <div class="ctx" style="left:{menu.x}px; top:{menu.y}px" role="menu">
    <button role="menuitem" onclick={() => { const a = menu!.agent; menu = null; editing = a; }}>Edit agent</button>
    <button role="menuitem" onclick={() => { const a = menu!.agent; menu = null; saveTemplate(a); }}>Save as template</button>
    {#if menu.agent.archivedAt}
      <button role="menuitem" onclick={() => { const a = menu!.agent; menu = null; agentStore.summonAgent(a.id); }}>Resume</button>
    {:else}
      <button role="menuitem" onclick={() => { const a = menu!.agent; menu = null; confirm = { kind: "archive", agent: a }; }}>Archive</button>
    {/if}
    <div class="ctx-div"></div>
    <button role="menuitem" class="danger" onclick={() => { const a = menu!.agent; menu = null; confirm = { kind: "delete", agent: a }; }}>Delete permanently</button>
  </div>
{/if}

{#if editing}
  <EditAgentDialog agent={editing} onclose={() => (editing = null)} />
{/if}

<ConfirmDialog
  open={confirm?.kind === "archive"}
  title="Archive {confirm?.agent.name}?"
  message="The agent leaves the active roster. History, sessions, and identity are preserved — resume it anytime from All."
  confirmLabel="Archive"
  onconfirm={runConfirm}
  oncancel={() => (confirm = null)}
/>
<ConfirmDialog
  open={confirm?.kind === "delete"}
  title="Permanently delete {confirm?.agent.name}?"
  message="Irreversible. All history, sessions, and stats for this agent are deleted."
  confirmLabel="Delete permanently"
  danger
  onconfirm={runConfirm}
  oncancel={() => (confirm = null)}
/>

<style>
  .ctx-scrim { position: fixed; inset: 0; z-index: 500; background: none; border: none; }
  .ctx {
    position: fixed; z-index: 501; min-width: 168px; padding: var(--s1);
    background: var(--bg-overlay); border: 1px solid var(--border-strong); border-radius: var(--r-md);
    display: flex; flex-direction: column; gap: 1px;
  }
  .ctx button {
    text-align: left; font: inherit; font-size: 12px; color: var(--text-secondary);
    background: none; border: none; border-radius: var(--r-sm); padding: 6px var(--s3); cursor: pointer;
  }
  .ctx button:hover { background: var(--bg-raised); color: var(--text-primary); }
  .ctx button.danger { color: var(--status-error); }
  .ctx button.danger:hover { background: color-mix(in srgb, var(--status-error) 16%, transparent); color: var(--status-error); }
  .ctx-div { height: 1px; background: var(--border-subtle); margin: 2px var(--s2); }

  .rail {
    width: 248px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--bg-sink);
    border-right: 1px solid var(--border-subtle);
    min-height: 0;
    user-select: none;
  }
  .rail.collapsed {
    width: 44px;
    align-items: center;
    padding-top: var(--s2);
    gap: var(--s1);
  }
  .rail.collapsed .tab {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    color: var(--text-secondary);
    cursor: pointer;
  }
  .rail.collapsed .tab:hover { background: var(--bg-raised); color: var(--text-primary); }

  .rail-head {
    display: flex;
    align-items: center;
    gap: var(--s2);
    height: 40px;
    flex: none;
    padding: 0 var(--s3);
    border-bottom: 1px solid var(--border-subtle);
  }
  .rail-head .title { font-size: 12px; font-weight: 600; letter-spacing: 0.04em; color: var(--text-primary); }
  .rail-head .count { font-size: 10px; color: var(--text-muted); }
  .rail-head .grow { flex: 1; }

  .filter { display: flex; padding: 2px; background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--r-md); }
  .filter-btn {
    font: inherit; font-size: 10.5px; font-weight: 500; color: var(--text-muted);
    background: transparent; border: none; padding: 2px var(--s2);
    border-radius: var(--r-sm); cursor: pointer;
  }
  .filter-btn:hover { color: var(--text-secondary); }
  .filter-btn.active { background: var(--bg-overlay); color: var(--text-primary); }

  .collapse {
    width: 24px; height: 24px; display: flex; align-items: center; justify-content: center;
    background: transparent; border: none; color: var(--text-muted); cursor: pointer; border-radius: var(--r-sm);
  }
  .collapse:hover { background: var(--bg-raised); color: var(--text-primary); }

  .extract {
    display: flex; align-items: center; justify-content: center; gap: var(--s2);
    margin: var(--s3); flex: none;
    font: inherit; font-size: 12px; font-weight: 500; color: var(--text-secondary);
    background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--r-md);
    padding: 7px var(--s3); cursor: pointer; transition: background 0.14s, border-color 0.14s, color 0.14s;
  }
  .extract:hover { background: var(--bg-raised); color: var(--text-primary); border-color: var(--border-strong); }

  .rail-body { flex: 1; min-height: 0; overflow-y: auto; padding: 0 var(--s2) var(--s3); }
  .rail-empty { font-size: 11px; color: var(--text-muted); padding: var(--s3); line-height: 1.5; }

  .rows { display: flex; flex-direction: column; gap: 1px; margin-top: var(--s2); }
</style>
