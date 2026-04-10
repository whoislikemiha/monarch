<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { AgentConfig, AgentTemplate, Project, ShadowGrade } from "./types";
  import { SHADOW_GRADES } from "./types";

  let {
    onspawn,
    oncancel,
    projects = [],
  }: {
    onspawn: (config: AgentConfig) => void;
    oncancel: () => void;
    projects?: Project[];
  } = $props();

  let modelInput = $state("");
  let thinkingLevel = $state("off");
  let cwd = $state("/home/miha");
  function formatCtxTokens(n: number): string {
    if (n >= 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + "M";
    if (n >= 1024) return (n / 1024).toFixed(n % 1024 === 0 ? 0 : 1) + "k";
    return String(n);
  }

  // Shadow identity
  let shadowName = $state("");
  let shadowTitle = $state("");
  let shadowGrade: ShadowGrade = $state("Knight");

  const thinkingLevels = ["off", "minimal", "low", "medium", "high", "xhigh"];

  // LM Studio runs on the user's localhost, so it's only reachable from the
  // Tauri desktop app — not from a browser dev context.
  const isTauri = typeof (window as any).__TAURI_INTERNALS__ !== "undefined";

  const providers = [
    { label: "Anthropic", value: "anthropic" },
    { label: "OpenAI Codex", value: "openai-codex" },
    { label: "OpenRouter", value: "openrouter" },
    ...(isTauri ? [{ label: "LM Studio", value: "lmstudio" }] : []),
  ];

  // Providers whose model lists are fetched over the network and benefit
  // from an explicit refresh action.
  const REFRESHABLE_PROVIDERS = new Set(["openrouter", "lmstudio"]);

  let selectedProvider = $state("openrouter");

  // Agent templates
  let templates: AgentTemplate[] = $state([]);
  let saveAsTemplate = $state(false);

  // Model list from backend
  interface ModelInfo {
    id: string;
    name: string;
    provider: string;
    // LM Studio only — live `loaded_context_length` as reported by the
    // native `/api/v0/models` endpoint. Absent on the `/v1/models` fallback
    // path and for non-LM-Studio providers.
    contextWindow?: number;
  }

  interface ProviderAuthStatus {
    provider: string;
    checked: boolean;
    configured: boolean;
    source?: string | null;
    message: string;
  }

  interface DetectedProject {
    rootPath: string;
    name: string;
    projectId?: string | null;
    hasInstructions: boolean;
  }

  let allModels: ModelInfo[] = $state([]);
  let modelsLoading = $state(false);
  let modelsError: string | null = $state(null);
  let modelFetchToken = 0;
  let authLoading = $state(false);
  let authStatus: ProviderAuthStatus | null = $state(null);
  let detectedProject: DetectedProject | null = $state(null);
  let showDropdown = $state(false);
  let highlightedIndex = $state(-1);
  let modelInputEl: HTMLInputElement | undefined = $state(undefined);
  let fixedModelId = $derived(selectedProvider === "openai-codex" ? "gpt-5.4" : "");

  // Fuzzy filtered models — each space-separated term must match somewhere
  let filteredModels = $derived(() => {
    const query = modelInput.toLowerCase().trim();
    if (!query) return allModels.slice(0, 50);
    const terms = query.split(/\s+/).filter(Boolean);
    return allModels
      .filter((m) => {
        const haystack = (m.id + " " + m.name).toLowerCase();
        return terms.every((t) => haystack.includes(t));
      })
      .slice(0, 50);
  });

  // Hardcoded fallback for when Tauri IPC isn't available (browser mode)
  const FALLBACK_MODELS: Record<string, ModelInfo[]> = {
    anthropic: [
      { id: "claude-opus-4-6", name: "Claude Opus 4.6", provider: "anthropic" },
      { id: "claude-sonnet-4-5", name: "Claude Sonnet 4.5", provider: "anthropic" },
      { id: "claude-haiku-4-5", name: "Claude Haiku 4.5", provider: "anthropic" },
    ],
    "openai-codex": [
      { id: "gpt-5.4", name: "GPT-5.4", provider: "openai-codex" },
    ],
  };

  async function fetchModels(provider: string) {
    const token = ++modelFetchToken;
    modelsLoading = true;
    modelsError = null;
    try {
      const fetched = await invoke<ModelInfo[]>("get_models", { provider });
      if (token !== modelFetchToken) return; // stale — provider changed
      allModels = fetched;
    } catch (err) {
      if (token !== modelFetchToken) return;
      const message = err instanceof Error ? err.message : String(err);
      if (isTauri) {
        // Real backend error — surface it to the user (common for LM Studio).
        modelsError = message;
        allModels = [];
      } else {
        // Tauri unavailable (browser mode) — use fallbacks so the dev UI still works.
        if (provider === "openrouter") {
          try {
            const resp = await fetch("https://openrouter.ai/api/v1/models");
            const json = await resp.json();
            allModels = json.data.map((m: any) => ({ id: m.id, name: m.name, provider: "openrouter" }));
          } catch {
            allModels = [];
          }
        } else {
          allModels = FALLBACK_MODELS[provider] || [];
        }
      }
    } finally {
      if (token === modelFetchToken) {
        modelsLoading = false;
      }
    }
  }

  async function fetchAuthStatus(provider: string) {
    authLoading = true;
    try {
      authStatus = await invoke<ProviderAuthStatus>("get_provider_auth_status", { provider });
    } catch {
      authStatus = null;
    }
    authLoading = false;
  }

  async function loadTemplates() {
    if (!isTauri) return;
    try {
      templates = await invoke<AgentTemplate[]>("db_list_agent_templates");
    } catch {
      templates = [];
    }
  }

  onMount(() => {
    loadTemplates();
  });

  function applyTemplate(t: AgentTemplate) {
    if (t.provider) selectedProvider = t.provider;
    // The provider $effect resets modelInput, so defer model + other fields
    // until after the current microtask so they stick.
    queueMicrotask(() => {
      if (t.model) modelInput = t.model;
      if (t.thinkingLevel) thinkingLevel = t.thinkingLevel;
      if (t.cwd) cwd = t.cwd;
      shadowName = t.shadowName ?? "";
      shadowTitle = t.shadowTitle ?? "";
      if (t.shadowGrade) shadowGrade = t.shadowGrade as ShadowGrade;
    });
  }

  async function persistCurrentAsTemplate() {
    const name = shadowName.trim();
    if (!name || !isTauri) return;
    const now = new Date().toISOString();
    const template: AgentTemplate = {
      id: `tpl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      name,
      provider: selectedProvider,
      model: (fixedModelId || modelInput.trim()) || null,
      thinkingLevel,
      cwd: cwd || null,
      shadowName: name,
      shadowTitle: shadowTitle.trim() || null,
      shadowGrade,
      createdAt: now,
      updatedAt: now,
    };
    try {
      await invoke("db_save_agent_template", { template });
    } catch {
      // Swallow — spawning should not block on template save failures.
    }
  }

  async function deleteTemplate(id: string, e: MouseEvent) {
    e.stopPropagation();
    if (!isTauri) return;
    try {
      await invoke("db_delete_agent_template", { templateId: id });
      await loadTemplates();
    } catch {}
  }

  async function detectProject(path: string) {
    try {
      detectedProject = await invoke<DetectedProject | null>("detect_project", { cwd: path });
    } catch {
      detectedProject = null;
    }
  }

  // Fetch models when provider changes
  $effect(() => {
    const provider = selectedProvider;
    // Reset UI state first so no stale list/highlight bleeds in from the
    // previous provider before the new fetch resolves.
    allModels = [];
    modelsError = null;
    modelInput = fixedModelId || "";
    showDropdown = false;
    highlightedIndex = -1;
    fetchModels(provider);
    fetchAuthStatus(provider);
  });

  // The LM Studio ModelInfo currently matching the typed id, if any. Drives
  // the read-only context display and the value sent on spawn.
  let selectedLmStudioModel = $derived.by(() => {
    if (selectedProvider !== "lmstudio") return undefined;
    const id = modelInput.trim();
    if (!id) return undefined;
    return allModels.find((m) => m.id === id);
  });

  function refreshModels() {
    fetchModels(selectedProvider);
  }

  // Detect project when cwd changes
  $effect(() => {
    if (cwd.trim()) {
      detectProject(cwd.trim());
    } else {
      detectedProject = null;
    }
  });

  function selectModel(model: ModelInfo) {
    modelInput = model.id;
    showDropdown = false;
    highlightedIndex = -1;
  }

  function handleModelKeydown(e: KeyboardEvent) {
    if (fixedModelId) {
      return;
    }

    const models = filteredModels();
    if (!showDropdown || models.length === 0) {
      if (e.key === "ArrowDown" && allModels.length > 0) {
        showDropdown = true;
        highlightedIndex = 0;
        e.preventDefault();
      }
      return;
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        highlightedIndex = Math.min(highlightedIndex + 1, models.length - 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        highlightedIndex = Math.max(highlightedIndex - 1, 0);
        break;
      case "Enter":
        if (highlightedIndex >= 0 && highlightedIndex < models.length) {
          e.preventDefault();
          e.stopPropagation();
          selectModel(models[highlightedIndex]);
        }
        break;
      case "Escape":
        showDropdown = false;
        highlightedIndex = -1;
        break;
    }
  }

  async function handleSpawn() {
    if (saveAsTemplate && shadowName.trim()) {
      await persistCurrentAsTemplate();
    }

    const trimmed = modelInput.trim();
    const provider = selectedProvider;
    const model = fixedModelId || trimmed || undefined;

    // LM Studio: take the auto-detected value straight from the discovered
    // model entry. No user override path — if discovery didn't populate a
    // value (older LM Studio, model not in list), send nothing and let the
    // sidecar apply its default context window.
    const detectedCtx = selectedLmStudioModel?.contextWindow;
    const config: AgentConfig = {
      provider,
      model,
      thinkingLevel: thinkingLevel !== "off" ? thinkingLevel : undefined,
      cwd: cwd || undefined,
      contextWindow:
        provider === "lmstudio" && typeof detectedCtx === "number" && detectedCtx > 0
          ? Math.floor(detectedCtx)
          : undefined,
    };

    // Attach shadow identity if a name is provided
    const sName = shadowName.trim();
    if (sName) {
      config.shadow = {
        shadowName: sName,
        shadowTitle: shadowTitle.trim() || sName,
        shadowGrade: shadowGrade,
      };
    }

    onspawn(config);
  }

  async function browseFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: cwd || undefined,
      title: "Select Working Directory",
    });
    if (selected && typeof selected === "string") {
      cwd = selected;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !showDropdown) oncancel();
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) handleSpawn();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={oncancel} role="presentation">
  <div
    class="dialog"
    onclick={(e: MouseEvent) => e.stopPropagation()}
    role="dialog"
    tabindex="-1"
  >
    <h2>Extract Shadow</h2>

    {#if templates.length > 0}
      <div class="section">
        <span class="label">Templates</span>
        <div class="template-chips">
          {#each templates as t (t.id)}
            <button
              class="template-chip"
              onclick={() => applyTemplate(t)}
              title={`${t.provider ?? "?"} / ${t.model ?? "?"}`}
              type="button"
            >
              <span class="template-chip-name">{t.name}</span>
              <!-- svelte-ignore a11y_consider_explicit_label -->
              <span
                class="template-chip-del"
                onclick={(e) => deleteTemplate(t.id, e)}
                onkeydown={(e) => { if (e.key === "Enter") deleteTemplate(t.id, e as unknown as MouseEvent); }}
                role="button"
                tabindex="0"
                title="Delete template"
              >×</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="section">
      <span class="label">Provider</span>
      <div class="preset-grid">
        {#each providers as p}
          <button
            class="preset-btn"
            class:active={selectedProvider === p.value}
            onclick={() => (selectedProvider = p.value)}
          >
            {p.label}
          </button>
        {/each}
      </div>
    </div>

    {#if authLoading || authStatus}
      <div
        class="auth-status"
        class:ok={!!authStatus?.checked && !!authStatus?.configured}
        class:warn={!!authStatus?.checked && !authStatus?.configured}
        class:neutral={!authStatus?.checked}
      >
        <span class="auth-status-label">
          {#if authLoading}
            Checking auth...
          {:else if authStatus?.checked && authStatus?.configured}
            Auth ready
          {:else if authStatus?.checked}
            Auth missing
          {:else}
            Auth not checked
          {/if}
        </span>
        {#if !authLoading && authStatus}
          <span class="auth-status-text">{authStatus.message}</span>
        {/if}
      </div>
    {/if}

    <div class="field model-field">
      <label class="label" for="model-input">Model</label>
      <div class="model-input-wrap">
        <input
          id="model-input"
          type="text"
          bind:this={modelInputEl}
          bind:value={modelInput}
          placeholder={fixedModelId
            ? "Uses your Pi Codex login"
            : modelsLoading
              ? "Loading models..."
              : modelsError
                ? "Provider unreachable — see hint below"
                : allModels.length === 0
                  ? "No models available"
                  : "Search models..."}
          readonly={!!fixedModelId}
          onfocus={() => { if (!fixedModelId) showDropdown = true; }}
          onblur={() => setTimeout(() => (showDropdown = false), 200)}
          onkeydown={handleModelKeydown}
          oninput={() => { if (!fixedModelId) { showDropdown = true; highlightedIndex = 0; } }}
          autocomplete="off"
        />
        {#if modelsLoading}
          <span class="loading-indicator"></span>
        {:else if !fixedModelId && REFRESHABLE_PROVIDERS.has(selectedProvider)}
          <button
            class="refresh-btn"
            onmousedown={(e: MouseEvent) => { e.preventDefault(); refreshModels(); }}
            title="Refresh model list"
            type="button"
          >
            ↻
          </button>
        {/if}
      </div>
      {#if fixedModelId}
        <div class="field-hint">
          Uses Pi's existing `openai-codex` auth and locks this provider to GPT-5.4.
        </div>
      {/if}
      {#if !fixedModelId && modelsError}
        <div class="model-error">
          <span class="model-error-label">Can't reach provider</span>
          <span class="model-error-text">{modelsError}</span>
          <button class="model-error-retry" onclick={refreshModels} type="button">
            Retry
          </button>
        </div>
      {:else if !fixedModelId && !modelsLoading && allModels.length === 0}
        <div class="field-hint">
          No models found for this provider.
        </div>
      {/if}
      {#if selectedProvider === "lmstudio"}
        <div class="lmstudio-context">
          <span class="label">
            Context window
            {#if selectedLmStudioModel?.contextWindow}
              <span class="lmstudio-ctx-value">
                {formatCtxTokens(selectedLmStudioModel.contextWindow)}
              </span>
            {/if}
          </span>
          <div class="field-hint">
            {#if selectedLmStudioModel?.contextWindow}
              Auto-detected from LM Studio.
            {:else if modelInput.trim()}
              No context length reported for this model. Sidecar default will be used.
            {:else}
              Pick a loaded model above to see its context window.
            {/if}
          </div>
        </div>
      {/if}
      {#if !fixedModelId && showDropdown && filteredModels().length > 0}
        <div class="model-dropdown">
          {#each filteredModels() as model, i (model.id)}
            <button
              class="model-option"
              class:highlighted={i === highlightedIndex}
              class:selected={modelInput === model.id}
              onmousedown={(e: MouseEvent) => { e.preventDefault(); selectModel(model); }}
              onmouseenter={() => (highlightedIndex = i)}
            >
              <span class="model-id">{model.id}</span>
              <span class="model-name">{model.name}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    {#if projects.length > 0}
      <div class="section">
        <span class="label">Project</span>
        <div class="project-chips">
          {#each projects as p (p.id)}
            <button
              class="project-chip"
              class:active={detectedProject?.rootPath === p.rootPath}
              onclick={() => { cwd = p.rootPath; }}
            >
              <span class="project-chip-slash">/</span>{p.name}
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="row">
      <div class="field">
        <label class="label" for="thinking">Thinking</label>
        <select id="thinking" bind:value={thinkingLevel}>
          {#each thinkingLevels as level}
            <option value={level}>{level}</option>
          {/each}
        </select>
      </div>
      <div class="field flex-grow">
        <label class="label" for="cwd">Working Directory</label>
        <div class="cwd-row">
          <input
            id="cwd"
            type="text"
            bind:value={cwd}
            placeholder="/home/miha/project"
          />
          <button class="browse-btn" onclick={browseFolder} title="Browse">
            ...
          </button>
        </div>
      </div>
    </div>

    {#if detectedProject}
      <div class="project-info">
        <span class="project-info-label">
          <span class="project-icon">/</span>
          {detectedProject.name}
        </span>
        <span class="project-info-path">{detectedProject.rootPath}</span>
        {#if detectedProject.hasInstructions}
          <span class="project-info-tag">Project instructions found</span>
        {/if}
      </div>
    {/if}

    <div class="section">
      <span class="label">Shadow Identity</span>
      <div class="row">
        <div class="field">
          <label class="label" for="shadow-name">Name</label>
          <input
            id="shadow-name"
            type="text"
            bind:value={shadowName}
            placeholder="e.g. Igris, Beru, Tusk"
          />
        </div>
        <div class="field">
          <label class="label" for="shadow-grade">Grade</label>
          <select id="shadow-grade" bind:value={shadowGrade}>
            {#each SHADOW_GRADES as grade}
              <option value={grade}>{grade}</option>
            {/each}
          </select>
        </div>
      </div>
      <div class="field">
        <label class="label" for="shadow-title">Title</label>
        <input
          id="shadow-title"
          type="text"
          bind:value={shadowTitle}
          placeholder="e.g. Shadow Commander, The First Shadow"
        />
      </div>
    </div>

    {#if isTauri}
      <label class="template-save-check" title={shadowName.trim() ? "" : "Set a shadow name to enable"}>
        <input
          type="checkbox"
          bind:checked={saveAsTemplate}
          disabled={!shadowName.trim()}
        />
        <span>Save as template</span>
        {#if saveAsTemplate && shadowName.trim()}
          <span class="template-save-hint">&middot; uses "{shadowName.trim()}" as the name</span>
        {/if}
      </label>
    {/if}

    <div class="actions">
      <button class="btn-cancel" onclick={oncancel}>Cancel</button>
      <button class="btn-spawn" onclick={handleSpawn}>
        Extract
        <span class="shortcut">Ctrl+Enter</span>
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    overflow-y: auto;
    z-index: 100;
  }

  .dialog {
    background: var(--bg-panel, #171126);
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 12px;
    padding: 24px;
    width: min(560px, 100%);
    max-width: min(560px, calc(100vw - 48px));
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
    box-shadow: 0 28px 80px rgba(0, 0, 0, 0.45);
  }

  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary, #f2f4f8);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .label {
    font-size: 11px;
    color: var(--text-muted, #8f7aa8);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .template-chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .template-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px 4px 10px;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 999px;
    background: var(--bg-panel-2, #201734);
    color: var(--text-secondary, #dde1e6);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .template-chip:hover {
    background: var(--bg-panel-3, #2a1e45);
    border-color: rgba(190, 149, 255, 0.4);
  }

  .template-chip-name {
    color: var(--text-primary, #f2f4f8);
  }

  .template-chip-del {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    color: var(--text-muted, #8f7aa8);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
  }

  .template-chip-del:hover {
    background: rgba(255, 120, 120, 0.15);
    color: #ffb4b4;
  }

  .template-save-check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-secondary, #dde1e6);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    user-select: none;
  }

  .template-save-check input[type="checkbox"] {
    width: auto;
    margin: 0;
    accent-color: var(--accent-purple, #be95ff);
    cursor: pointer;
  }

  .template-save-check input[type="checkbox"]:disabled {
    cursor: not-allowed;
  }

  .template-save-check input[type="checkbox"]:disabled + span {
    color: var(--text-muted, #8f7aa8);
  }

  .template-save-hint {
    color: var(--text-muted, #8f7aa8);
  }

  .lmstudio-context {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 10px;
  }

  .lmstudio-context > .label {
    display: flex;
    align-items: baseline;
    gap: 8px;
    justify-content: space-between;
    text-transform: uppercase;
  }

  .lmstudio-ctx-value {
    color: var(--accent-purple, #be95ff);
    font-size: 12px;
    text-transform: none;
    letter-spacing: 0;
  }

  .preset-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }

  .preset-btn {
    padding: 6px 8px;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 6px;
    background: var(--bg-panel-2, #201734);
    color: var(--text-secondary, #dde1e6);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .preset-btn:hover {
    background: var(--bg-panel-3, #2a1e45);
  }

  .preset-btn.active {
    border-color: var(--accent-purple, #be95ff);
    color: var(--accent-purple, #be95ff);
  }

  .project-chips {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .project-chip {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 12px;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 6px;
    background: var(--bg-panel-2, #201734);
    color: var(--text-secondary, #dde1e6);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, color 0.15s;
  }

  .project-chip:hover {
    background: var(--bg-panel-3, #2a1e45);
    border-color: rgba(190, 149, 255, 0.3);
    color: var(--text-primary, #f2f4f8);
  }

  .project-chip.active {
    background: rgba(190, 149, 255, 0.1);
    border-color: var(--accent-purple, #be95ff);
    color: var(--accent-purple, #be95ff);
  }

  .project-chip-slash {
    color: var(--accent-purple, #be95ff);
    font-weight: 700;
  }

  .project-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid rgba(190, 149, 255, 0.25);
    background: rgba(190, 149, 255, 0.06);
  }

  .project-info-label {
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--text-primary, #f2f4f8);
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .project-info .project-icon {
    color: var(--accent-purple, #be95ff);
    font-weight: 700;
  }

  .project-info-path {
    font-size: 10px;
    color: var(--text-muted, #8f7aa8);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    overflow-wrap: anywhere;
  }

  .project-info-tag {
    font-size: 10px;
    color: var(--accent-purple, #be95ff);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .field-hint {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-muted, #8f7aa8);
    line-height: 1.5;
  }

  .auth-status {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--border-subtle, #35274f);
    background: var(--bg-panel-2, #201734);
  }

  .auth-status.ok {
    border-color: rgba(61, 214, 140, 0.4);
    background: rgba(18, 53, 39, 0.45);
  }

  .auth-status.warn {
    border-color: rgba(255, 176, 32, 0.35);
    background: rgba(64, 42, 12, 0.4);
  }

  .auth-status.neutral {
    border-color: var(--border-subtle, #35274f);
  }

  .auth-status-label {
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-primary, #f2f4f8);
  }

  .auth-status-text {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-secondary, #dde1e6);
  }

  .model-field {
    position: relative;
  }

  .model-input-wrap {
    position: relative;
  }

  .loading-indicator {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    width: 12px;
    height: 12px;
    border: 2px solid var(--border-subtle, #35274f);
    border-top-color: var(--accent-purple, #be95ff);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: translateY(-50%) rotate(360deg); }
  }

  .refresh-btn {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    width: 22px;
    height: 22px;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 4px;
    background: var(--bg-panel-2, #201734);
    color: var(--text-secondary, #dde1e6);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .refresh-btn:hover {
    background: var(--bg-panel-3, #2a1e45);
    color: var(--accent-purple, #be95ff);
  }

  .model-error {
    margin-top: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid rgba(255, 120, 120, 0.4);
    background: rgba(64, 20, 20, 0.45);
  }

  .model-error-label {
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: #ffb4b4;
  }

  .model-error-text {
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-secondary, #dde1e6);
    overflow-wrap: anywhere;
  }

  .model-error-retry {
    align-self: flex-start;
    margin-top: 4px;
    padding: 4px 10px;
    border: 1px solid rgba(255, 180, 180, 0.5);
    border-radius: 4px;
    background: transparent;
    color: #ffb4b4;
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
  }

  .model-error-retry:hover {
    background: rgba(255, 180, 180, 0.1);
  }

  .model-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 200;
    max-height: 240px;
    overflow-y: auto;
    background: var(--bg-panel-2, #201734);
    border: 1px solid var(--border-subtle, #35274f);
    border-top: none;
    border-radius: 0 0 6px 6px;
    display: flex;
    flex-direction: column;
  }

  .model-option {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 10px;
    border: none;
    background: transparent;
    color: var(--text-secondary, #dde1e6);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .model-option:hover,
  .model-option.highlighted {
    background: var(--bg-panel-3, #2a1e45);
  }

  .model-option.selected {
    border-left: 2px solid var(--accent-purple, #be95ff);
  }

  .model-id {
    color: var(--text-primary, #f2f4f8);
    font-size: 12px;
  }

  .model-name {
    color: var(--text-muted, #8f7aa8);
    font-size: 10px;
  }

  .cwd-row {
    display: flex;
    gap: 6px;
  }

  .cwd-row input {
    flex: 1;
  }

  .browse-btn {
    padding: 8px 12px;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 6px;
    background: var(--bg-panel-2, #201734);
    color: var(--text-secondary, #dde1e6);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
    flex-shrink: 0;
  }

  .browse-btn:hover {
    background: var(--bg-panel-3, #2a1e45);
  }

  .row {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }

  .flex-grow {
    flex: 2;
  }

  input,
  select {
    width: 100%;
    background: var(--bg-panel-2, #201734);
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 6px;
    color: var(--text-primary, #f2f4f8);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 8px 10px;
    outline: none;
  }

  input::placeholder {
    color: var(--text-muted, #8f7aa8);
    opacity: 0.6;
  }

  input:focus,
  select:focus {
    border-color: var(--accent-purple, #be95ff);
  }

  select {
    cursor: pointer;
    appearance: none;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .btn-cancel {
    padding: 8px 16px;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary, #dde1e6);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-cancel:hover {
    background: var(--bg-panel-2, #201734);
  }

  .btn-spawn {
    padding: 8px 20px;
    border: none;
    border-radius: 8px;
    background: var(--accent-purple, #be95ff);
    color: #140d22;
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    transition: background 0.15s;
  }

  .btn-spawn:hover {
    background: #d5bbff;
  }

  .shortcut {
    font-size: 10px;
    font-weight: 400;
    opacity: 0.6;
  }

  @media (max-width: 640px) {
    .overlay {
      padding: 16px;
    }

    .dialog {
      padding: 20px;
      max-width: calc(100vw - 32px);
      max-height: calc(100vh - 32px);
    }

    .preset-grid {
      grid-template-columns: 1fr;
    }

    .actions {
      flex-wrap: wrap-reverse;
    }

    .actions button {
      width: 100%;
      justify-content: center;
    }
  }
</style>
