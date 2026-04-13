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
