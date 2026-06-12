/**
 * Inspector panel registry — the extension seam for the dockable right rail.
 * Adding a panel = one entry here. Future panels (git, diff, war-room) register
 * the same way; the layout store + PanelHost handle docking/pinning generically.
 *
 * The four inspectors reuse the existing tool components (they already take
 * `{ agentContext }`); their internals will be restyled onto the design-system
 * atoms in a later pass. The Architect is a stub until its backend lands.
 */
import type { Component } from "svelte";
import type { ToolProps } from "$lib/toolbox/types";
import IdentityTool from "$lib/toolbox/tools/IdentityTool.svelte";
import MemoryInspectorTool from "$lib/toolbox/tools/MemoryInspectorTool.svelte";
import ContextInspectorTool from "$lib/toolbox/tools/ContextInspectorTool.svelte";
import ShadowStatsTool from "$lib/toolbox/tools/ShadowStatsTool.svelte";
import ArchitectPanel from "$lib/panels/ArchitectPanel.svelte";
import SessionHistoryTool from "$lib/toolbox/tools/SessionHistoryTool.svelte";

export interface PanelDef {
  id: string;
  title: string;
  /** Inline SVG markup for the rail icon. */
  icon: string;
  component: Component<ToolProps>;
}

export const PANELS: PanelDef[] = [
  {
    id: "sessions",
    title: "Sessions",
    // clock-with-arrow history icon — session browser (MON-127)
    icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7"/><polyline points="3 4 3 9 8 9"/><polyline points="12 7 12 12 16 14"/></svg>`,
    component: SessionHistoryTool,
  },
  {
    id: "memory",
    title: "Memory",
    icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2"/><circle cx="18" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="18" r="2"/><circle cx="12" cy="12" r="2"/><line x1="7.4" y1="7.4" x2="10.6" y2="10.6"/><line x1="16.6" y1="7.4" x2="13.4" y2="10.6"/><line x1="7.4" y1="16.6" x2="10.6" y2="13.4"/><line x1="16.6" y1="16.6" x2="13.4" y2="13.4"/></svg>`,
    component: MemoryInspectorTool,
  },
  {
    id: "context",
    title: "Context",
    icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/></svg>`,
    component: ContextInspectorTool,
  },
  {
    id: "architect",
    title: "Architect",
    icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 21h18"/><path d="M5 21V8l7-5 7 5v13"/><path d="M9 21v-6h6v6"/></svg>`,
    component: ArchitectPanel,
  },
  {
    id: "stats",
    title: "Stats",
    icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/></svg>`,
    component: ShadowStatsTool,
  },
  {
    id: "identity",
    title: "Identity",
    icon: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`,
    component: IdentityTool,
  },
];

export function getPanel(id: string): PanelDef | undefined {
  return PANELS.find((p) => p.id === id);
}
