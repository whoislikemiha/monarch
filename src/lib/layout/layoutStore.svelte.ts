/**
 * Layout state for the dockable inspector panels. Slice 6 ships a right dock
 * (stack of open panels) shared across views, with per-panel open state, a
 * resizable dock width, and a "pinned" flag. Persisted to ui_state so the
 * arrangement survives restarts.
 *
 * Pinned panels stay open across agent + view switches (they re-bind to the
 * active agent). The registry-driven design leaves room for a bottom dock and
 * full drag-split docking later without touching panel components.
 */
import { invoke } from "$lib/api";

const OPEN_KEY = "v2.layout.openPanels";
const WIDTH_KEY = "v2.layout.dockWidth";
const PIN_KEY = "v2.layout.pinnedPanels";
const HEIGHTS_KEY = "v2.layout.panelHeights";
const TLFRAC_KEY = "v2.layout.timelineFrac";
const BOARDFRAC_KEY = "v2.layout.boardFrac";
const ORIENT_KEY = "v2.layout.workspaceOrient";

const DEFAULT_WIDTH = 320;
const MIN_WIDTH = 240;
const MAX_WIDTH = 720;
const DEFAULT_PANEL_HEIGHT = 280;
const MIN_PANEL_HEIGHT = 120;
const MIN_TL_FRAC = 0.2;
const MAX_TL_FRAC = 0.8;

class LayoutStore {
  /** Open panel ids in the right dock, top → bottom. */
  openPanels: string[] = $state([]);
  /** Right dock width in px. */
  dockWidth = $state(DEFAULT_WIDTH);
  /** Pinned panel ids (kept open across switches). */
  pinned: string[] = $state([]);
  /** Per-panel dock height in px (the last panel flexes to fill). */
  panelHeights: Record<string, number> = $state({});
  /** Timeline pane share of the solo workspace split (0.2–0.8). */
  timelineFrac = $state(0.5);
  /** Campaign-tree share of the Projects board split (0.2–0.8). */
  boardFrac = $state(0.42);
  /** Workspace tile-stack orientation: "h" = side-by-side, "v" = stacked. */
  workspaceOrient: "h" | "v" = $state("h");

  private initialized = false;

  async init(): Promise<void> {
    try {
      const open = await invoke<string | null>("db_get_ui_state", { key: OPEN_KEY });
      if (open) this.openPanels = JSON.parse(open);
    } catch {}
    try {
      const w = await invoke<string | null>("db_get_ui_state", { key: WIDTH_KEY });
      if (w) this.dockWidth = clamp(parseInt(w, 10) || DEFAULT_WIDTH);
    } catch {}
    try {
      const p = await invoke<string | null>("db_get_ui_state", { key: PIN_KEY });
      if (p) this.pinned = JSON.parse(p);
    } catch {}
    try {
      const h = await invoke<string | null>("db_get_ui_state", { key: HEIGHTS_KEY });
      if (h) this.panelHeights = JSON.parse(h);
    } catch {}
    try {
      const f = await invoke<string | null>("db_get_ui_state", { key: TLFRAC_KEY });
      if (f) this.timelineFrac = clampFrac(parseFloat(f) || 0.5);
    } catch {}
    try {
      const b = await invoke<string | null>("db_get_ui_state", { key: BOARDFRAC_KEY });
      if (b) this.boardFrac = clampFrac(parseFloat(b) || 0.42);
    } catch {}
    try {
      const o = await invoke<string | null>("db_get_ui_state", { key: ORIENT_KEY });
      if (o === "h" || o === "v") this.workspaceOrient = o;
    } catch {}
    this.initialized = true;
  }

  toggleOrient(): void {
    this.workspaceOrient = this.workspaceOrient === "h" ? "v" : "h";
    if (this.initialized) invoke("db_set_ui_state", { key: ORIENT_KEY, value: this.workspaceOrient }).catch(() => {});
  }

  reorderPanels(fromIdx: number, toIdx: number): void {
    const panels = [...this.openPanels];
    if (fromIdx < 0 || fromIdx >= panels.length || toIdx < 0 || toIdx >= panels.length) return;
    const [moved] = panels.splice(fromIdx, 1);
    panels.splice(toIdx, 0, moved);
    this.openPanels = panels;
    this.persistOpen();
  }

  panelHeight(id: string): number {
    return this.panelHeights[id] ?? DEFAULT_PANEL_HEIGHT;
  }

  setPanelHeight(id: string, px: number): void {
    this.panelHeights = { ...this.panelHeights, [id]: Math.max(MIN_PANEL_HEIGHT, px) };
    if (this.initialized) {
      invoke("db_set_ui_state", { key: HEIGHTS_KEY, value: JSON.stringify(this.panelHeights) }).catch(() => {});
    }
  }

  setTimelineFrac(frac: number): void {
    this.timelineFrac = clampFrac(frac);
    if (this.initialized) {
      invoke("db_set_ui_state", { key: TLFRAC_KEY, value: this.timelineFrac.toFixed(4) }).catch(() => {});
    }
  }

  setBoardFrac(frac: number): void {
    this.boardFrac = clampFrac(frac);
    if (this.initialized) {
      invoke("db_set_ui_state", { key: BOARDFRAC_KEY, value: this.boardFrac.toFixed(4) }).catch(() => {});
    }
  }

  isOpen(id: string): boolean {
    return this.openPanels.includes(id);
  }

  isPinned(id: string): boolean {
    return this.pinned.includes(id);
  }

  toggle(id: string): void {
    this.openPanels = this.isOpen(id)
      ? this.openPanels.filter((p) => p !== id)
      : [...this.openPanels, id];
    this.persistOpen();
  }

  close(id: string): void {
    this.openPanels = this.openPanels.filter((p) => p !== id);
    this.pinned = this.pinned.filter((p) => p !== id);
    this.persistOpen();
    this.persistPinned();
  }

  togglePin(id: string): void {
    this.pinned = this.isPinned(id)
      ? this.pinned.filter((p) => p !== id)
      : [...this.pinned, id];
    this.persistPinned();
  }

  setWidth(px: number): void {
    this.dockWidth = clamp(px);
    if (this.initialized) {
      invoke("db_set_ui_state", { key: WIDTH_KEY, value: String(Math.round(this.dockWidth)) }).catch(() => {});
    }
  }

  private persistOpen(): void {
    if (!this.initialized) return;
    invoke("db_set_ui_state", { key: OPEN_KEY, value: JSON.stringify(this.openPanels) }).catch(() => {});
  }

  private persistPinned(): void {
    if (!this.initialized) return;
    invoke("db_set_ui_state", { key: PIN_KEY, value: JSON.stringify(this.pinned) }).catch(() => {});
  }
}

function clamp(px: number): number {
  return Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, px));
}

function clampFrac(f: number): number {
  return Math.max(MIN_TL_FRAC, Math.min(MAX_TL_FRAC, f));
}

export const layoutStore = new LayoutStore();
