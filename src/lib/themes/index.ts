import type { Theme, ThemeId } from "./types";
import { purple } from "./purple";
import { obsidian } from "./obsidian";
import { midnight } from "./midnight";
import { light } from "./light";

export type { Theme, ThemeId };

/** All registered themes, keyed by their id */
export const themes: Record<ThemeId, Theme> = {
  purple,
  obsidian,
  midnight,
  light,
};

export const DEFAULT_THEME: ThemeId = "purple";

/** Currently active theme — readable from JS without getComputedStyle */
let current: Theme = purple;

export function getActiveTheme(): Theme {
  return current;
}

/**
 * Maps a camelCase Theme key to its CSS variable name.
 * e.g. "bgApp" → "--bg-app", "accentBlueBgSubtle" → "--accent-blue-bg-subtle"
 */
function toVarName(key: string): string {
  return "--" + key
    .replace(/([A-Z])/g, "-$1")
    .replace(/([a-zA-Z])(\d)/g, "$1-$2")
    .toLowerCase();
}

/**
 * Apply a theme by setting CSS custom properties on :root.
 * Returns the resolved theme id (falls back to default if unknown).
 */
export function applyTheme(id: ThemeId): ThemeId {
  const theme = themes[id] ?? themes[DEFAULT_THEME];
  const resolvedId = themes[id] ? id : DEFAULT_THEME;
  current = theme;

  const style = document.documentElement.style;
  for (const [key, value] of Object.entries(theme)) {
    if (key === "name" || key === "label") continue;
    style.setProperty(toVarName(key), value);
  }

  // Also set body background for the rare pre-paint moment
  document.body.style.background = theme.bgApp;

  return resolvedId;
}

/** List of themes for the UI picker */
export function listThemes(): { id: ThemeId; label: string; theme: Theme }[] {
  return Object.entries(themes).map(([id, theme]) => ({
    id,
    label: theme.label,
    theme,
  }));
}
