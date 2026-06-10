import type { Theme } from "./types";

export const purple: Theme = {
  name: "purple",
  label: "Purple",

  // Surfaces
  bgApp: "#120d1f",
  bgAppGlow: "#211235",
  bgSidebar: "#0c0816",
  bgPanel: "#171126",
  bgPanel2: "#201734",
  bgPanel3: "#2a1e45",
  bgCode: "#110b1d",

  // Borders
  borderSubtle: "#35274f",
  borderStrong: "#4a3670",

  // Text
  textPrimary: "#f2f4f8",
  textSecondary: "#dde1e6",
  textMuted: "#9f8cb8", // visual-language AA nudge (was #8f7aa8): clears 4.5:1 on raised/overlay @10–11px
  textOnAccent: "#140d22",

  // Accent
  accent: "#be95ff",
  accentHover: "#d5bbff",
  accentLight: "#e2d4ff",
  accentBgSubtle: "rgba(190, 149, 255, 0.06)",
  accentBgHover: "rgba(190, 149, 255, 0.12)",
  accentBorderSubtle: "rgba(190, 149, 255, 0.2)",
  accentBorderHover: "rgba(190, 149, 255, 0.4)",

  // Blue
  accentBlue: "#33b1ff",
  accentBlueHover: "#78a9ff",
  accentBlueBgSubtle: "rgba(51, 177, 255, 0.06)",
  accentBlueBorderSubtle: "rgba(51, 177, 255, 0.15)",
  accentBlueBorder: "rgba(51, 177, 255, 0.4)",

  // Cyan
  accentCyan: "#3ddbd9",

  // Success
  success: "#42be65",
  successGlow: "rgba(66, 190, 101, 0.5)",
  successBgSubtle: "rgba(66, 190, 101, 0.08)",

  // Warning
  warning: "#ffe97b",
  warningGlow: "rgba(255, 233, 123, 0.32)",
  warningBgSubtle: "rgba(255, 233, 123, 0.06)",
  warningBgFaint: "rgba(255, 233, 123, 0.03)",
  warningBorderSubtle: "rgba(255, 233, 123, 0.2)",
  warningBorderFaint: "rgba(255, 233, 123, 0.1)",

  // Error
  error: "#ee5396",
  errorLight: "#ffb4b4",
  errorGlow: "rgba(238, 83, 150, 0.32)",
  errorBgSubtle: "rgba(238, 83, 150, 0.06)",
  errorBgFaint: "rgba(238, 83, 150, 0.12)",
  errorBorderSubtle: "rgba(238, 83, 150, 0.15)",
  errorBorderFaint: "rgba(238, 83, 150, 0.35)",

  // Diff
  diffAddBg: "rgba(66, 190, 101, 0.12)",
  diffAddText: "#42be65",
  diffDelBg: "rgba(238, 83, 150, 0.12)",
  diffDelText: "#ee5396",

  // Overlays
  overlayBackdrop: "rgba(0, 0, 0, 0.6)",
  shadowDark: "rgba(0, 0, 0, 0.5)",
  shadowInsetWhite: "rgba(255, 255, 255, 0.04)",
  scrollbarThumb: "rgba(53, 39, 79, 0.3)",

  // Interactive
  inputInsetShadow: "rgba(255, 255, 255, 0.03)",

  // Context meter
  contextTrackBg: "rgba(9, 6, 16, 0.72)",
  contextTrackInset: "rgba(255, 255, 255, 0.04)",
  contextTrackOverlay: "rgba(255, 255, 255, 0.05)",

  // Misc overlays
  hoverOverlay: "rgba(255, 255, 255, 0.03)",
  activeOverlay: "rgba(255, 255, 255, 0.05)",
  subtleDivider: "rgba(255, 255, 255, 0.03)",

  // Auth status
  authOkBorder: "rgba(61, 214, 140, 0.4)",
  authOkBg: "rgba(18, 53, 39, 0.45)",
  authWarnBorder: "rgba(255, 176, 32, 0.35)",
  authWarnBg: "rgba(64, 42, 12, 0.4)",

  // Model error
  modelErrorBorder: "rgba(255, 120, 120, 0.4)",
  modelErrorBg: "rgba(64, 20, 20, 0.45)",
  modelErrorText: "#ffb4b4",
  modelErrorRetryBorder: "rgba(255, 180, 180, 0.5)",

  // Resize handle
  resizeHandleActive: "rgba(190, 149, 255, 0.35)",

  // Template chip delete
  chipDeleteHoverBg: "rgba(255, 120, 120, 0.15)",
  chipDeleteHoverText: "#ffb4b4",

  // Unsaved badge
  unsavedBadgeBg: "rgba(255, 176, 32, 0.15)",

  // Grade ramp — gray → green → blue → violet → amber → magenta
  gradeE: "#8a76a2",
  gradeD: "#42be65",
  gradeC: "#33b1ff",
  gradeB: "#be95ff",
  gradeA: "#ffc24d",
  gradeS: "#ee5396",
};
