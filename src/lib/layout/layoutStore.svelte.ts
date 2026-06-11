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

const DEFAULT_WIDTH = 320;
const MIN_WIDTH = 240;
const MAX_WIDTH = 560;

class LayoutStore {
  /** Open panel ids in the right dock, top → bottom. */
  openPanels: string[] = $state([]);
  /** Right dock width in px. */
  dockWidth = $state(DEFAULT_WIDTH);
  /** Pinned panel ids (kept open across switches). */
  pinned: string[] = $state([]);

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
    this.initialized = true;
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

export const layoutStore = new LayoutStore();
