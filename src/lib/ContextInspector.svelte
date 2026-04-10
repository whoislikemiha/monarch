<script lang="ts">
  import type { DisplayItem, ToolExecution, Usage, ContentBlock } from "./types";

  let {
    items,
    lastUsage,
    contextWindow,
    sessionStats,
    onclose,
  }: {
    items: DisplayItem[];
    lastUsage?: Usage;
    contextWindow?: number;
    sessionStats?: { totalTokens: number; totalCost: number; messageCount: number; turnCount: number };
    onclose: () => void;
  } = $props();

  // ~4 chars per token is a rough but reasonable heuristic for English text
  const CHARS_PER_TOKEN = 4;

  function estimateTokens(text: string): number {
    return Math.ceil(text.length / CHARS_PER_TOKEN);
  }

  function formatTokens(n: number): string {
    if (n >= 1000000) return (n / 1000000).toFixed(1) + "M";
    if (n >= 1000) return (n / 1000).toFixed(1) + "k";
    return n.toString();
  }

  function contentBlockText(block: ContentBlock): string {
    switch (block.type) {
      case "text": return block.text;
      case "thinking": return block.thinking;
      case "toolCall": return JSON.stringify(block.arguments);
      case "image": return "[image]";
      default: return "";
    }
  }

  function truncate(text: string, max: number): string {
    if (text.length <= max) return text;
    return text.slice(0, max) + "...";
  }

  interface ContextCategory {
    id: string;
    label: string;
    tokens: number;
    entries: { label: string; tokens: number; preview: string }[];
  }

  let expanded: Record<string, boolean> = $state({});

  function toggle(id: string) {
    expanded = { ...expanded, [id]: !expanded[id] };
  }

  let categories = $derived.by(() => {
    const cats: ContextCategory[] = [];

    // --- Conversation (user messages) ---
    const userEntries: ContextCategory["entries"] = [];
    let userIdx = 0;
    for (const item of items) {
      if (item.kind === "user") {
        userIdx++;
        const text = item.content;
        userEntries.push({
          label: `User #${userIdx}`,
          tokens: estimateTokens(text),
          preview: truncate(text, 120),
        });
      }
    }
    if (userEntries.length > 0) {
      cats.push({
        id: "user-messages",
        label: "User Messages",
        tokens: userEntries.reduce((s, e) => s + e.tokens, 0),
        entries: userEntries,
      });
    }

    // --- Assistant responses ---
    const assistantEntries: ContextCategory["entries"] = [];
    let assistantIdx = 0;
    for (const item of items) {
      if (item.kind === "assistant") {
        assistantIdx++;
        const textParts = item.content
          .filter((b): b is { type: "text"; text: string } => b.type === "text")
          .map((b) => b.text);
        const fullText = textParts.join("");
        assistantEntries.push({
          label: `Assistant #${assistantIdx}${item.model ? ` (${item.model})` : ""}`,
          tokens: estimateTokens(fullText),
          preview: truncate(fullText, 120),
        });
      }
    }
    if (assistantEntries.length > 0) {
      cats.push({
        id: "assistant-messages",
        label: "Assistant Messages",
        tokens: assistantEntries.reduce((s, e) => s + e.tokens, 0),
        entries: assistantEntries,
      });
    }

    // --- Thinking blocks ---
    const thinkingEntries: ContextCategory["entries"] = [];
    let thinkIdx = 0;
    for (const item of items) {
      if (item.kind === "assistant") {
        for (const block of item.content) {
          if (block.type === "thinking") {
            thinkIdx++;
            thinkingEntries.push({
              label: `Thinking #${thinkIdx}`,
              tokens: estimateTokens(block.thinking),
              preview: truncate(block.thinking, 120),
            });
          }
        }
      }
    }
    if (thinkingEntries.length > 0) {
      cats.push({
        id: "thinking",
        label: "Thinking",
        tokens: thinkingEntries.reduce((s, e) => s + e.tokens, 0),
        entries: thinkingEntries,
      });
    }

    // --- Tool calls (arguments sent to tools) ---
    const toolCallEntries: ContextCategory["entries"] = [];
    for (const item of items) {
      if (item.kind === "tool-group") {
        for (const exec of item.executions) {
          const argsText = JSON.stringify(exec.args ?? {});
          toolCallEntries.push({
            label: exec.toolName,
            tokens: estimateTokens(argsText),
            preview: truncate(argsText, 120),
          });
        }
      }
    }
    if (toolCallEntries.length > 0) {
      cats.push({
        id: "tool-calls",
        label: "Tool Calls",
        tokens: toolCallEntries.reduce((s, e) => s + e.tokens, 0),
        entries: toolCallEntries,
      });
    }

    // --- Tool outputs (results from tools) ---
    const toolOutputEntries: ContextCategory["entries"] = [];
    for (const item of items) {
      if (item.kind === "tool-group") {
        for (const exec of item.executions) {
          if (exec.result != null) {
            const resultText = typeof exec.result === "string" ? exec.result : JSON.stringify(exec.result);
            toolOutputEntries.push({
              label: `${exec.toolName} result`,
              tokens: estimateTokens(resultText),
              preview: truncate(resultText, 120),
            });
          }
        }
      }
    }
    if (toolOutputEntries.length > 0) {
      cats.push({
        id: "tool-outputs",
        label: "Tool Outputs",
        tokens: toolOutputEntries.reduce((s, e) => s + e.tokens, 0),
        entries: toolOutputEntries,
      });
    }

    return cats;
  });

  let estimatedTotal = $derived(categories.reduce((s, c) => s + c.tokens, 0));
  let apiTotal = $derived(sessionStats?.totalTokens ?? lastUsage?.totalTokens ?? 0);
  let resolvedWindow = $derived(contextWindow ?? 128000);
  let usedRatio = $derived(apiTotal > 0 ? apiTotal / resolvedWindow : 0);
