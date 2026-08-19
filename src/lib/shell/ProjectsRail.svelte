<script lang="ts">
  /**
   * Left rail for the PROJECTS lens — projects with their recent
   * conversations (any agent's), "(more)" expands a project to its full
   * list. The counterpart of AgentRail: the roster is who, this is where.
   * Clicking a conversation focuses its agent on that session and jumps
   * back to the Agents lens.
   */
  import { invoke } from "$lib/api";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { viewStore } from "./viewStore.svelte";
  import { formatCost } from "$lib/format";
  import Avatar from "$lib/ui/Avatar.svelte";

  const RECENT = 4;

  interface ConversationOverview {
    id: string;
    agentId: string;
    agentName: string;
    agentArchived: boolean;
    projectId: string | null;
    projectName: string | null;
    model: string | null;
    provider: string | null;
    startedAt: string;
    endedAt: string | null;
    messageCount: number;
    totalTokens: number;
    totalCost: number;
    title: string | null;
    preview: string | null;
  }

  interface Group {
    key: string;
    name: string;
    rows: ConversationOverview[];
  }

  let collapsed = $derived(agentStore.sidebarCollapsed);

  let rows: ConversationOverview[] = $state([]);
  let error = $state("");
  let expanded = $state<Set<string>>(new Set());
  /** Collapsed project groups (persisted). */
  let collapsedGroups = $state<Set<string>>(new Set());
  /** Pinned conversation ids in pin order (persisted). Pinned rows sort to
   *  the top of their group and are always visible past the RECENT cut. */
  let pinned = $state<string[]>([]);

  const COLLAPSED_KEY = "projectsRail.collapsedGroups";
  const PINNED_KEY = "projectsRail.pinned";

  // One-time restore of collapse/pin prefs.
  $effect(() => {
    (async () => {
      try {
        const c = await invoke<string | null>("db_get_ui_state", { key: COLLAPSED_KEY });
        if (c) collapsedGroups = new Set(JSON.parse(c) as string[]);
      } catch {}
      try {
        const p = await invoke<string | null>("db_get_ui_state", { key: PINNED_KEY });
        if (p) pinned = JSON.parse(p) as string[];
      } catch {}
    })();
  });

  function toggleGroupCollapsed(key: string) {
    const next = new Set(collapsedGroups);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    collapsedGroups = next;
    invoke("db_set_ui_state", { key: COLLAPSED_KEY, value: JSON.stringify([...next]) }).catch(() => {});
  }

  function togglePin(id: string, ev: Event) {
    ev.stopPropagation();
    const next = pinned.includes(id) ? pinned.filter((p) => p !== id) : [...pinned, id];
    pinned = next;
    invoke("db_set_ui_state", { key: PINNED_KEY, value: JSON.stringify(next) }).catch(() => {});
  }

  function isPinned(id: string): boolean {
    return pinned.includes(id);
  }

  async function refresh() {
    error = "";
    try {
      rows = await invoke<ConversationOverview[]>("db_list_conversations");
    } catch (e) {
      error = String(e);
      rows = [];
    }
  }

  // Refresh on mount and whenever any agent's active session changes (new
  // session / continue create or reorder rows).
  $effect(() => {
    for (const a of agentStore.agents) a.sessionId;
    refresh();
  });

  // Groups ordered by their newest conversation (rows arrive newest-first);
  // project-less conversations collect in a trailing "No project" bucket.
  let groups = $derived.by<Group[]>(() => {
    const byProject = new Map<string, Group>();
    const loose: Group = { key: "none", name: "No project", rows: [] };
    for (const r of rows) {
      if (r.projectId) {
        let g = byProject.get(r.projectId);
        if (!g) {
          g = { key: r.projectId, name: r.projectName ?? r.projectId, rows: [] };
          byProject.set(r.projectId, g);
        }
        g.rows.push(r);
      } else {
        loose.rows.push(r);
      }
    }
    const result = [...byProject.values()];
    if (loose.rows.length > 0) result.push(loose);
    return result;
  });

  function toggleExpand(key: string) {
    const next = new Set(expanded);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expanded = next;
  }

  function label(r: ConversationOverview): string {
    return r.title || r.preview || "Untitled session";
  }

  function formatDate(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleDateString([], { month: "short", day: "numeric" }) +
        " " + d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    } catch {
      return iso;
    }
  }

  async function open(r: ConversationOverview) {
    const agent = agentStore.getAgent(r.agentId);
    if (!agent) return; // archived + hidden by the Active filter
    agentStore.selectAgent(r.agentId);
    // switchSession handles scope: sleeping agents get re-pointed, live ones
    // switch in place, and cross-project targets respawn in their cwd.
    await agentStore.switchSession(r.agentId, r.id);
    viewStore.setView("agents");
  }

  /** New conversation in this project, with the currently active agent. */
  async function newConversationIn(projectId: string, ev: Event) {
    ev.stopPropagation();
    const project = agentStore.projects.find((p) => p.id === projectId);
    const agentId = agentStore.activeTabId;
    if (!project || !agentId) return;
    await agentStore.newConversation(agentId, project.rootPath);
    viewStore.setView("agents");
  }
