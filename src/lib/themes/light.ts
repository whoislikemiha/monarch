import type { Theme } from "./types";

export const light: Theme = {
  name: "light",
  label: "Light",

  // Surfaces — light backgrounds
  bgApp: "#f5f5f7",
  bgAppGlow: "#ecedf0",
  bgSidebar: "#eaeaed",
  bgPanel: "#ffffff",
  bgPanel2: "#f0f0f3",
  bgPanel3: "#e5e5ea",
  bgCode: "#f0f0f3",

  // Borders
  borderSubtle: "#d4d4d8",
  borderStrong: "#b0b0b8",

  // Text
  textPrimary: "#1c1c1e",
  textSecondary: "#3a3a3c",
  textMuted: "#8e8e93",
  textOnAccent: "#ffffff",

  // Accent — vibrant purple
  accent: "#7c3aed",
  accentHover: "#6d28d9",
  accentLight: "#8b5cf6",
  accentBgSubtle: "rgba(124, 58, 237, 0.06)",
  accentBgHover: "rgba(124, 58, 237, 0.10)",
  accentBorderSubtle: "rgba(124, 58, 237, 0.2)",
  accentBorderHover: "rgba(124, 58, 237, 0.35)",

  // Blue
  accentBlue: "#2563eb",
  accentBlueHover: "#1d4ed8",
  accentBlueBgSubtle: "rgba(37, 99, 235, 0.06)",
  accentBlueBorderSubtle: "rgba(37, 99, 235, 0.15)",
  accentBlueBorder: "rgba(37, 99, 235, 0.4)",

  // Cyan
  accentCyan: "#0891b2",

  // Success
  success: "#16a34a",
  successGlow: "rgba(22, 163, 74, 0.25)",
  successBgSubtle: "rgba(22, 163, 74, 0.08)",

  // Warning
  warning: "#ca8a04",
  warningGlow: "rgba(202, 138, 4, 0.2)",
  warningBgSubtle: "rgba(202, 138, 4, 0.06)",
  warningBgFaint: "rgba(202, 138, 4, 0.03)",
  warningBorderSubtle: "rgba(202, 138, 4, 0.2)",
  warningBorderFaint: "rgba(202, 138, 4, 0.1)",

  // Error
  error: "#dc2626",
  errorLight: "#ef4444",
  errorGlow: "rgba(220, 38, 38, 0.2)",
  errorBgSubtle: "rgba(220, 38, 38, 0.06)",
  errorBgFaint: "rgba(220, 38, 38, 0.08)",
  errorBorderSubtle: "rgba(220, 38, 38, 0.15)",
  errorBorderFaint: "rgba(220, 38, 38, 0.25)",

  // Diff
  diffAddBg: "rgba(22, 163, 74, 0.1)",
  diffAddText: "#16a34a",
  diffDelBg: "rgba(220, 38, 38, 0.1)",
  diffDelText: "#dc2626",

  // Overlays
  overlayBackdrop: "rgba(0, 0, 0, 0.3)",
  shadowDark: "rgba(0, 0, 0, 0.12)",
  shadowInsetWhite: "rgba(255, 255, 255, 0.5)",
  scrollbarThumb: "rgba(0, 0, 0, 0.15)",

  // Interactive
  inputInsetShadow: "rgba(0, 0, 0, 0.03)",

  // Context meter
  contextTrackBg: "rgba(0, 0, 0, 0.06)",
  contextTrackInset: "rgba(0, 0, 0, 0.03)",
  contextTrackOverlay: "rgba(0, 0, 0, 0.03)",

  // Misc overlays
  hoverOverlay: "rgba(0, 0, 0, 0.03)",
  activeOverlay: "rgba(0, 0, 0, 0.05)",
  subtleDivider: "rgba(0, 0, 0, 0.04)",

  // Auth status
  authOkBorder: "rgba(22, 163, 74, 0.3)",
  authOkBg: "rgba(22, 163, 74, 0.06)",
  authWarnBorder: "rgba(202, 138, 4, 0.3)",
  authWarnBg: "rgba(202, 138, 4, 0.06)",

  // Model error
  modelErrorBorder: "rgba(220, 38, 38, 0.3)",
  modelErrorBg: "rgba(220, 38, 38, 0.06)",
  modelErrorText: "#dc2626",
  modelErrorRetryBorder: "rgba(220, 38, 38, 0.4)",

  // Resize handle
  resizeHandleActive: "rgba(124, 58, 237, 0.3)",

  // Template chip delete
  chipDeleteHoverBg: "rgba(220, 38, 38, 0.1)",
  chipDeleteHoverText: "#dc2626",

  // Unsaved badge
  unsavedBadgeBg: "rgba(202, 138, 4, 0.1)",
};
