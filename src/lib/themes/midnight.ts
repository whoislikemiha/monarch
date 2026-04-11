import type { Theme } from "./types";

export const midnight: Theme = {
  name: "midnight",
  label: "Midnight",

  // Surfaces — deep navy backgrounds
  bgApp: "#0a1628",
  bgAppGlow: "#0f2240",
  bgSidebar: "#060e1e",
  bgPanel: "#0d1a30",
  bgPanel2: "#142440",
  bgPanel3: "#1a2e50",
  bgCode: "#080f20",

  // Borders
  borderSubtle: "#1e3455",
  borderStrong: "#2a4872",

  // Text
  textPrimary: "#e8eef8",
  textSecondary: "#b4c4da",
  textMuted: "#6080a8",
  textOnAccent: "#060e1e",

  // Accent — steel blue
  accent: "#4a9eff",
  accentHover: "#7ab8ff",
  accentLight: "#a8d0ff",
  accentBgSubtle: "rgba(74, 158, 255, 0.06)",
  accentBgHover: "rgba(74, 158, 255, 0.12)",
  accentBorderSubtle: "rgba(74, 158, 255, 0.2)",
  accentBorderHover: "rgba(74, 158, 255, 0.4)",

  // Blue (secondary)
  accentBlue: "#5daaff",
  accentBlueHover: "#82beff",
  accentBlueBgSubtle: "rgba(93, 170, 255, 0.06)",
  accentBlueBorderSubtle: "rgba(93, 170, 255, 0.15)",

  // Cyan
  accentCyan: "#40c4c0",

  // Success
  success: "#3eb370",
  successGlow: "rgba(62, 179, 112, 0.5)",
  successBgSubtle: "rgba(62, 179, 112, 0.08)",

  // Warning
  warning: "#f0d060",
  warningGlow: "rgba(240, 208, 96, 0.32)",
  warningBgSubtle: "rgba(240, 208, 96, 0.06)",
  warningBgFaint: "rgba(240, 208, 96, 0.03)",
  warningBorderSubtle: "rgba(240, 208, 96, 0.2)",
  warningBorderFaint: "rgba(240, 208, 96, 0.1)",

  // Error
  error: "#e85577",
  errorLight: "#ffb0b0",
  errorGlow: "rgba(232, 85, 119, 0.32)",
  errorBgSubtle: "rgba(232, 85, 119, 0.06)",
  errorBgFaint: "rgba(232, 85, 119, 0.12)",
  errorBorderSubtle: "rgba(232, 85, 119, 0.15)",
  errorBorderFaint: "rgba(232, 85, 119, 0.35)",

  // Diff
  diffAddBg: "rgba(62, 179, 112, 0.12)",
  diffAddText: "#3eb370",
  diffDelBg: "rgba(232, 85, 119, 0.12)",
  diffDelText: "#e85577",

  // Overlays
  overlayBackdrop: "rgba(0, 0, 0, 0.65)",
  shadowDark: "rgba(0, 0, 0, 0.55)",
  shadowInsetWhite: "rgba(255, 255, 255, 0.03)",
  scrollbarThumb: "rgba(30, 52, 85, 0.5)",

  // Interactive
  inputInsetShadow: "rgba(255, 255, 255, 0.02)",

  // Context meter
  contextTrackBg: "rgba(6, 10, 20, 0.72)",
  contextTrackInset: "rgba(255, 255, 255, 0.03)",
  contextTrackOverlay: "rgba(255, 255, 255, 0.04)",

  // Misc overlays
  hoverOverlay: "rgba(255, 255, 255, 0.03)",
  activeOverlay: "rgba(255, 255, 255, 0.05)",
  subtleDivider: "rgba(255, 255, 255, 0.03)",

  // Auth status
  authOkBorder: "rgba(62, 179, 112, 0.4)",
  authOkBg: "rgba(15, 45, 30, 0.45)",
  authWarnBorder: "rgba(240, 208, 96, 0.35)",
  authWarnBg: "rgba(50, 40, 15, 0.4)",

  // Model error
  modelErrorBorder: "rgba(255, 120, 120, 0.4)",
  modelErrorBg: "rgba(50, 18, 18, 0.45)",
  modelErrorText: "#ffb0b0",
  modelErrorRetryBorder: "rgba(255, 176, 176, 0.5)",

  // Resize handle
  resizeHandleActive: "rgba(74, 158, 255, 0.35)",

  // Template chip delete
  chipDeleteHoverBg: "rgba(255, 120, 120, 0.15)",
  chipDeleteHoverText: "#ffb0b0",

  // Unsaved badge
  unsavedBadgeBg: "rgba(240, 208, 96, 0.15)",
};
