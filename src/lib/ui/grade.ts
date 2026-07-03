/**
 * Maps the agent seniority ladder onto the design-system E→S grade ramp.
 * Seniority tiers are the product vocabulary; the E–S letters + `--grade-*` tokens
 * are the visual rarity ladder (see global.css / theme files).
 */
import type { ShadowGrade } from "$lib/types";

export type GradeLetter = "E" | "D" | "C" | "B" | "A" | "S";

const RANK_TO_GRADE: Record<ShadowGrade, GradeLetter> = {
  Intern: "E",
  Trainee: "D",
  Junior: "C",
  Mid: "B",
  Senior: "A",
  Staff: "S",
  Principal: "S",
};

export function gradeLetter(rank: ShadowGrade | undefined | null): GradeLetter {
  return rank ? RANK_TO_GRADE[rank] ?? "C" : "C";
}

/** CSS var for a grade letter, e.g. "var(--grade-b)". */
export function gradeColor(letter: GradeLetter): string {
  return `var(--grade-${letter.toLowerCase()})`;
}
