<script lang="ts">
  /**
   * v2 command-center shell. Owns boot, global keybindings, zoom, and the
   * persistent frame (TopBar · AgentRail · PanelHost · notifications). All
   * surface content lives in the views/workspace/board/panels trees; this file
   * stays thin.
   *
   * Settings (appearance · keybindings · memory) opens from the gear at the
   * bottom of the inspector rail or Ctrl+, — see SettingsDialog.
   */
  import { onMount } from "svelte";
  import { invoke } from "$lib/api";
  import "./lib/ui/styles/atoms.css";

  import TopBar from "./lib/shell/TopBar.svelte";
  import CommandPalette from "./lib/shell/CommandPalette.svelte";
  import AgentRail from "./lib/shell/AgentRail.svelte";
  import PanelHost from "./lib/shell/PanelHost.svelte";
  import NotificationStack from "./lib/NotificationStack.svelte";
  import SpawnDialog from "./lib/SpawnDialog.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";

  import { agentStore } from "./lib/stores/agentStore.svelte";
  import { viewStore } from "./lib/shell/viewStore.svelte";
  import { layoutStore } from "./lib/layout/layoutStore.svelte";
  import { loadKeybindings, matchBinding } from "$lib/keybindings.svelte";

  // --- Dialog state (shell-local) -------------------------------------
  let showSpawnDialog = $state(false);
  let showSettings = $state(false);
  let showPalette = $state(false);

  // --- Zoom ------------------------------------------------------------
  const ZOOM_STEP = 0.05;
  const ZOOM_DEFAULT = 1.0;
  let zoomLevel = $state(ZOOM_DEFAULT);

  async function applyZoom(level: number) {
    try {
      const clamped = await invoke<number>("set_zoom", { level });
      zoomLevel = clamped;
      invoke("db_set_ui_state", { key: "zoomLevel", value: String(clamped) }).catch(() => {});
    } catch {
      // browser mode — no Tauri window
    }
  }

  function handleWheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    applyZoom(zoomLevel + (e.deltaY < 0 ? 1 : -1) * ZOOM_STEP);
  }

  // --- Boot ------------------------------------------------------------
  agentStore.setupEffects();

  onMount(async () => {
    // Theme + active lens (viewStore.init applies the theme before paint).
    await viewStore.init();
    await layoutStore.init();
    await agentStore.init();
    await loadKeybindings();

    try {
      const saved = await invoke<string | null>("db_get_ui_state", { key: "zoomLevel" });
      if (saved) {
        const level = parseFloat(saved);
        if (!isNaN(level)) applyZoom(level);
      }
    } catch {}
  });

  // --- Keybindings -----------------------------------------------------
  let activeAgent = $derived(agentStore.getAgent(agentStore.activeTabId ?? ""));

  function handleKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const inInput =
      target.tagName === "TEXTAREA" || target.tagName === "INPUT" || target.tagName === "SELECT";
    const inDialog = target.closest("[role=dialog]") !== null;

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
    if (matchBinding(e, "global.command-palette")) {
      e.preventDefault();
      showPalette = !showPalette;
      return;
    }
    if (matchBinding(e, "global.toggle-sidebar")) {
      e.preventDefault();
      agentStore.toggleSidebarCollapsed();
      return;
    }

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

    for (let i = 1; i <= 9; i++) {
      if (matchBinding(e, `nav.tab-${i}`)) {
        e.preventDefault();
        agentStore.switchToTabIndex(i - 1);
        return;
      }
    }
    if (matchBinding(e, "nav.recent-agent")) {
      e.preventDefault();
      agentStore.switchToRecentAgent();
      return;
    }
    if (matchBinding(e, "nav.next-agent")) {
      e.preventDefault();
      agentStore.switchToNextAgent();
      return;
    }

    if (inInput || inDialog) return;

    if (e.key === "Escape") {
      (document.activeElement as HTMLElement)?.blur();
      return;
    }
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

  // --- Breadcrumb ------------------------------------------------------
  let crumbs = $derived.by(() => {
    if (viewStore.activeView === "agents") {
      return activeAgent ? ["Agents", activeAgent.name] : ["Agents"];
    }
    return ["Projects"];
  });
</script>

<svelte:window onkeydown={handleKeydown} onwheel={handleWheel} />

<main class="shell">
  <TopBar {crumbs} onCommandPalette={() => (showPalette = true)} />
  <div class="body">
    <AgentRail onextract={() => (showSpawnDialog = true)} />
    <PanelHost onsettings={() => (showSettings = true)} />
  </div>
</main>

<NotificationStack />

{#if showSpawnDialog}
  <SpawnDialog
    onspawn={(config) => {
      showSpawnDialog = false;
      agentStore.createAgent(config);
    }}
    oncancel={() => (showSpawnDialog = false)}
  />
{/if}

{#if showSettings}
  <SettingsDialog onclose={() => (showSettings = false)} {zoomLevel} onzoom={applyZoom} />
{/if}

{#if showPalette}
  <CommandPalette
    onclose={() => (showPalette = false)}
    onspawn={() => (showSpawnDialog = true)}
    onsettings={() => (showSettings = true)}
  />
{/if}

<style>
  .shell {
    display: flex;
    flex-direction: column;
    width: 100vw;
    height: 100vh;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-base);
  }
  .body {
    flex: 1;
    display: flex;
    flex-direction: row;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }
</style>
