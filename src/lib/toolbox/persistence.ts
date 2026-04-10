const WIDTH_KEY = "monarch.toolbox.width";
const OPEN_IDS_KEY = "monarch.toolbox.openIds";

export const TOOLBOX_MIN_WIDTH = 240;
export const TOOLBOX_MAX_WIDTH = 600;
export const TOOLBOX_DEFAULT_WIDTH = 320;

export function clampWidth(width: number): number {
  if (!Number.isFinite(width)) return TOOLBOX_DEFAULT_WIDTH;
  return Math.min(TOOLBOX_MAX_WIDTH, Math.max(TOOLBOX_MIN_WIDTH, Math.round(width)));
}

export function restoreWidth(): number {
  try {
    const raw = localStorage.getItem(WIDTH_KEY);
    if (raw == null) return TOOLBOX_DEFAULT_WIDTH;
    const parsed = Number.parseInt(raw, 10);
    return clampWidth(parsed);
  } catch {
    return TOOLBOX_DEFAULT_WIDTH;
  }
}

export function persistWidth(width: number): void {
  try {
    localStorage.setItem(WIDTH_KEY, String(clampWidth(width)));
  } catch {
    // localStorage unavailable — silently ignore
  }
}

export function restoreOpenIds(): string[] {
  try {
    const raw = localStorage.getItem(OPEN_IDS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

export function persistOpenIds(ids: string[]): void {
  try {
    localStorage.setItem(OPEN_IDS_KEY, JSON.stringify(ids));
  } catch {
    // localStorage unavailable — silently ignore
  }
}
