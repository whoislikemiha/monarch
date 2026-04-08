<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { AgentConfig, ShadowGrade } from "./types";
  import { SHADOW_GRADES } from "./types";

  let {
    onspawn,
    oncancel,
  }: {
    onspawn: (config: AgentConfig) => void;
    oncancel: () => void;
  } = $props();

  let modelInput = $state("");
  let thinkingLevel = $state("off");
  let cwd = $state("/home/miha");

  // Shadow identity
  let shadowName = $state("");
  let shadowTitle = $state("");
  let shadowGrade: ShadowGrade = $state("Knight");

  const thinkingLevels = ["off", "minimal", "low", "medium", "high", "xhigh"];

  const providers = [
    { label: "Anthropic", value: "anthropic" },
    { label: "OpenAI", value: "openai" },
    { label: "OpenRouter", value: "openrouter" },
  ];

  let selectedProvider = $state("openrouter");

  // Model list from backend
  interface ModelInfo {
    id: string;
    name: string;
    provider: string;
  }

  let allModels: ModelInfo[] = $state([]);
  let modelsLoading = $state(false);
  let showDropdown = $state(false);
  let highlightedIndex = $state(-1);
  let modelInputEl: HTMLInputElement | undefined = $state(undefined);

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
    openai: [
      { id: "gpt-4.1", name: "GPT-4.1", provider: "openai" },
      { id: "gpt-4.1-mini", name: "GPT-4.1 Mini", provider: "openai" },
      { id: "o3", name: "o3", provider: "openai" },
      { id: "o4-mini", name: "o4 Mini", provider: "openai" },
      { id: "gpt-4o", name: "GPT-4o", provider: "openai" },
    ],
  };

  async function fetchModels(provider: string) {
    modelsLoading = true;
    try {
      allModels = await invoke<ModelInfo[]>("get_models", { provider });
    } catch {
      // Tauri not available (browser mode) — try direct fetch for OpenRouter, else fallback
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
    modelsLoading = false;
  }

  // Fetch models when provider changes
  $effect(() => {
    fetchModels(selectedProvider);
    modelInput = "";
  });

  function selectModel(model: ModelInfo) {
    modelInput = model.id;
    showDropdown = false;
    highlightedIndex = -1;
  }

  function handleModelKeydown(e: KeyboardEvent) {
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

  function handleSpawn() {
    const trimmed = modelInput.trim();
    const provider = selectedProvider;
    const model = trimmed || undefined;

    const config: AgentConfig = {
      provider,
      model,
      thinkingLevel: thinkingLevel !== "off" ? thinkingLevel : undefined,
      cwd: cwd || undefined,
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

    <div class="field model-field">
      <label class="label" for="model-input">Model</label>
      <div class="model-input-wrap">
        <input
          id="model-input"
          type="text"
          bind:this={modelInputEl}
          bind:value={modelInput}
          placeholder={modelsLoading ? "Loading models..." : "Search models..."}
          onfocus={() => (showDropdown = true)}
          onblur={() => setTimeout(() => (showDropdown = false), 200)}
          onkeydown={handleModelKeydown}
          oninput={() => { showDropdown = true; highlightedIndex = 0; }}
          autocomplete="off"
        />
        {#if modelsLoading}
          <span class="loading-indicator"></span>
        {/if}
      </div>
      {#if showDropdown && filteredModels().length > 0}
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
    z-index: 100;
  }

  .dialog {
    background: var(--bg-panel, #171126);
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 12px;
    padding: 24px;
    width: 480px;
    max-width: 90vw;
    display: flex;
    flex-direction: column;
    gap: 16px;
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
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }

  .flex-grow {
    flex: 2;
  }

  input,
  select {
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
</style>
