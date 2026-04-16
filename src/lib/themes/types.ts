/**
 * Every semantic color token the app uses. Each theme must define all of these.
 * Components consume these as CSS custom properties: var(--token-name).
 */
export interface Theme {
  name: string;
  label: string;

  // ── Surfaces ──────────────────────────────────────────────
  bgApp: string;
  bgAppGlow: string;
  bgSidebar: string;
  bgPanel: string;
  bgPanel2: string;
  bgPanel3: string;
  bgCode: string;

  // ── Borders ───────────────────────────────────────────────
  borderSubtle: string;
  borderStrong: string;

  // ── Text ──────────────────────────────────────────────────
  textPrimary: string;
  textSecondary: string;
  textMuted: string;
  textOnAccent: string;

  // ── Accent ────────────────────────────────────────────────
  accent: string;
  accentHover: string;
  accentLight: string;
  accentBgSubtle: string;
  accentBgHover: string;
  accentBorderSubtle: string;
  accentBorderHover: string;

  // ── Secondary accent (blue) ───────────────────────────────
  accentBlue: string;
  accentBlueHover: string;
  accentBlueBgSubtle: string;
  accentBlueBorderSubtle: string;
  accentBlueBorder: string;

  // ── Accent cyan ───────────────────────────────────────────
  accentCyan: string;

  // ── Status: success ───────────────────────────────────────
  success: string;
  successGlow: string;
  successBgSubtle: string;

  // ── Status: warning ───────────────────────────────────────
  warning: string;
  warningGlow: string;
  warningBgSubtle: string;
  warningBgFaint: string;
  warningBorderSubtle: string;
  warningBorderFaint: string;

  // ── Status: error ─────────────────────────────────────────
  error: string;
  errorLight: string;
  errorGlow: string;
  errorBgSubtle: string;
  errorBgFaint: string;
  errorBorderSubtle: string;
  errorBorderFaint: string;

  // ── Diff ──────────────────────────────────────────────────
  diffAddBg: string;
  diffAddText: string;
  diffDelBg: string;
  diffDelText: string;

  // ── Overlays ──────────────────────────────────────────────
  overlayBackdrop: string;
  shadowDark: string;
  shadowInsetWhite: string;
  scrollbarThumb: string;

  // ── Interactive ───────────────────────────────────────────
  inputInsetShadow: string;

  // ── Context meter ─────────────────────────────────────────
  contextTrackBg: string;
  contextTrackInset: string;
  contextTrackOverlay: string;

  // ── Misc overlays ─────────────────────────────────────────
  hoverOverlay: string;
  activeOverlay: string;
  subtleDivider: string;

  // ── Auth status ───────────────────────────────────────────
  authOkBorder: string;
  authOkBg: string;
  authWarnBorder: string;
  authWarnBg: string;

  // ── Model error ───────────────────────────────────────────
  modelErrorBorder: string;
  modelErrorBg: string;
  modelErrorText: string;
  modelErrorRetryBorder: string;

  // ── Resize handle ─────────────────────────────────────────
  resizeHandleActive: string;

  // ── Template chip delete hover ────────────────────────────
  chipDeleteHoverBg: string;
  chipDeleteHoverText: string;

  // ── Unsaved badge ─────────────────────────────────────────
  unsavedBadgeBg: string;
}

/** Registry key — the value stored in ui_state */
export type ThemeId = string;
