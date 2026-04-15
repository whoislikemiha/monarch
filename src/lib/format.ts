/**
 * Shared display formatters. Keep render decisions here so message lists,
 * history panels, sidebar rows, and toolbox tools all present numbers
 * consistently.
 */

/**
 * Format a USD cost. Returns `null` for zero so callers can hide the chip
 * entirely on free-provider runs (local LM Studio, etc.) instead of cluttering
 * the UI with meaningless `$0.0000` values.
 *
 * For very small non-zero amounts (< $0.0001, which `toFixed(4)` would round
 * to `$0.0000`) a `<$0.0001` indicator is returned so the user can tell the
 * call wasn't free — just cheap.
 */
export function formatCost(n: number | null | undefined): string | null {
  if (n == null || n <= 0) return null;
  if (n < 0.0001) return "<$0.0001";
  return "$" + n.toFixed(4);
}

/**
 * Format a wall-clock duration for the chat-header chips (turn, tool call,
 * thinking block). Accepts milliseconds so callers can feed `now - startedAt`
 * directly from the live ticker without converting.
 *
 * Returns `null` for sub-1-second durations and null-ish inputs so callers can
 * hide the chip entirely. This keeps very fast tool calls (200 ms reads,
 * trivial greps) from cluttering the header — and makes an absent chip
 * unambiguous shorthand for "too fast to measure" vs. a historical message
 * that pre-dates duration persistence (which renders the same way).
 *
 * Formats:
 *   < 1 sec      → null (no chip)
 *   < 60 sec     → "15 sec"
 *   < 1 hour     → "2 min 14 sec"  (drops "0 sec" on minute boundaries)
 *   >= 1 hour    → "1 hr 30 min"
 */
export function formatDuration(ms: number | null | undefined): string | null {
  if (ms == null || !Number.isFinite(ms) || ms < 1000) return null;
  const totalSec = Math.floor(ms / 1000);
  if (totalSec < 60) return `${totalSec} sec`;
  const totalMin = Math.floor(totalSec / 60);
  if (totalMin < 60) {
    const sec = totalSec % 60;
    return sec === 0 ? `${totalMin} min` : `${totalMin} min ${sec} sec`;
  }
  const hr = Math.floor(totalMin / 60);
  const min = totalMin % 60;
  return min === 0 ? `${hr} hr` : `${hr} hr ${min} min`;
}
