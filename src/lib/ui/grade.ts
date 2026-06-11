/**
 * Maps the shadow rank ladder onto the design-system E→S grade ramp.
 * Rank names are the product vocabulary; the E–S letters + `--grade-*` tokens
 * are the visual rarity ladder (see global.css / theme files).
 */
import type { ShadowGrade } from "$lib/types";

export type GradeLetter = "E" | "D" | "C" | "B" | "A" | "S";

const RANK_TO_GRADE: Record<ShadowGrade, GradeLetter> = {
  Normal: "E",
  Elite: "D",
  Knight: "C",
  "Elite Knight": "B",
  General: "A",
  Marshal: "S",
  "Grand Marshal": "S",
};

export function gradeLetter(rank: ShadowGrade | undefined | null): GradeLetter {
  return rank ? RANK_TO_GRADE[rank] ?? "C" : "C";
}

/** CSS var for a grade letter, e.g. "var(--grade-b)". */
export function gradeColor(letter: GradeLetter): string {
  return `var(--grade-${letter.toLowerCase()})`;
}
