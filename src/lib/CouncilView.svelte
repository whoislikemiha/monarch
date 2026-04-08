<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import AssistantMessageComp from "./AssistantMessage.svelte";
  import ChatInput from "./ChatInput.svelte";
  import type {
    Agent,
    PiEvent,
    AssistantMessage,
    ContentBlock,
    CouncilResponse,
    Usage,
  } from "./types";

  let {
    agents,
    onback,
  }: {
    agents: Agent[];
    onback: () => void;
  } = $props();

  // Council state
  let prompt = $state("");
  let responses: Map<string, CouncilResponse> = $state(new Map());
  let selectedAgentId: string | null = $state(null);
  let councilStatus: "idle" | "streaming" | "voting" | "decided" = $state("idle");
  let hasPrompted = $state(false);

  // Per-agent streaming state
  let streamingMessages: Map<string, AssistantMessage | null> = $state(new Map());
  let unlisteners: UnlistenFn[] = [];

  // Track which agents have finished
  let finishedAgents = $state(new Set<string>());

  let allFinished = $derived(
    hasPrompted && agents.length > 0 && finishedAgents.size >= agents.length
  );

  $effect(() => {
    if (allFinished && councilStatus === "streaming") {
      councilStatus = "voting";
    }
  });

  function handleAgentEvent(agentId: string, raw: string) {
    let event: PiEvent;
    try {
      event = JSON.parse(raw);
    } catch {
      return;
    }

    switch (event.type) {
      case "message_start":
        if (event.message.role === "assistant") {
          streamingMessages.set(agentId, event.message as AssistantMessage);
          streamingMessages = new Map(streamingMessages);
        }
        break;

      case "message_update":
        if (event.message.role === "assistant") {
          streamingMessages.set(agentId, event.message as AssistantMessage);
          streamingMessages = new Map(streamingMessages);

          // Update the response content live
          const msg = event.message as AssistantMessage;
          const existing = responses.get(agentId);
          if (existing) {
            existing.content = msg.content;
            existing.isStreaming = true;
            if (msg.usage) existing.usage = msg.usage;
            if (msg.model) existing.model = msg.model;
            responses = new Map(responses);
          }
        }
        break;

      case "message_end":
        if (event.message.role === "assistant") {
          const msg = event.message as AssistantMessage;
          const existing = responses.get(agentId);
          if (existing) {
            existing.content = msg.content;
            existing.isStreaming = false;
            if (msg.usage) existing.usage = msg.usage;
            if (msg.model) existing.model = msg.model;
            responses = new Map(responses);
          }
          streamingMessages.delete(agentId);
          streamingMessages = new Map(streamingMessages);
        }
        break;

      case "agent_end":
        finishedAgents.add(agentId);
        finishedAgents = new Set(finishedAgents);
        const resp = responses.get(agentId);
        if (resp) {
          resp.isStreaming = false;
          responses = new Map(responses);
        }
        break;
    }
  }

  async function sendCouncilPrompt(message: string) {
    prompt = message;
    hasPrompted = true;
    councilStatus = "streaming";
    finishedAgents = new Set();

    // Initialize response slots
    const newResponses = new Map<string, CouncilResponse>();
    for (const agent of agents) {
      newResponses.set(agent.id, {
        agentId: agent.id,
        shadowName: agent.shadow?.shadowName || agent.name,
        shadowGrade: agent.shadow?.shadowGrade,
        content: [],
        isStreaming: true,
        votes: 0,
      });
    }
    responses = newResponses;

    // Broadcast to all agents
    const agentIds = agents.map((a) => a.id);
    await invoke("broadcast_prompt", { agentIds, message });
  }

  function selectWinner(agentId: string) {
    selectedAgentId = agentId;
    councilStatus = "decided";
  }

  function resetCouncil() {
    responses = new Map();
    selectedAgentId = null;
    councilStatus = "idle";
    hasPrompted = false;
    finishedAgents = new Set();
    streamingMessages = new Map();
    prompt = "";
  }

  // Grade color mapping
  function gradeColor(grade?: string): string {
    switch (grade) {
      case "Grand Marshal": return "#ff7eb6";
      case "Marshal": return "#be95ff";
      case "General": return "#33b1ff";
      case "Elite Knight": return "#42be65";
      case "Knight": return "#ffe97b";
      default: return "#8f7aa8";
    }
  }

  onMount(async () => {
    // Listen to events from all council agents
    for (const agent of agents) {
      const unlisten = await listen<string>(
        `agent-event-${agent.id}`,
        (event) => handleAgentEvent(agent.id, event.payload),
      );
      unlisteners.push(unlisten);
    }
  });

  onDestroy(() => {
    for (const unlisten of unlisteners) {
      unlisten();
    }
  });
</script>

