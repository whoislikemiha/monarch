<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "$lib/api";
  import Sidebar from "./lib/Sidebar.svelte";
  import AgentView from "./lib/AgentView.svelte";
  import SpawnDialog from "./lib/SpawnDialog.svelte";
  import TabBar from "./lib/TabBar.svelte";
  import ProjectEditor from "./lib/ProjectEditor.svelte";
  import ToolRail from "./lib/toolbox/ToolRail.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import ToolPanelStack from "./lib/toolbox/ToolPanelStack.svelte";
  import {
    liveAgentStore,
  } from "./lib/toolbox/liveAgentStore.svelte";
  import {
    persistOpenIds,
    persistWidth,
    restoreOpenIds,
    restoreWidth,
  } from "./lib/toolbox/persistence";
  import type { AgentContext } from "./lib/toolbox/types";
  import type { Project } from "./lib/types";
  import { loadKeybindings, matchBinding } from "$lib/keybindings.svelte";

  import {
    agents,
    projects,
    openTabs,
    activeTabId,
    sidebarCollapsed,
    activeAgent,
    activeProject,
    initialize,
    createAgent,
    killAgent,
    restartAgent,
    spawnStoppedAgent,
    newConversation,
    updateAgent,
    selectAgent,
    closeTab,
    switchToRecentAgent,
    switchToNextAgent,
    toggleSidebar,
    updateProjects,
    loadProjects,
    setActiveTab,
  } from "$lib/stores/agentStore.svelte";

  // --- Local UI state (not shared with children) ---
  let showSpawnDialog = $state(false);
  let showSettings = $state(false);
  let editingProject: Project | null = $state(null);
  let agentViewRef: AgentView | undefined = $state(undefined);

  // --- Toolbox state ---
  let openToolIds: string[] = $state(restoreOpenIds());
  let toolboxWidth = $state(restoreWidth());

  $effect(() => {
    persistOpenIds(openToolIds);
  });
  $effect(() => {
    persistWidth(toolboxWidth);
  });

  function toggleTool(id: string) {
    openToolIds = openToolIds.includes(id)
      ? openToolIds.filter((t) => t !== id)
      : [...openToolIds, id];
  }

  function closeTool(id: string) {
    openToolIds = openToolIds.filter((t) => t !== id);
  }

  // --- Zoom state ---
  const ZOOM_STEP = 0.05;
  const ZOOM_DEFAULT = 1.0;
  let zoomLevel = $state(ZOOM_DEFAULT);

  async function applyZoom(level: number) {
    try {
      const clamped = await invoke<number>("set_zoom", { level });
      zoomLevel = clamped;
      invoke("db_set_ui_state", { key: "zoomLevel", value: String(clamped) }).catch(() => {});
    } catch {
      // Not in Tauri (browser mode) — skip
    }
  }

  function handleWheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    const direction = e.deltaY < 0 ? 1 : -1;
    applyZoom(zoomLevel + direction * ZOOM_STEP);
  }

  onMount(async () => {
    await initialize();
    await loadKeybindings();

    // Restore zoom level
    try {
      const saved = await invoke<string | null>("db_get_ui_state", { key: "zoomLevel" });
      if (saved) {
        const level = parseFloat(saved);
        if (!isNaN(level)) applyZoom(level);
      }
    } catch {}
  });

  // --- AgentContext for toolbox ---
  let currentLive = $derived(
    activeTabId ? liveAgentStore.byAgent.get(activeTabId) ?? null : null,
  );
  let activeCustomPrompt: string | null = $state(null);
  let agentContext: AgentContext = $derived(
    activeAgent && currentLive
      ? {
          agentId: activeAgent.id,
          agent: activeAgent,
          live: currentLive,
          setup: {
            customPrompt: activeCustomPrompt,
            projectInstructions: activeProject?.instructions ?? null,
          },
        }
      : null,
  );

  // --- Keyboard handler ---
  function handleKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const inInput = target.tagName === "TEXTAREA" || target.tagName === "INPUT" || target.tagName === "SELECT";
    const inDialog = target.closest("[role=dialog]") !== null;

    // --- Always active (even in inputs/dialogs) ---

    if (matchBinding(e, "global.spawn-agent")) {
      e.preventDefault();
      showSpawnDialog = true;
      return;
    }

    if (matchBinding(e, "global.settings")) {
      e.preventDefault();
      showSettings = !showSettings;
      return;
    }

    if (matchBinding(e, "global.toggle-sidebar")) {
      e.preventDefault();
      toggleSidebar();
      return;
    }

    // Zoom (non-editable, kept as direct checks for = / + ambiguity)
    if (e.ctrlKey && (e.key === "=" || e.key === "+")) {
      e.preventDefault();
      applyZoom(zoomLevel + ZOOM_STEP);
      return;
    }
    if (e.ctrlKey && e.key === "-") {
      e.preventDefault();
      applyZoom(zoomLevel - ZOOM_STEP);
      return;
    }
    if (e.ctrlKey && e.key === "0") {
      e.preventDefault();
      applyZoom(ZOOM_DEFAULT);
      return;
    }

    // Tab switching (Ctrl+1-9)
    for (let i = 1; i <= 9; i++) {
      if (matchBinding(e, `nav.tab-${i}`)) {
        e.preventDefault();
        if (i - 1 < openTabs.length) {
          setActiveTab(openTabs[i - 1]);
        }
        return;
      }
    }

    // Recent agent (Ctrl+Tab)
    if (matchBinding(e, "nav.recent-agent")) {
      e.preventDefault();
      switchToRecentAgent();
      return;
    }

    // Next agent (Ctrl+PageDown)
    if (matchBinding(e, "nav.next-agent")) {
      e.preventDefault();
      switchToNextAgent();
      return;
    }

    // --- Only when not in input/dialog ---
    if (inInput || inDialog) return;

    if (matchBinding(e, "global.focus-chat") || matchBinding(e, "global.focus-chat-alt")) {
      e.preventDefault();
      agentViewRef?.focusInput();
      return;
    }

    // Escape — unfocus (not in registry, universal behavior)
    if (e.key === "Escape") {
      (document.activeElement as HTMLElement)?.blur();
      return;
    }

    // Abort agent (Ctrl+C when no text selected)
    if (matchBinding(e, "global.abort-agent")) {
      const selection = window.getSelection();
      if (selection && selection.toString().length > 0) return;
      e.preventDefault();
      if (activeAgent) {
        invoke("send_command", {
          id: activeAgent.id,
          commandJson: JSON.stringify({ type: "abort" }),
        });
      }
      return;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} onwheel={handleWheel} />

