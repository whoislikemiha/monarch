<script lang="ts">
  import { invoke } from "$lib/api";
  import { open } from "@tauri-apps/plugin-dialog";
  import { matchBinding } from "$lib/keybindings.svelte";
  import type { AgentConfig, DetectedProject, ShadowGrade } from "./types";
  import { SHADOW_GRADES } from "./types";
  import type { AgentTemplateRow } from "./bindings";
  import { agentStore } from "./stores/agentStore.svelte";
  import ModelSelector, { type ModelsStatus } from "./ModelSelector.svelte";
  import TemplateSelector from "./TemplateSelector.svelte";
  import { commands } from "$lib/bindings";
  import { clampLevel } from "./thinking";

  let {
    onspawn,
    oncancel,
  }: {
    onspawn: (config: AgentConfig) => void;
    oncancel: () => void;
  } = $props();

  // ModelSelector-bound state
  let provider = $state("anthropic");
  let model = $state("");
  let thinkingLevel = $state("off");
  let contextWindow: number | undefined = $state(undefined);
  let modelsStatus = $state<ModelsStatus>({ loading: false, error: null, count: 0 });

  // Creating an agent is gated on a picked model only. Loading/error states
  // drive the hint copy but do not themselves disable the button — see MON-79.
  let canSpawn = $derived(!!provider && model.trim().length > 0);

  let disabledHint = $derived.by(() => {
    if (canSpawn) return "";
    if (modelsStatus.error) return "Some model lists failed to load — see error above.";
    if (modelsStatus.loading && modelsStatus.count === 0) return "Loading models…";
    return "Select a model.";
  });
  // On every distinct (provider, model) we apply the config default once.
  // The user can override in the picker afterwards; if they later switch
  // model, the new model's default wins (thinking is model-specific anyway).
  let lastAppliedKey = $state("");

  $effect(() => {
    const key = `${provider}|${model}`;
    if (!provider || !model || key === lastAppliedKey) return;
    lastAppliedKey = key;
    commands
      .getThinkingDefault(provider, model)
      .then((result) => {
        if (result.status !== "ok") return;
        // Guard against stale resolves after the user has switched again.
        if (`${provider}|${model}` !== lastAppliedKey) return;
        thinkingLevel = clampLevel(provider, model, result.data);
      })
      .catch(() => {});
  });

  let cwd = $state("");
  let detectedProject: DetectedProject | null = $state(null);

  // Agent identity
  let shadowName = $state("");
  let shadowTitle = $state("");
  let shadowGrade: ShadowGrade = $state("Junior");

  let saveAsTemplate = $state(false);

  function applyTemplate(t: AgentTemplateRow) {
    if (t.provider) provider = t.provider;
    if (t.model) model = t.model;
    if (t.thinkingLevel) thinkingLevel = t.thinkingLevel;
    if (t.cwd) cwd = t.cwd;
    shadowName = t.shadowName ?? "";
    shadowTitle = t.shadowTitle ?? "";
    if (t.shadowGrade) shadowGrade = t.shadowGrade as ShadowGrade;
  }

  async function persistCurrentAsTemplate() {
    const name = shadowName.trim();
    if (!name) return;
    const now = new Date().toISOString();
    const template: AgentTemplateRow = {
      id: `tpl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      name,
      provider,
      model: model.trim() || null,
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

  async function detectProject(path: string) {
    try {
      detectedProject = await invoke<DetectedProject | null>("detect_project", { cwd: path });
    } catch {
      detectedProject = null;
    }
  }

  // Detect project when cwd changes
  $effect(() => {
    if (cwd.trim()) {
      detectProject(cwd.trim());
    } else {
      detectedProject = null;
    }
  });

  async function handleSpawn() {
    if (!canSpawn) return;
    if (saveAsTemplate && shadowName.trim()) {
      await persistCurrentAsTemplate();
    }

    const config: AgentConfig = {
      provider,
      model: model.trim() || undefined,
      thinkingLevel,
      cwd: cwd || undefined,
      contextWindow,
    };

    // Attach agent identity if a name is provided
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
    // ModelSelector stops Escape propagation when its dropdown is open,
    // so here Escape is unconditionally a "close dialog" request.
    if (e.key === "Escape") oncancel();
    if (matchBinding(e, "dialog.confirm-spawn")) handleSpawn();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="form">
<TemplateSelector onselect={applyTemplate} />

<ModelSelector bind:provider bind:model bind:thinkingLevel bind:contextWindow bind:modelsStatus />

{#if agentStore.projects.length > 0}
  <div class="section">
    <span class="label">Project</span>
    <div class="project-chips">
      {#each agentStore.projects as p (p.id)}
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

<div class="field">
  <label class="label" for="cwd">Working Directory</label>
  <div class="cwd-row">
    <input
      id="cwd"
      type="text"
      bind:value={cwd}
      placeholder="/path/to/project"
    />
    <button class="browse-btn" onclick={browseFolder} title="Browse">
      ...
    </button>
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
  <span class="label">Agent Identity</span>
  <div class="row">
    <div class="field">
      <label class="label" for="agent-name">Name</label>
      <input
        id="agent-name"
        type="text"
        bind:value={shadowName}
        placeholder="e.g. Atlas, Nova, Sage"
      />
    </div>
    <div class="field">
      <label class="label" for="agent-grade">Grade</label>
      <select id="agent-grade" bind:value={shadowGrade}>
        {#each SHADOW_GRADES as grade}
          <option value={grade}>{grade}</option>
        {/each}
      </select>
    </div>
  </div>
  <div class="field">
    <label class="label" for="agent-title">Title</label>
    <input
      id="agent-title"
      type="text"
      bind:value={shadowTitle}
      placeholder="e.g. Backend Engineer, Tech Lead"
    />
  </div>
</div>

<label class="template-save-check" title={shadowName.trim() ? "" : "Set an agent name to enable"}>
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

{#if !canSpawn}
  <div class="action-hint">{disabledHint}</div>
{/if}

<div class="actions">
  <button class="btn-cancel" onclick={oncancel}>Cancel</button>
  <button class="btn-spawn" onclick={handleSpawn} disabled={!canSpawn}>
    Create agent
    <span class="shortcut">Ctrl+Enter</span>
  </button>
</div>
</div>

<style>
  /* Vertical rhythm for the whole dialog — SpawnForm renders into the Modal
     body, which has no stacking of its own. */
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--s4);
  }

  /* Design-system restyle: Inter for labels/copy, mono only for paths.
     Foundation tokens throughout. */
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
  }

  .label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  .template-save-check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-secondary);
    cursor: pointer;
    user-select: none;
  }

  .template-save-check input[type="checkbox"] {
    width: auto;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .template-save-check input[type="checkbox"]:disabled {
    cursor: not-allowed;
  }

  .template-save-check input[type="checkbox"]:disabled + span {
    color: var(--text-muted);
  }

  .template-save-hint {
    color: var(--text-muted);
  }

  .project-chips {
    display: flex;
    gap: var(--s2);
    flex-wrap: wrap;
  }

  .project-chip {
    display: flex;
    align-items: center;
    gap: 2px;
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    padding: 5px var(--s3);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-raised);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.14s, border-color 0.14s, color 0.14s;
  }

  .project-chip:hover {
    background: var(--bg-overlay);
    border-color: var(--border-strong);
    color: var(--text-primary);
  }

  .project-chip.active {
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-raised));
    border-color: var(--accent);
    color: var(--accent);
  }

  .project-chip-slash {
    color: var(--accent);
    font-weight: 700;
  }

  .project-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: var(--s2) var(--s3);
    border-radius: var(--r-md);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    background: color-mix(in srgb, var(--accent) 6%, var(--bg-raised));
  }

  .project-info-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .project-info .project-icon {
    color: var(--accent);
    font-weight: 700;
  }

  .project-info-path {
    font-size: 10px;
    color: var(--text-muted);
    font-family: "JetBrains Mono", monospace;
    overflow-wrap: anywhere;
  }

  .project-info-tag {
    font-size: 10.5px;
    color: var(--accent);
  }

  .cwd-row {
    display: flex;
    gap: var(--s2);
  }

  .cwd-row input {
    flex: 1;
    font-family: "JetBrains Mono", monospace;
    font-size: 11.5px;
  }

  .browse-btn {
    font: inherit;
    font-size: 12px;
    padding: 7px var(--s3);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-raised);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.14s;
    flex-shrink: 0;
  }

  .browse-btn:hover {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }

  .row {
    display: flex;
    gap: var(--s3);
    flex-wrap: wrap;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
    min-width: 0;
  }

  input,
  select {
    font: inherit;
    font-size: 12.5px;
    width: 100%;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    color: var(--text-primary);
    padding: 7px var(--s3);
    transition: border-color 0.14s, background 0.14s;
  }

  input::placeholder {
    color: var(--text-muted);
  }

  input:focus,
  select:focus {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
    border-color: var(--accent);
    background: var(--bg-overlay);
  }

  select {
    cursor: pointer;
    appearance: none;
    background-image: linear-gradient(45deg, transparent 50%, var(--text-muted) 50%), linear-gradient(135deg, var(--text-muted) 50%, transparent 50%);
    background-position: calc(100% - 15px) 14px, calc(100% - 10px) 14px;
    background-size: 5px 5px, 5px 5px;
    background-repeat: no-repeat;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--s2);
    margin-top: var(--s1);
  }

  .btn-cancel {
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    padding: 7px var(--s4);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.14s, color 0.14s, border-color 0.14s;
  }

  .btn-cancel:hover {
    background: var(--bg-raised);
    color: var(--text-primary);
    border-color: var(--border-strong);
  }

  .btn-spawn {
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    padding: 7px var(--s4);
    border: 1px solid var(--accent);
    border-radius: var(--r-md);
    background: var(--accent);
    color: var(--accent-ink);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: var(--s2);
    transition: background 0.14s;
  }

  .btn-spawn:hover {
    background: var(--accent-hover);
    border-color: var(--accent-hover);
  }

  .btn-spawn:disabled {
    background: var(--bg-raised);
    border-color: var(--border);
    color: var(--text-muted);
    cursor: not-allowed;
  }

  .btn-spawn:focus-visible,
  .btn-cancel:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  .action-hint {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .shortcut {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    font-weight: 400;
    opacity: 0.7;
  }

  @media (max-width: 640px) {
    .actions {
      flex-wrap: wrap-reverse;
    }

    .actions button {
      width: 100%;
      justify-content: center;
    }
  }
</style>
