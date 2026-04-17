import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEDUP_WINDOW_MS,
  DEFAULT_INFO_MS,
  DEFAULT_WARNING_MS,
  notificationsStore,
} from "./notificationsStore.svelte";

describe("notificationsStore", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    notificationsStore.dismissAll();
  });

  afterEach(() => {
    notificationsStore.dismissAll();
    vi.useRealTimers();
  });

  it("add pushes an entry with a unique id, level, message, and count=1", () => {
    const id = notificationsStore.add({ level: "error", message: "boom" });
    expect(notificationsStore.notifications).toHaveLength(1);
    const entry = notificationsStore.notifications[0];
    expect(entry.id).toBe(id);
    expect(entry.level).toBe("error");
    expect(entry.message).toBe("boom");
    expect(entry.count).toBe(1);
  });

  it("error-level notifications never auto-expire", () => {
    notificationsStore.add({ level: "error", message: "persistent" });
    vi.advanceTimersByTime(10 * 60 * 1000);
    expect(notificationsStore.notifications).toHaveLength(1);
  });

  it("warning auto-expires at DEFAULT_WARNING_MS", () => {
    notificationsStore.add({ level: "warning", message: "heads up" });
    vi.advanceTimersByTime(DEFAULT_WARNING_MS - 1);
    expect(notificationsStore.notifications).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(notificationsStore.notifications).toHaveLength(0);
  });

  it("info auto-expires at DEFAULT_INFO_MS", () => {
    notificationsStore.add({ level: "info", message: "fyi" });
    vi.advanceTimersByTime(DEFAULT_INFO_MS - 1);
    expect(notificationsStore.notifications).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(notificationsStore.notifications).toHaveLength(0);
  });

  it("dismiss removes the entry and clears its pending timer", () => {
    const id = notificationsStore.add({ level: "warning", message: "x" });
    notificationsStore.dismiss(id);
    expect(notificationsStore.notifications).toHaveLength(0);
    // If the timer wasn't cleared, advancing past expiry would throw when it
    // tries to dismiss an already-removed entry. It should be a no-op.
    expect(() => vi.advanceTimersByTime(DEFAULT_WARNING_MS + 1)).not.toThrow();
    expect(notificationsStore.notifications).toHaveLength(0);
  });

  it("dedupes identical (level, message, agentId) within DEDUP_WINDOW_MS: increments count, resets timer", () => {
    const firstId = notificationsStore.add({
      level: "warning",
      message: "rate limited",
      agentId: "agent-1",
    });
    // Move halfway through the warning's original timer, then dedup.
    vi.advanceTimersByTime(DEFAULT_WARNING_MS / 2);
    const secondId = notificationsStore.add({
      level: "warning",
      message: "rate limited",
      agentId: "agent-1",
    });
    expect(secondId).toBe(firstId);
    expect(notificationsStore.notifications).toHaveLength(1);
    expect(notificationsStore.notifications[0].count).toBe(2);

    // The timer should have been reset — advancing by slightly less than a
    // full window from the second add shouldn't expire it.
    vi.advanceTimersByTime(DEFAULT_WARNING_MS - 1);
    expect(notificationsStore.notifications).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(notificationsStore.notifications).toHaveLength(0);
  });

  it("outside the dedup window, a fresh entry is pushed", () => {
    notificationsStore.add({ level: "error", message: "same", agentId: "a" });
    vi.advanceTimersByTime(DEDUP_WINDOW_MS + 1);
    notificationsStore.add({ level: "error", message: "same", agentId: "a" });
    expect(notificationsStore.notifications).toHaveLength(2);
  });

  it("differentiates by agentId when deduping", () => {
    notificationsStore.add({ level: "error", message: "m", agentId: "a" });
    notificationsStore.add({ level: "error", message: "m", agentId: "b" });
    expect(notificationsStore.notifications).toHaveLength(2);
  });

  it("pauseExpiry stops the timer; resumeExpiry restarts it with the full duration", () => {
    notificationsStore.add({ level: "info", message: "hover" });
    const id = notificationsStore.notifications[0].id;
    vi.advanceTimersByTime(DEFAULT_INFO_MS / 2);
    notificationsStore.pauseExpiry(id);
    vi.advanceTimersByTime(DEFAULT_INFO_MS * 5);
    expect(notificationsStore.notifications).toHaveLength(1);
    notificationsStore.resumeExpiry(id);
    vi.advanceTimersByTime(DEFAULT_INFO_MS - 1);
    expect(notificationsStore.notifications).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(notificationsStore.notifications).toHaveLength(0);
  });

  it("dismissAll empties the store and cancels every pending timer", () => {
    notificationsStore.add({ level: "warning", message: "a" });
    notificationsStore.add({ level: "info", message: "b" });
    notificationsStore.add({ level: "error", message: "c" });
    expect(notificationsStore.notifications).toHaveLength(3);
    notificationsStore.dismissAll();
    expect(notificationsStore.notifications).toHaveLength(0);
    expect(() => vi.advanceTimersByTime(60_000)).not.toThrow();
    expect(notificationsStore.notifications).toHaveLength(0);
  });
});
