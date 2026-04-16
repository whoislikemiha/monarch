import type { Theme } from "./types";

export const obsidian: Theme = {
  name: "obsidian",
  label: "Obsidian",

  // Surfaces — neutral dark greys, no purple tint
  bgApp: "#141414",
  bgAppGlow: "#1e1e1e",
  bgSidebar: "#0e0e0e",
  bgPanel: "#1a1a1a",
  bgPanel2: "#222222",
  bgPanel3: "#2c2c2c",
  bgCode: "#111111",

  // Borders
  borderSubtle: "#333333",
  borderStrong: "#4a4a4a",

  // Text
  textPrimary: "#e8e8e8",
  textSecondary: "#b0b0b0",
  textMuted: "#707070",
  textOnAccent: "#0e0e0e",

  // Accent — cool blue-grey
  accent: "#8ab4f8",
  accentHover: "#aecbfa",
  accentLight: "#c6dbfa",
  accentBgSubtle: "rgba(138, 180, 248, 0.06)",
  accentBgHover: "rgba(138, 180, 248, 0.12)",
  accentBorderSubtle: "rgba(138, 180, 248, 0.2)",
  accentBorderHover: "rgba(138, 180, 248, 0.4)",

  // Blue
  accentBlue: "#66b3ff",
  accentBlueHover: "#8ac4ff",
  accentBlueBgSubtle: "rgba(102, 179, 255, 0.06)",
  accentBlueBorderSubtle: "rgba(102, 179, 255, 0.15)",
  accentBlueBorder: "rgba(102, 179, 255, 0.4)",

  // Cyan
  accentCyan: "#4ecdc4",

  // Success
  success: "#4caf50",
  successGlow: "rgba(76, 175, 80, 0.5)",
  successBgSubtle: "rgba(76, 175, 80, 0.08)",

  // Warning
  warning: "#ffd54f",
  warningGlow: "rgba(255, 213, 79, 0.32)",
  warningBgSubtle: "rgba(255, 213, 79, 0.06)",
  warningBgFaint: "rgba(255, 213, 79, 0.03)",
  warningBorderSubtle: "rgba(255, 213, 79, 0.2)",
  warningBorderFaint: "rgba(255, 213, 79, 0.1)",

  // Error
  error: "#ef5350",
  errorLight: "#ff8a80",
  errorGlow: "rgba(239, 83, 80, 0.32)",
  errorBgSubtle: "rgba(239, 83, 80, 0.06)",
  errorBgFaint: "rgba(239, 83, 80, 0.12)",
  errorBorderSubtle: "rgba(239, 83, 80, 0.15)",
  errorBorderFaint: "rgba(239, 83, 80, 0.35)",

  // Diff
  diffAddBg: "rgba(76, 175, 80, 0.12)",
  diffAddText: "#4caf50",
  diffDelBg: "rgba(239, 83, 80, 0.12)",
  diffDelText: "#ef5350",

  // Overlays
  overlayBackdrop: "rgba(0, 0, 0, 0.65)",
  shadowDark: "rgba(0, 0, 0, 0.55)",
  shadowInsetWhite: "rgba(255, 255, 255, 0.03)",
  scrollbarThumb: "rgba(80, 80, 80, 0.35)",

  // Interactive
  inputInsetShadow: "rgba(255, 255, 255, 0.02)",

  // Context meter
  contextTrackBg: "rgba(10, 10, 10, 0.72)",
  contextTrackInset: "rgba(255, 255, 255, 0.03)",
  contextTrackOverlay: "rgba(255, 255, 255, 0.04)",

  // Misc overlays
  hoverOverlay: "rgba(255, 255, 255, 0.03)",
  activeOverlay: "rgba(255, 255, 255, 0.05)",
  subtleDivider: "rgba(255, 255, 255, 0.03)",

  // Auth status
  authOkBorder: "rgba(76, 175, 80, 0.4)",
  authOkBg: "rgba(20, 50, 25, 0.45)",
  authWarnBorder: "rgba(255, 213, 79, 0.35)",
  authWarnBg: "rgba(60, 45, 12, 0.4)",

  // Model error
  modelErrorBorder: "rgba(255, 120, 120, 0.4)",
  modelErrorBg: "rgba(60, 20, 20, 0.45)",
  modelErrorText: "#ff8a80",
  modelErrorRetryBorder: "rgba(255, 138, 128, 0.5)",

  // Resize handle
  resizeHandleActive: "rgba(138, 180, 248, 0.35)",

  // Template chip delete
  chipDeleteHoverBg: "rgba(255, 120, 120, 0.15)",
  chipDeleteHoverText: "#ff8a80",

  // Unsaved badge
  unsavedBadgeBg: "rgba(255, 213, 79, 0.15)",
};