<main class="app">
  <Sidebar
    collapsed={sidebarCollapsed}
    oncreate={() => (showSpawnDialog = true)}
    oneditproject={(project) => { editingProject = project; }}
    onsavetemplate={async (source) => {
      const now = new Date().toISOString();
      const template = {
        id: `tpl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        name: source.name,
        provider: source.provider ?? null,
        model: source.model ?? null,
        thinkingLevel: source.thinkingLevel ?? null,
        cwd: source.cwd ?? null,
        shadowName: source.shadow?.shadowName ?? source.name,
        shadowTitle: source.shadow?.shadowTitle ?? null,
        shadowGrade: source.shadow?.shadowGrade ?? null,
        createdAt: now,
        updatedAt: now,
      };
      try {
        await invoke("db_save_agent_template", { template });
      } catch {}
    }}
  />
  <div class="main-panel">
    <TabBar />
    <div class="main-content">
      {#if activeAgent}
        {#key activeAgent.viewKey}
          <AgentView
            agent={activeAgent}
            projectName={activeProject?.name}
            bind:customPrompt={activeCustomPrompt}
            bind:this={agentViewRef}
          />
        {/key}
      {:else}
        <div class="empty-state">
          <span class="empty-icon">&gt;_</span>
          <p>Extract a shadow to begin</p>
          <p class="hint">Ctrl+N extract &middot; Ctrl+B sidebar &middot; Ctrl+1-9 switch</p>
        </div>
      {/if}
    </div>
  </div>
  <ToolPanelStack
    {openToolIds}
    {agentContext}
    width={toolboxWidth}
    onclose={closeTool}
    onresize={(w) => (toolboxWidth = w)}
  />
  <ToolRail {openToolIds} ontoggle={toggleTool} onsettings={() => (showSettings = true)} />
</main>

{#if showSpawnDialog}
  <SpawnDialog
    onspawn={(config) => {
      showSpawnDialog = false;
      createAgent(config);
    }}
    oncancel={() => (showSpawnDialog = false)}
  />
{/if}

{#if editingProject}
  <ProjectEditor
    project={editingProject}
    onclose={() => (editingProject = null)}
    onupdate={(updated) => {
      updateProjects((ps) => ps.map(p => p.id === updated.id ? updated : p));
      editingProject = updated;
    }}
  />
{/if}

{#if showSettings}
  <SettingsDialog
    onclose={() => (showSettings = false)}
    {zoomLevel}
    onzoom={applyZoom}
  />
{/if}

<style>
  .app {
    display: flex;
    width: 100vw;
    height: 100vh;
    min-width: 0;
    min-height: 100vh;
    overflow: hidden;
  }

  .main-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    position: relative;
    overflow: hidden;
  }

  .main-content {
    flex: 1;
    display: flex;
    min-width: 0;
    min-height: 0;
    position: relative;
    overflow: hidden;
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px;
    text-align: center;
    color: var(--text-muted);
  }

  .empty-icon {
    font-size: 48px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    line-height: 1;
    color: var(--accent);
  }

  .empty-state p {
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin: 0;
    max-width: 32rem;
  }

  .hint {
    margin-top: 8px !important;
    font-size: 11px !important;
    opacity: 0.6;
  }
</style>
