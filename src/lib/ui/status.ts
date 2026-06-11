/**
 * Objective status → label + tone + status-dot shape class (`.sdot` variants).
 * Status is paired with shape AND label, never color alone (house rule).
 */
export type StatusTone = "muted" | "info" | "success" | "warning" | "error";

export interface StatusView {
  label: string;
  tone: StatusTone;
  /** `.sdot` modifier class: idle | running | success | warning | error. */
  dot: string;
}

export function objectiveStatus(status: string): StatusView {
  switch (status) {
    case "pending":
      return { label: "Planned", tone: "muted", dot: "idle" };
    case "in_progress":
    case "active":
      return { label: "In progress", tone: "info", dot: "running" };
    case "blocked":
      return { label: "Blocked", tone: "warning", dot: "warning" };
    case "completed":
    case "done":
      return { label: "Done", tone: "success", dot: "success" };
    case "abandoned":
    case "skipped":
      return { label: "Abandoned", tone: "muted", dot: "idle" };
    default:
      return { label: status, tone: "muted", dot: "idle" };
  }
}

export function planItemStatus(status: string): StatusView {
  switch (status) {
    case "active":
    case "in_progress":
      return { label: "Now", tone: "info", dot: "running" };
    case "completed":
    case "done":
      return { label: "Done", tone: "success", dot: "success" };
    case "blocked":
      return { label: "Blocked", tone: "warning", dot: "warning" };
    case "skipped":
      return { label: "Skipped", tone: "muted", dot: "idle" };
    default:
      return { label: "Next", tone: "muted", dot: "idle" };
  }
}
