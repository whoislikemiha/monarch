<script lang="ts">
  /**
   * ⌘K command palette. Fuzzy-searches everything the shell can do from the
   * keyboard: switch agents, toggle inspector panels, change view/theme, and
   * a few global actions. Pure frontend — every command drives an existing
   * store, so the palette needs no backend of its own.
   */
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { viewStore } from "./viewStore.svelte";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";
  import { PANELS } from "$lib/layout/panelRegistry";
  import { listThemes } from "$lib/themes";
  import { getBinding, formatBindingParts } from "$lib/keybindings.svelte";

  function shortcut(bindingId: string): string {
    const keys = getBinding(bindingId);
    return keys ? formatBindingParts(keys).join(" ") : "";
  }

  interface Props {
    onclose: () => void;
    onspawn: () => void;
    onsettings: () => void;
  }
  let { onclose, onspawn, onsettings }: Props = $props();

  interface Command {
    id: string;
    group: string;
    label: string;
    /** Right-aligned hint; rendered mono when `mono` is set (ids, shortcuts). */
    hint?: string;
    mono?: boolean;
    keywords?: string;
    run: () => void;
  }

  let query = $state("");
  let selected = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    inputEl?.focus();
  });

  const themes = listThemes();

  let commands = $derived.by<Command[]>(() => {
    const out: Command[] = [];

    for (const agent of agentStore.agents.filter((a) => !a.archivedAt)) {
      out.push({
        id: `agent:${agent.id}`,
        group: "Agents",
        label: agent.name,
        hint: agent.model,
        mono: true,
        keywords: "agent switch go",
        run: () => {
          viewStore.setView("agents");
          agentStore.selectAgent(agent.id);
        },
      });
    }

    for (const panel of PANELS) {
      const open = layoutStore.isOpen(panel.id);
      out.push({
        id: `panel:${panel.id}`,
        group: "Panels",
        label: panel.title,
        hint: open ? "Close" : "Open",
        keywords: "panel inspector dock toggle",
        run: () => layoutStore.toggle(panel.id),
      });
    }

    out.push(
      {
        id: "view:agents",
        group: "View",
        label: "Agents view",
        hint: viewStore.activeView === "agents" ? "Active" : undefined,
        keywords: "view lens switch workspace",
        run: () => viewStore.setView("agents"),
      },
      {
        id: "view:projects",
        group: "View",
        label: "Projects view",
        hint: viewStore.activeView === "projects" ? "Active" : undefined,
        keywords: "view lens switch campaign",
        run: () => viewStore.setView("projects"),
      },
    );

    for (const { id, label } of themes) {
      out.push({
        id: `theme:${id}`,
        group: "Theme",
        label: `Theme: ${label}`,
        hint: viewStore.themeId === id ? "Active" : undefined,
        keywords: "theme appearance color dark light",
        run: () => viewStore.setTheme(id),
      });
    }

    out.push(
      {
        id: "action:spawn",
        group: "Actions",
        label: "New agent…",
        hint: shortcut("global.spawn-agent"),
        mono: true,
        keywords: "create spawn new agent",
        run: onspawn,
      },
      {
        id: "action:settings",
        group: "Actions",
        label: "Settings…",
        hint: shortcut("global.settings"),
        mono: true,
        keywords: "settings preferences appearance keybindings memory",
        run: onsettings,
      },
      {
        id: "action:sidebar",
        group: "Actions",
        label: "Toggle sidebar",
        hint: shortcut("global.toggle-sidebar"),
        mono: true,
        keywords: "sidebar rail roster collapse",
        run: () => agentStore.toggleSidebarCollapsed(),
      },
    );

    return out;
  });

  /** Substring beats subsequence; earlier and shorter matches rank higher. */
  function score(q: string, cmd: Command): number {
    if (!q) return 1;
    const t = `${cmd.label} ${cmd.keywords ?? ""}`.toLowerCase();
    const idx = t.indexOf(q);
    if (idx === 0) return 100;
    if (idx > 0) return 60 - Math.min(idx, 40);
    let ti = 0;
    for (const ch of q) {
      ti = t.indexOf(ch, ti);
      if (ti === -1) return -1;
      ti++;
    }
    return 15;
  }

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return commands
      .map((cmd) => ({ cmd, s: score(q, cmd) }))
      .filter((x) => x.s >= 0)
      .sort((a, b) => b.s - a.s)
      .map((x) => x.cmd);
  });

  // Clamp/reset the selection when the result set changes.
  $effect(() => {
    void filtered;
    selected = 0;
  });

  // Keep the selected row visible.
  $effect(() => {
    void selected;
    listEl?.querySelector(".row.sel")?.scrollIntoView({ block: "nearest" });
  });

  function run(cmd: Command) {
    onclose();
    cmd.run();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (filtered.length) selected = (selected + 1) % filtered.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (filtered.length) selected = (selected - 1 + filtered.length) % filtered.length;
    } else if (e.key === "Enter") {
      e.preventDefault();
      const cmd = filtered[selected];
      if (cmd) run(cmd);
    }
  }

  // Group rows for display while keyboard nav walks the flat filtered order.
  let grouped = $derived.by(() => {
    const groups: { name: string; items: { cmd: Command; index: number }[] }[] = [];
    filtered.forEach((cmd, index) => {
      const last = groups[groups.length - 1];
      if (last && last.name === cmd.group) last.items.push({ cmd, index });
      else groups.push({ name: cmd.group, items: [{ cmd, index }] });
    });
    return groups;
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="scrim" role="presentation" onclick={onclose}>
  <div
    class="palette"
    role="dialog"
    aria-modal="true"
    aria-label="Command palette"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={onKeydown}
  >
    <div class="search">
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>
      </svg>
      <input
        bind:this={inputEl}
        bind:value={query}
        placeholder="Search agents, panels, actions…"
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <div class="results" bind:this={listEl}>
      {#if filtered.length === 0}
        <p class="none">No matches for “{query}”</p>
      {:else}
        {#each grouped as group (group.name)}
          <span class="group">{group.name}</span>
          {#each group.items as { cmd, index } (cmd.id)}
            <button
              class="row"
              class:sel={index === selected}
              onclick={() => run(cmd)}
              onmousemove={() => (selected = index)}
            >
              <span class="label">{cmd.label}</span>
              {#if cmd.hint}
                <span class="hint" class:mono={cmd.mono}>{cmd.hint}</span>
              {/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>

    <footer class="foot">
      <span><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
      <span><kbd>↵</kbd> run</span>
      <span><kbd>esc</kbd> close</span>
    </footer>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 110;
    background: var(--scrim);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 12vh var(--s5) var(--s5);
  }
  .palette {
    width: min(560px, calc(100vw - 48px));
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: 60vh;
    background: var(--bg-panel);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-lg);
    overflow: hidden;
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: var(--s3) var(--s4);
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-muted);
    flex: none;
  }
  .search input {
    flex: 1;
    min-width: 0;
    font: inherit;
    font-size: 13px;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-primary);
  }
  .search input::placeholder { color: var(--text-muted); }

  .results {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--s2);
    display: flex;
    flex-direction: column;
  }
  .group {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--text-muted);
    padding: var(--s2) var(--s2) var(--s1);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--s3);
    font: inherit;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: var(--r-sm);
    padding: var(--s2) var(--s2);
    cursor: pointer;
    color: var(--text-secondary);
  }
  .row.sel {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .row .label {
    flex: 1;
    min-width: 0;
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row .hint {
    flex: none;
    font-size: 10px;
    color: var(--text-muted);
  }
  .row .hint.mono { font-family: "JetBrains Mono", monospace; }

  .none {
    margin: 0;
    padding: var(--s4);
    font-size: 12px;
    color: var(--text-muted);
    text-align: center;
  }

  .foot {
    display: flex;
    align-items: center;
    gap: var(--s4);
    padding: var(--s2) var(--s4);
    border-top: 1px solid var(--border-subtle);
    font-size: 10px;
    color: var(--text-muted);
    flex: none;
  }
  .foot kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    margin-right: 3px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-raised);
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
    line-height: 1;
  }
</style>
