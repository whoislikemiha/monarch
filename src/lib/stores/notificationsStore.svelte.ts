/**
 * MON-51: app-wide error/warning surface.
 *
 * Today Monarch has several error sources (spawn failures, `sidecar_error`
 * events, non-zero agent exits) that either land in a per-agent stderr panel
 * or get swallowed by `console.error`. This store drives a single overlay
 * (`<NotificationStack>`) so the operator sees problems without having to
 * open the right agent view or devtools.
 *
 * Behaviour:
 *   - `error`-level entries persist until dismissed.
 *   - `warning` / `info` auto-expire after `DEFAULT_WARNING_MS` /
 *     `DEFAULT_INFO_MS`.
 *   - Identical `(level, message, agentId)` within `DEDUP_WINDOW_MS`
 *     collapses into the existing entry: `count` increments, `createdAt`
 *     refreshes, the expiry timer resets. Prevents 50-toast error loops.
 *   - `pauseExpiry` / `resumeExpiry` let the component stop the clock while
 *     the user hovers a card.
 *
 * Out of scope (see `thoughts/plan/MON-51.md`): no persistence, no OS
 * notifications, no component `$effect` wiring — timers are plain
 * `setTimeout` and work fine at module scope, so no `setupEffects()`.
 */
export type NotificationLevel = "error" | "warning" | "info";

export interface NotificationInput {
  level: NotificationLevel;
  message: string;
  agentId?: string;
  agentName?: string;
}

export interface Notification extends NotificationInput {
  id: string;
  createdAt: number;
  count: number;
}

export const DEFAULT_WARNING_MS = 6_000;
export const DEFAULT_INFO_MS = 4_000;
export const DEDUP_WINDOW_MS = 5_000;

class NotificationsStore {
  notifications = $state<Notification[]>([]);
  private timers = new Map<string, ReturnType<typeof setTimeout>>();

  add(input: NotificationInput): string {
    const now = Date.now();
    const match = this.notifications.find(
      (n) =>
        n.level === input.level &&
        n.message === input.message &&
        (n.agentId ?? null) === (input.agentId ?? null) &&
        now - n.createdAt <= DEDUP_WINDOW_MS,
    );

    if (match) {
      const id = match.id;
      this.notifications = this.notifications.map((n) =>
        n.id === id ? { ...n, count: n.count + 1, createdAt: now } : n,
      );
      this.scheduleExpiry(id, input.level);
      return id;
    }

    const id = `notif-${now}-${Math.random().toString(36).slice(2, 8)}`;
    const entry: Notification = {
      id,
      createdAt: now,
      count: 1,
      level: input.level,
      message: input.message,
      agentId: input.agentId,
      agentName: input.agentName,
    };
    this.notifications = [...this.notifications, entry];
    this.scheduleExpiry(id, input.level);
    return id;
  }

  dismiss(id: string): void {
    this.clearTimer(id);
    this.notifications = this.notifications.filter((n) => n.id !== id);
  }

  dismissAll(): void {
    for (const timer of this.timers.values()) clearTimeout(timer);
    this.timers.clear();
    this.notifications = [];
  }

  pauseExpiry(id: string): void {
    this.clearTimer(id);
  }

  resumeExpiry(id: string): void {
    const entry = this.notifications.find((n) => n.id === id);
    if (entry) this.scheduleExpiry(id, entry.level);
  }

  private scheduleExpiry(id: string, level: NotificationLevel): void {
    this.clearTimer(id);
    if (level === "error") return;
    const ms = level === "warning" ? DEFAULT_WARNING_MS : DEFAULT_INFO_MS;
    this.timers.set(
      id,
      setTimeout(() => this.dismiss(id), ms),
    );
  }

  private clearTimer(id: string): void {
    const timer = this.timers.get(id);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(id);
    }
  }
}

export const notificationsStore = new NotificationsStore();
export type { NotificationsStore };