<div class="council-view">
  <div class="council-header">
    <button class="back-btn" onclick={onback} title="Back to single view">
      &larr;
    </button>
    <h2>Council Chamber</h2>
    <span class="council-count">{agents.length} shadows</span>
    {#if councilStatus === "decided"}
      <button class="reset-btn" onclick={resetCouncil}>New Council</button>
    {/if}
  </div>

  {#if hasPrompted}
    <div class="prompt-display">
      <span class="prompt-label">Monarch's Question</span>
      <p class="prompt-text">{prompt}</p>
    </div>
  {/if}

  <div class="responses-grid" class:single={agents.length === 1} class:dual={agents.length === 2} class:multi={agents.length > 2}>
    {#each agents as agent (agent.id)}
      {@const response = responses.get(agent.id)}
      <div
        class="response-card"
        class:streaming={response?.isStreaming}
        class:selected={selectedAgentId === agent.id}
        class:decided={councilStatus === "decided"}
      >
        <div class="response-header">
          <span class="shadow-name" style="color: {gradeColor(agent.shadow?.shadowGrade)}">
            {agent.shadow?.shadowName || agent.name}
          </span>
          {#if agent.shadow?.shadowGrade}
            <span class="shadow-grade" style="color: {gradeColor(agent.shadow.shadowGrade)}">
              {agent.shadow.shadowGrade}
            </span>
          {/if}
          {#if response?.isStreaming}
            <span class="streaming-dot"></span>
          {/if}
          {#if response?.model}
            <span class="model-tag">{response.model}</span>
          {/if}
        </div>

        <div class="response-body">
          {#if response && response.content.length > 0}
            <AssistantMessageComp content={response.content} />
          {:else if hasPrompted}
            <div class="waiting">Awaiting response...</div>
          {:else}
            <div class="waiting">Ready</div>
          {/if}
        </div>

        {#if response?.usage}
          <div class="response-footer">
            <span class="usage-tag">{response.usage.totalTokens.toLocaleString()} tokens</span>
            {#if response.usage.cost}
              <span class="cost-tag">${response.usage.cost.total.toFixed(4)}</span>
            {/if}
          </div>
        {/if}

        {#if councilStatus === "voting"}
          <button
            class="select-btn"
            onclick={() => selectWinner(agent.id)}
          >
            Select this answer
          </button>
        {/if}

        {#if selectedAgentId === agent.id}
          <div class="winner-badge">Selected</div>
        {/if}
      </div>
    {/each}
  </div>

  {#if !hasPrompted}
    <div class="council-input">
      <ChatInput
        onsend={sendCouncilPrompt}
        disabled={councilStatus === "streaming"}
        placeholder="Ask the council..."
      />
    </div>
  {:else if councilStatus === "decided"}
    <div class="council-input">
      <ChatInput
        onsend={sendCouncilPrompt}
        disabled={false}
        placeholder="Ask another question..."
      />
    </div>
  {/if}
</div>

<style>
  .council-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel, #171126);
    min-width: 0;
    height: 100%;
  }

  .council-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-sidebar, #0c0816);
  }

  .council-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--accent-purple, #be95ff);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .council-count {
    font-size: 11px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .back-btn {
    padding: 4px 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 14px;
    cursor: pointer;
    transition: background 0.15s;
  }
  .back-btn:hover {
    background: var(--bg-panel-3);
    color: var(--text-primary);
  }

  .reset-btn {
    margin-left: auto;
    padding: 4px 12px;
    border: 1px solid var(--accent-purple, #be95ff);
    border-radius: 6px;
    background: transparent;
    color: var(--accent-purple, #be95ff);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }
  .reset-btn:hover {
    background: rgba(190, 149, 255, 0.1);
  }

  .prompt-display {
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-panel-2, #201734);
  }

  .prompt-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--accent-blue, #33b1ff);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .prompt-text {
    margin: 6px 0 0;
    font-size: 13px;
    color: var(--text-primary);
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .responses-grid {
    flex: 1;
    display: grid;
    gap: 1px;
    background: var(--border-subtle);
    overflow: hidden;
  }

  .responses-grid.single {
    grid-template-columns: 1fr;
  }
  .responses-grid.dual {
    grid-template-columns: 1fr 1fr;
  }
  .responses-grid.multi {
    grid-template-columns: repeat(auto-fit, minmax(350px, 1fr));
  }

  .response-card {
    display: flex;
    flex-direction: column;
    background: var(--bg-panel, #171126);
    overflow: hidden;
    position: relative;
    transition: opacity 0.3s;
  }

  .response-card.decided:not(.selected) {
    opacity: 0.4;
  }

  .response-card.selected {
    box-shadow: inset 0 0 0 2px var(--accent-purple, #be95ff);
  }

  .response-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-sidebar, #0c0816);
  }

  .shadow-name {
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .shadow-grade {
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    opacity: 0.7;
  }

  .streaming-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent-blue, #33b1ff);
    animation: pulse 1s ease-in-out infinite;
  }

  .model-tag {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .response-body {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px;
    font-size: 13px;
    line-height: 1.6;
  }

  .waiting {
    color: var(--text-muted);
    font-size: 12px;
    font-style: italic;
    text-align: center;
    padding: 24px;
  }

  .response-footer {
    display: flex;
    gap: 8px;
    padding: 6px 16px;
    border-top: 1px solid var(--border-subtle);
    font-size: 10px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .select-btn {
    margin: 8px 16px 12px;
    padding: 8px 16px;
    border: 1px solid var(--accent-purple, #be95ff);
    border-radius: 8px;
    background: transparent;
    color: var(--accent-purple, #be95ff);
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .select-btn:hover {
    background: var(--accent-purple, #be95ff);
    color: #140d22;
  }

  .winner-badge {
    position: absolute;
    top: 10px;
    right: 16px;
    padding: 2px 10px;
    border-radius: 4px;
    background: var(--accent-purple, #be95ff);
    color: #140d22;
    font-size: 10px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .council-input {
    border-top: 1px solid var(--border-subtle);
    padding: 12px 20px;
    background: var(--bg-sidebar, #0c0816);
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }
</style>