</script>

{#if collapsed}
  <div class="rail collapsed">
    <button class="tab" title="Show projects" onclick={() => agentStore.toggleSidebarCollapsed()}>
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M6 3l5 5-5 5" />
      </svg>
    </button>
  </div>
{:else}
  <aside class="rail">
    <div class="rail-head">
      <span class="title">Projects</span>
      <span class="count mono">{groups.length}</span>
      <div class="grow"></div>
      <button class="collapse" title="Hide projects" onclick={() => agentStore.toggleSidebarCollapsed()}>
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M10 3l-5 5 5 5" />
        </svg>
      </button>
    </div>

    <div class="rail-body">
      {#if error}<div class="err">{error}</div>{/if}
      {#if groups.length === 0}
        <div class="rail-empty">No conversations yet. Talk to an agent inside a project and it appears here.</div>
      {:else}
        {#each groups as group (group.key)}
          {@const isOpen = expanded.has(group.key)}
          {@const isCollapsed = collapsedGroups.has(group.key)}
          {@const pinnedRows = pinned
            .map((id) => group.rows.find((r) => r.id === id))
            .filter((r): r is ConversationOverview => !!r)}
          {@const unpinnedRows = group.rows.filter((r) => !isPinned(r.id))}
          {@const visible = [...pinnedRows, ...(isOpen ? unpinnedRows : unpinnedRows.slice(0, RECENT))]}
          <div class="group">
            <div class="group-head">
              <button class="group-label" onclick={() => toggleGroupCollapsed(group.key)} aria-expanded={!isCollapsed}>
                <svg class="chev" class:closed={isCollapsed} viewBox="0 0 16 16" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
                  <path d="M4 6l4 4 4-4" />
                </svg>
                <span class="slash">/</span>{group.name}
                <span class="group-count mono">{group.rows.length}</span>
              </button>
              {#if group.key !== "none" && agentStore.activeTabId}
                <button
                  class="gplus"
                  title="New conversation in {group.name} (active agent)"
                  aria-label="New conversation in {group.name}"
                  onclick={(e) => newConversationIn(group.key, e)}
                >
                  <svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 3v10M3 8h10" /></svg>
                </button>
              {/if}
            </div>
            {#if !isCollapsed}
              <div class="group-rows">
                {#each visible as r (r.id)}
                  {@const agent = agentStore.getAgent(r.agentId)}
                  {@const rowPinned = isPinned(r.id)}
                  <div class="row" class:unavailable={!agent} class:pinned={rowPinned}>
                    <button
                      class="head"
                      title={!agent ? `${r.agentName} is archived` : label(r)}
                      onclick={() => open(r)}
                    >
                      {#if agent}
                        <Avatar
                          name={agent.name}
                          size={18}
                          avatarType={agent.avatarType}
                          avatarPath={agent.avatarPath}
                          provider={agent.provider}
                        />
                      {:else}
                        <span class="ghost-avatar mono">{r.agentName.slice(0, 1).toUpperCase()}</span>
                      {/if}
                      <span class="title-wrap">
                        <span class="rtitle">{label(r)}</span>
                        <span class="meta mono">
                          {r.agentName}
                          · {formatDate(r.startedAt)}
                          · {r.messageCount} msg{r.messageCount === 1 ? "" : "s"}
                          {#if formatCost(r.totalCost)}· {formatCost(r.totalCost)}{/if}
                          {#if r.agentArchived}· archived{/if}
                        </span>
                      </span>
                    </button>
                    <button
                      class="pin"
                      class:on={rowPinned}
                      title={rowPinned ? "Unpin conversation" : "Pin conversation"}
                      aria-label={rowPinned ? "Unpin conversation" : "Pin conversation"}
                      aria-pressed={rowPinned}
                      onclick={(e) => togglePin(r.id, e)}
                    >
                      <svg viewBox="0 0 16 16" width="11" height="11" fill={rowPinned ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.4">
                        <path d="M6 2h4l-.5 4 2 2v1H4.5v-1l2-2L6 2z" /><path d="M8 9v5" />
                      </svg>
                    </button>
                  </div>
                {/each}
                {#if unpinnedRows.length > RECENT}
                  <button class="more" onclick={() => toggleExpand(group.key)}>
                    {isOpen ? "less" : `more (${unpinnedRows.length - RECENT})`}
                  </button>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </aside>
{/if}

<style>
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

  .collapse {
    width: 24px; height: 24px; display: flex; align-items: center; justify-content: center;
    background: transparent; border: none; color: var(--text-muted); cursor: pointer; border-radius: var(--r-sm);
  }
  .collapse:hover { background: var(--bg-raised); color: var(--text-primary); }

  .rail-body { flex: 1; min-height: 0; overflow-y: auto; padding: 0 var(--s2) var(--s3); }
  .rail-empty { font-size: 11px; color: var(--text-muted); padding: var(--s3); line-height: 1.5; }
  .err { padding: var(--s2) var(--s3); font-size: 11px; color: var(--status-error); }

  .group { display: flex; flex-direction: column; gap: 2px; margin-top: var(--s2); }
  .group-head { display: flex; align-items: center; gap: 2px; }
  .group-head .gplus {
    width: 20px; height: 20px; flex: none;
    display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: var(--r-sm);
    color: var(--text-muted); cursor: pointer; opacity: 0;
  }
  .group-head:hover .gplus { opacity: 1; }
  .group-head .gplus:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .group-label {
    display: flex; align-items: center; gap: var(--s2); flex: 1; min-width: 0;
    font: inherit; font-size: 9.5px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase;
    color: var(--text-muted); padding: var(--s2) var(--s2) var(--s1);
    background: none; border: none; border-radius: var(--r-sm); cursor: pointer; text-align: left;
  }
  .group-label:hover { color: var(--text-secondary); background: var(--bg-raised); }
  .group-label .chev { flex: none; transition: transform 0.12s; }
  .group-label .chev.closed { transform: rotate(-90deg); }
  .group-label .slash { color: var(--accent); font-weight: 700; }
  .group-label .group-count { margin-left: auto; text-transform: none; letter-spacing: 0; }
  .group-rows { display: flex; flex-direction: column; gap: 1px; }

  .row {
    display: flex; align-items: stretch; min-width: 0;
    border: 1px solid transparent; border-radius: var(--r-sm);
  }
  .row:hover { background: var(--bg-raised); }
  .row.unavailable { opacity: 0.55; }
  .row .head {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: var(--s2);
    background: none; border: none; padding: var(--s2); cursor: pointer;
    text-align: left; font: inherit; color: var(--text-secondary);
  }
  .row.unavailable .head { cursor: default; }
  .row .pin {
    width: 22px; align-self: center; height: 22px; flex: none; margin-right: var(--s1);
    display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: var(--r-sm);
    color: var(--text-muted); cursor: pointer; opacity: 0;
  }
  .row:hover .pin, .row .pin.on { opacity: 1; }
  .row .pin:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .row .pin.on { color: var(--accent); }

  .ghost-avatar {
    width: 18px; height: 18px; flex: none; border-radius: 50%;
    display: inline-flex; align-items: center; justify-content: center;
    font-size: 9px; color: var(--text-muted);
    background: var(--bg-base); border: 1px solid var(--border-subtle);
  }

  .title-wrap { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .rtitle {
    font-size: 12px; color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .meta { font-size: 9.5px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .more {
    align-self: flex-start;
    font: inherit; font-size: 10px; color: var(--text-muted);
    background: none; border: none; cursor: pointer;
    padding: 2px var(--s2); border-radius: var(--r-sm);
  }
  .more:hover { color: var(--text-secondary); background: var(--bg-raised); }
</style>
