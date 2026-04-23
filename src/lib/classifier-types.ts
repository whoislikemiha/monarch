/**
 * MON-82 — shared frontend types for the classifier pill + settings tool.
 * Kept outside `classifierStore.svelte.ts` so non-reactive imports
 * (plain .ts files) can pull the types without bringing in Svelte runes.
 */

export type ComplexityLabel =
  | "chitchat"
  | "simple"
  | "decomposable"
  | "delegate";

export const COMPLEXITY_LABELS: ComplexityLabel[] = [
  "chitchat",
  "simple",
  "decomposable",
  "delegate",
];

/** Hex colors per label — distinct enough at a glance, not jarring. */
export const COMPLEXITY_COLORS: Record<ComplexityLabel, string> = {
  chitchat: "#9aa0a6",
  simple: "#4ea7fc",
  decomposable: "#f2994a",
  delegate: "#bb87fc",
};

export const COMPLEXITY_DESCRIPTIONS: Record<ComplexityLabel, string> = {
  chitchat: "Social / meta — no task work needed",
  simple: "Single focused turn",
  decomposable: "Plan-worthy multi-step work",
  delegate: "Parallel sub-tasks across areas",
};
