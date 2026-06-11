/**
 * Top-level shell state for the v2 command center.
 *
 * Owns the active lens (Agents ⇄ Projects) and the active theme. Both are
 * persisted to SQLite `ui_state` so the shell restores between launches.
 * Kept deliberately small — per-agent / per-panel state lives in the existing
 * domain stores (agentStore, liveAgentStore, objectiveStore) and in the
 * layout store (slice 6).
 *
 * Class-based for the same reason as agentStore: Svelte 5 forbids exporting
 * reassigned module-level `$state`.
 */

import { invoke } from "$lib/api";
import { applyTheme, DEFAULT_THEME, type ThemeId } from "$lib/themes";

export type ViewId = "agents" | "projects";

const ACTIVE_VIEW_KEY = "v2.activeView";
const THEME_KEY = "theme";

class ViewStore {
  /** The active top-level lens. */
  activeView: ViewId = $state("agents");
  /** The active theme id — drives the TopBar switcher. */
  themeId: ThemeId = $state(DEFAULT_THEME);

  /**
   * Restore persisted view + theme. Theme is also applied here (in addition to
   * the FOUC-blocking script in index.html) so the in-memory `themeId` and the
   * applied CSS vars agree on first paint.
   */
  async init(): Promise<void> {
    try {
      const v = await invoke<string | null>("db_get_ui_state", { key: ACTIVE_VIEW_KEY });
      if (v === "agents" || v === "projects") this.activeView = v;
    } catch {}
    try {
      const t = await invoke<string | null>("db_get_ui_state", { key: THEME_KEY });
      if (t) {
        const id = JSON.parse(t) as ThemeId;
        this.themeId = applyTheme(id);
      }
    } catch {}
  }

  setView(v: ViewId): void {
    if (v === this.activeView) return;
    this.activeView = v;
    invoke("db_set_ui_state", { key: ACTIVE_VIEW_KEY, value: v }).catch(() => {});
  }

  setTheme(id: ThemeId): void {
    this.themeId = applyTheme(id);
    invoke("db_set_ui_state", { key: THEME_KEY, value: JSON.stringify(this.themeId) }).catch(() => {});
  }
}

export const viewStore = new ViewStore();