</script>

<div class="inspector">
  <div class="inspector-header">
    <span class="inspector-title">Context Inspector</span>
    <button class="close-btn" onclick={onclose}>&times;</button>
  </div>

  <div class="inspector-summary">
    <div class="summary-row">
      <span class="summary-label">Context Used</span>
      <span class="summary-value">{formatTokens(apiTotal)} / {formatTokens(resolvedWindow)}</span>
    </div>
    <div class="summary-bar">
      <div class="summary-fill" style:width={`${Math.min(usedRatio * 100, 100)}%`}></div>
    </div>
    {#if sessionStats}
      <div class="summary-row sub">
        <span class="summary-label">Turns</span>
        <span class="summary-value">{sessionStats.turnCount}</span>
      </div>
      <div class="summary-row sub">
        <span class="summary-label">Messages</span>
        <span class="summary-value">{sessionStats.messageCount}</span>
      </div>
    {/if}
    {#if lastUsage}
      <div class="summary-row sub">
        <span class="summary-label">Cache Read</span>
        <span class="summary-value">{formatTokens(lastUsage.cacheRead)}</span>
      </div>
      <div class="summary-row sub">
        <span class="summary-label">Cache Write</span>
        <span class="summary-value">{formatTokens(lastUsage.cacheWrite)}</span>
      </div>
    {/if}
  </div>

  <div class="inspector-categories">
    {#each categories as cat (cat.id)}
      <div class="category">
        <button class="category-header" onclick={() => toggle(cat.id)}>
          <span class="category-chevron">{expanded[cat.id] ? "▾" : "▸"}</span>
          <span class="category-label">{cat.label}</span>
          <span class="category-count">{cat.entries.length}</span>
          <span class="category-tokens">{formatTokens(cat.tokens)}</span>
        </button>

        {#if expanded[cat.id]}
          <div class="category-entries">
            {#each cat.entries as entry}
              <div class="entry">
                <div class="entry-header">
                  <span class="entry-label">{entry.label}</span>
                  <span class="entry-tokens">{formatTokens(entry.tokens)}</span>
                </div>
                <div class="entry-preview">{entry.preview}</div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}

    {#if categories.length === 0}
      <div class="empty">No context data yet</div>
    {/if}
  </div>

  <div class="inspector-footer">
    <span class="footer-note">Estimated ~{formatTokens(estimatedTotal)} from content</span>
  </div>
</div>

<style>
  .inspector {
    width: 320px;
    min-width: 280px;
    max-width: 400px;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-sidebar, #0c0816);
    border-left: 1px solid var(--border-subtle, #35274f);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-size: 11px;
    color: var(--text-secondary, #dde1e6);
    overflow: hidden;
  }

  .inspector-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-subtle, #35274f);
    flex-shrink: 0;
  }

  .inspector-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-primary, #f2f4f8);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-muted, #8f7aa8);
    font-size: 16px;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
    transition: color 0.15s;
  }

  .close-btn:hover {
    color: var(--text-primary, #f2f4f8);
  }

  .inspector-summary {
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-subtle, #35274f);
    flex-shrink: 0;
  }

  .summary-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .summary-row.sub {
    margin-bottom: 2px;
  }

  .summary-label {
    color: var(--text-muted, #8f7aa8);
    font-size: 10px;
  }

  .summary-value {
    color: var(--text-secondary, #dde1e6);
    font-size: 10px;
  }

  .summary-row.sub .summary-label,
  .summary-row.sub .summary-value {
    font-size: 9px;
  }

  .summary-bar {
    height: 4px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.06);
    margin-bottom: 8px;
    overflow: hidden;
  }

  .summary-fill {
    height: 100%;
    border-radius: 2px;
    background: var(--accent-purple, #be95ff);
    transition: width 0.25s ease;
  }

  .inspector-categories {
    flex: 1;
    overflow-y: auto;
    padding: 8px 0;
  }

  .category {
    border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  }

  .category-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 8px 14px;
    background: none;
    border: none;
    color: var(--text-secondary, #dde1e6);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .category-header:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .category-chevron {
    font-size: 9px;
    width: 10px;
    color: var(--text-muted, #8f7aa8);
    flex-shrink: 0;
  }

  .category-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .category-count {
    font-size: 9px;
    color: var(--text-muted, #8f7aa8);
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.04);
    flex-shrink: 0;
  }

  .category-tokens {
    font-size: 10px;
    color: var(--accent-purple, #be95ff);
    flex-shrink: 0;
    min-width: 40px;
    text-align: right;
  }

  .category-entries {
    padding: 0 14px 8px 30px;
  }

  .entry {
    padding: 4px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.02);
  }

  .entry:last-child {
    border-bottom: none;
  }

  .entry-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 2px;
  }

  .entry-label {
    font-size: 10px;
    color: var(--text-secondary, #dde1e6);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .entry-tokens {
    font-size: 9px;
    color: var(--text-muted, #8f7aa8);
    flex-shrink: 0;
    margin-left: 8px;
  }

  .entry-preview {
    font-size: 9px;
    color: var(--text-muted, #8f7aa8);
    line-height: 1.4;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    word-break: break-word;
  }

  .empty {
    padding: 24px 14px;
    text-align: center;
    color: var(--text-muted, #8f7aa8);
    font-size: 10px;
  }

  .inspector-footer {
    padding: 8px 14px;
    border-top: 1px solid var(--border-subtle, #35274f);
    flex-shrink: 0;
  }

  .footer-note {
    font-size: 9px;
    color: var(--text-muted, #8f7aa8);
  }
</style>
