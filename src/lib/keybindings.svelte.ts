/**
 * Central keybinding registry (MON-44).
 *
 * Defines every app-specific shortcut, provides matching and display utilities,
 * and persists user overrides to the ui_state DB table.
 *
 * Universal bindings (Enter to send, Escape to close, arrow keys in dropdowns)
 * are intentionally omitted — they stay hardcoded in their components.
 */

import { invoke } from "$lib/api";

// --- Types ---

export interface KeyBindingDef {
  id: string;
  label: string;
  group: BindingGroup;
  defaultKeys: string;
  editable: boolean;
  hint?: string;
}

export const BINDING_GROUPS = ["Global", "Navigation", "Zoom", "Dialog"] as const;
export type BindingGroup = (typeof BINDING_GROUPS)[number];

// --- Platform detection ---

const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

// --- Default bindings ---

export const DEFAULT_BINDINGS: KeyBindingDef[] = [
  // Global
  { id: "global.spawn-agent", label: "Create agent", group: "Global", defaultKeys: "Ctrl+n", editable: true },
  { id: "global.settings", label: "Toggle settings", group: "Global", defaultKeys: "Ctrl+,", editable: true },
  { id: "global.toggle-sidebar", label: "Toggle sidebar", group: "Global", defaultKeys: "Ctrl+b", editable: true },
  { id: "global.focus-chat", label: "Focus chat input", group: "Global", defaultKeys: "/", editable: true, hint: "when not in input" },
  { id: "global.focus-chat-alt", label: "Focus chat input (alt)", group: "Global", defaultKeys: "i", editable: true, hint: "when not in input" },
  { id: "global.abort-agent", label: "Abort agent", group: "Global", defaultKeys: "Ctrl+c", editable: true, hint: "when no text selected" },

  // Navigation
  { id: "nav.tab-1", label: "Switch to tab 1", group: "Navigation", defaultKeys: "Ctrl+1", editable: true },
  { id: "nav.tab-2", label: "Switch to tab 2", group: "Navigation", defaultKeys: "Ctrl+2", editable: true },
  { id: "nav.tab-3", label: "Switch to tab 3", group: "Navigation", defaultKeys: "Ctrl+3", editable: true },
  { id: "nav.tab-4", label: "Switch to tab 4", group: "Navigation", defaultKeys: "Ctrl+4", editable: true },
  { id: "nav.tab-5", label: "Switch to tab 5", group: "Navigation", defaultKeys: "Ctrl+5", editable: true },
  { id: "nav.tab-6", label: "Switch to tab 6", group: "Navigation", defaultKeys: "Ctrl+6", editable: true },
  { id: "nav.tab-7", label: "Switch to tab 7", group: "Navigation", defaultKeys: "Ctrl+7", editable: true },
  { id: "nav.tab-8", label: "Switch to tab 8", group: "Navigation", defaultKeys: "Ctrl+8", editable: true },
  { id: "nav.tab-9", label: "Switch to tab 9", group: "Navigation", defaultKeys: "Ctrl+9", editable: true },
  { id: "nav.recent-agent", label: "Recent agent", group: "Navigation", defaultKeys: "Ctrl+Tab", editable: true },
  { id: "nav.next-agent", label: "Next agent", group: "Navigation", defaultKeys: "Ctrl+PageDown", editable: true },

  // Zoom (non-editable, display only)
  { id: "zoom.in", label: "Zoom in", group: "Zoom", defaultKeys: "Ctrl+=", editable: false },
  { id: "zoom.out", label: "Zoom out", group: "Zoom", defaultKeys: "Ctrl+-", editable: false },
  { id: "zoom.reset", label: "Reset zoom", group: "Zoom", defaultKeys: "Ctrl+0", editable: false },
  { id: "zoom.scroll", label: "Zoom with scroll", group: "Zoom", defaultKeys: "Ctrl+Scroll", editable: false },

  // Dialog
  { id: "dialog.confirm-spawn", label: "Confirm create", group: "Dialog", defaultKeys: "Ctrl+Enter", editable: true },
];

// --- Reactive state ---

let overrides: Record<string, string> = $state({});

