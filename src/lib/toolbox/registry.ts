import ContextInspectorTool from "./tools/ContextInspectorTool.svelte";
import ShadowStatsTool from "./tools/ShadowStatsTool.svelte";
import PlaceholderTool from "./tools/PlaceholderTool.svelte";
import QuestTimelineTool from "./tools/QuestTimelineTool.svelte";
import ClassifierSettingsTool from "./tools/ClassifierSettingsTool.svelte";
import IdentityTool from "./tools/IdentityTool.svelte";
import MemoryInspectorTool from "./tools/MemoryInspectorTool.svelte";
import type { ToolDefinition } from "./types";

/**
 * The toolbox registry. Adding a tool = appending an entry here and creating
 * its Svelte component under `tools/`. Order of rendering is determined by
 * the optional `order` field (ascending), then array position.
 */
export const TOOLS: ToolDefinition[] = [
  {
    id: "identity",
    title: "Identity",
    order: 5,
    hasBackend: true,
    // person / user icon — captain + shadow identity editor
    icon: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`,
    component: IdentityTool,
  },
  {
    id: "memory-inspector",
    title: "Memory",
    order: 6,
    hasBackend: true,
    // brain / nodes icon — memory tree inspector
    icon: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2"/><circle cx="18" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="18" r="2"/><circle cx="12" cy="12" r="2"/><line x1="7.4" y1="7.4" x2="10.6" y2="10.6"/><line x1="16.6" y1="7.4" x2="13.4" y2="10.6"/><line x1="7.4" y1="16.6" x2="10.6" y2="13.4"/><line x1="16.6" y1="16.6" x2="13.4" y2="13.4"/></svg>`,
    component: MemoryInspectorTool,
  },
  {
    id: "context-inspector",
    title: "Context",
    order: 10,
    hasBackend: false,
    icon: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/></svg>`,
    component: ContextInspectorTool,
  },
  {
    id: "shadow-stats",
    title: "Stats",
    order: 15,
    hasBackend: true,
    icon: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/></svg>`,
    component: ShadowStatsTool,
  },
  {
    id: "quest-timeline",
    title: "Quests",
    order: 20,
    hasBackend: true,
    // compass / map icon — quest tree navigation
    icon: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polygon points="16 8 14 14 8 16 10 10 16 8"/></svg>`,
    component: QuestTimelineTool,
  },
  {
    id: "classifier-settings",
    title: "Classifier",
    order: 25,
    hasBackend: true,
    // tag icon — per-message labeling
    icon: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20.59 13.41L13.42 20.58a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/></svg>`,
    component: ClassifierSettingsTool,
  },
  {
    id: "placeholder",
    title: "Placeholder",
    order: 100,
    hasBackend: true,
    icon: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3"/></svg>`,
    component: PlaceholderTool,
  },
];

export function getTool(id: string): ToolDefinition | undefined {
  return TOOLS.find((t) => t.id === id);
}

export function sortedTools(): ToolDefinition[] {
  return [...TOOLS].sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
}
