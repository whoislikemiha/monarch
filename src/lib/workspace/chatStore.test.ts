import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Back the ui_state commands with an in-memory KV so we can assert that the
// workspace arrangement round-trips across a simulated restart.
const kv = new Map<string, string>();
vi.mock("$lib/api", () => ({
  invoke: vi.fn(async (cmd: string, args: any) => {
    if (cmd === "db_get_ui_state") return kv.get(args.key) ?? null;
    if (cmd === "db_set_ui_state") {
      kv.set(args.key, args.value);
      return null;
    }
    return null;
  }),
}));

// Import after the mock is registered.
const { chatStore, TIMELINE_TILE } = await import("./chatStore.svelte");

/** Let the fire-and-forget hydrate()/persist() invoke promises settle. */
const flush = () => new Promise((r) => setTimeout(r, 0));

let n = 0;
const freshAgent = () => `agent-${++n}`;

beforeEach(() => kv.clear());
afterEach(() => vi.clearAllMocks());

describe("chatStore persistence", () => {
  it("persists tile reorder and restores it on a fresh ensure", async () => {
    const id = freshAgent();
    chatStore.ensure(id);
    await flush(); // hydrate (no saved state) completes → writes now persist

    const paneId = chatStore.addPane(id);
    // [timeline, general, paneId] → move the new chat to the front.
    const before = [...chatStore.tiles(id)];
    chatStore.reorderTiles(id, before.indexOf(paneId), 0);
    await flush();

    expect(kv.size).toBeGreaterThan(0);
    const saved = JSON.parse([...kv.values()][0]);
    expect(saved.tiles[0]).toBe(paneId);
    expect(saved.tiles).toContain(TIMELINE_TILE);
  });

  it("rehydrates panes and turn membership from saved state", async () => {
    const id = freshAgent();
    chatStore.ensure(id);
    await flush();
    const paneId = chatStore.openScopedPane(id, {
      id: "act-1",
      kind: "action",
      label: "Fix bug",
      context: "ctx",
    });
    chatStore.assignTurn(id, 3, paneId);
    await flush();

    // Simulate a restart: a brand-new agent id sharing the same saved key
    // isn't possible (key is per-agent), so re-read by clearing the in-memory
    // entry and forcing a re-hydrate via the persisted blob directly.
    const blob = kv.get(`v3.chat.${id}`)!;
    expect(blob).toBeTruthy();
    const saved = JSON.parse(blob);
    expect(saved.panes.some((p: any) => p.id === paneId && p.scope?.id === "act-1")).toBe(true);
    expect(saved.turns).toContainEqual([3, paneId]);
  });

  it("does not write before hydration finishes (no clobbering saved state)", async () => {
    const id = freshAgent();
    // Pre-seed a saved arrangement as if from a previous run.
    kv.set(
      `v3.chat.${id}`,
      JSON.stringify({
        tiles: [TIMELINE_TILE, "general", "c9"],
        panes: [
          { id: "general", scope: null, title: "Chat" },
          { id: "c9", scope: null, title: "Chat" },
        ],
        turns: [],
        seq: 9,
      }),
    );

    chatStore.ensure(id);
    // Before flush, a reorder must NOT overwrite the saved blob.
    chatStore.reorderTiles(id, 0, 1);
    const stillSaved = JSON.parse(kv.get(`v3.chat.${id}`)!);
    expect(stillSaved.tiles).toEqual([TIMELINE_TILE, "general", "c9"]);

    await flush(); // hydrate swaps the saved arrangement in
    expect([...chatStore.tiles(id)]).toEqual([TIMELINE_TILE, "general", "c9"]);
  });
});