// --- Persistence ---

export async function loadKeybindings(): Promise<void> {
  try {
    const json = await invoke<string | null>("db_get_ui_state", { key: "keybindings" });
    if (json) {
      overrides = JSON.parse(json);
    }
  } catch {
    // No saved keybindings yet
  }
}

function persistOverrides(): void {
  invoke("db_set_ui_state", {
    key: "keybindings",
    value: JSON.stringify(overrides),
  }).catch(() => {});
}

// --- Public API ---

/** Get the current key combo for a binding (override or default). */
export function getBinding(id: string): string {
  if (overrides[id]) return overrides[id];
  const def = DEFAULT_BINDINGS.find((b) => b.id === id);
  return def?.defaultKeys ?? "";
}

/** Set a custom key combo for a binding. Persists to DB. */
export function setBinding(id: string, keys: string): void {
  const def = DEFAULT_BINDINGS.find((b) => b.id === id);
  if (!def || !def.editable) return;

  if (keys === def.defaultKeys) {
    const { [id]: _, ...rest } = overrides;
    overrides = rest;
  } else {
    overrides = { ...overrides, [id]: keys };
  }
  persistOverrides();
}

/** Reset all bindings to defaults. Persists to DB. */
export function resetAllBindings(): void {
  overrides = {};
  persistOverrides();
}

/** Get all bindings with their current (possibly overridden) keys. */
export function getAllBindings(): Array<KeyBindingDef & { currentKeys: string; isOverridden: boolean }> {
  return DEFAULT_BINDINGS.map((def) => ({
    ...def,
    currentKeys: overrides[def.id] || def.defaultKeys,
    isOverridden: !!overrides[def.id],
  }));
}

// --- Matching ---

/** Check if a KeyboardEvent matches a binding by ID. */
export function matchBinding(e: KeyboardEvent, bindingId: string): boolean {
  const keys = getBinding(bindingId);
  if (!keys) return false;
  return matchKeys(e, keys);
}

/** Check if a KeyboardEvent matches a key combo string. */
export function matchKeys(e: KeyboardEvent, keys: string): boolean {
  if (keys.includes("Scroll")) return false;

  const parts = keys.split("+");
  const key = parts[parts.length - 1];
  const mods = new Set(parts.slice(0, -1));

  const wantCtrl = mods.has("Ctrl");
  const wantShift = mods.has("Shift");
  const wantAlt = mods.has("Alt");

  const hasCtrl = isMac ? e.metaKey : e.ctrlKey;
  if (hasCtrl !== wantCtrl) return false;
  if (e.shiftKey !== wantShift) return false;
  if (e.altKey !== wantAlt) return false;

  return e.key.toLowerCase() === key.toLowerCase();
}

// --- Key capture (for settings UI) ---

/**
 * Convert a KeyboardEvent into a binding string for storage.
 * Returns null for bare modifier presses.
 */
export function eventToBindingString(e: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;

  const parts: string[] = [];
  const hasCtrl = isMac ? e.metaKey : e.ctrlKey;
  if (hasCtrl) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");
  parts.push(e.key.length === 1 ? e.key.toLowerCase() : e.key);

  return parts.join("+");
}

// --- Display formatting ---

/** Format a key part for display (platform-aware). */
function formatKeyPart(part: string): string {
  if (part === "Ctrl") return isMac ? "\u2318" : "Ctrl";
  if (part === "Shift") return isMac ? "\u21E7" : "Shift";
  if (part === "Alt") return isMac ? "\u2325" : "Alt";
  if (part === "Tab") return "Tab";
  if (part === "Enter") return "\u21B5";
  if (part === "PageDown") return "PgDn";
  if (part === "PageUp") return "PgUp";
  if (part === "Scroll") return "Scroll";
  if (part === "=") return "+";
  if (part === "-") return "\u2212";
  if (part.length === 1 && /[a-z]/.test(part)) return part.toUpperCase();
  return part;
}

/** Split a binding string into display-ready parts (for rendering as <kbd> badges). */
export function formatBindingParts(keys: string): string[] {
  return keys.split("+").map(formatKeyPart);
}
