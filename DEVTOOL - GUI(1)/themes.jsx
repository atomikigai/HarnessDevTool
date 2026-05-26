// Four palette explorations for Harness GUI.
// Each theme defines window chrome + per-data-type accent colors.

const T_BASE_DARK = {
  windowBg: '#0e1116',
  windowBorder: '#1f2630',
  windowShadow: '0 24px 60px rgba(0,0,0,0.35)',
  titleBarBg: '#0e1116',
  titleBarFg: '#c5cdd9',
  titleBarIcon: '#6b7684',
  railBg: '#0a0d12',
  railActiveBg: 'rgba(94, 234, 212, 0.10)',
  panelBg: '#0e1116',
  canvasBg: '#0b0e13',
  toolbarBg: '#0e1116',
  tableHeaderBg: '#0e1116',
  rowStripe: 'rgba(255,255,255,0.012)',
  rowDivider: 'rgba(255,255,255,0.04)',
  statusBarBg: '#0a0d12',
};

// ============================================================
// 1. SIGNAL — Subtle semantic accents on the existing brand teal.
//    Most conservative; closest to current. Easy adoption.
// ============================================================
const themeSignal = {
  ...T_BASE_DARK,
  name: 'Signal',
  text: '#c5cdd9',
  muted: '#5d6776',
  label: '#5d6776',
  accent: '#5eead4', // brand teal preserved
  breadcrumb: '#8a94a3',
  tableHeaderFg: '#5eead4',

  itemActiveBg: 'rgba(94, 234, 212, 0.08)',
  badgePostgresBg: 'transparent',
  badgePostgresFg: '#5eead4',
  badgePostgresBorder: 'rgba(94, 234, 212, 0.3)',

  btnBg: 'transparent',
  btnFg: '#c5cdd9',
  btnBorder: 'rgba(255,255,255,0.08)',
  primaryBtnBg: 'transparent',
  primaryBtnFg: '#5eead4',
  primaryBtnBorder: 'rgba(94, 234, 212, 0.35)',

  cellFocusBorder: '#5eead4',
  cellFocusBg: 'transparent',
  successDot: '#5eead4',

  boolStyle: 'pill',
  showTypeChip: false,

  types: {
    int:       '#8b9bb0',  // dim — IDs don't need to shout
    uuid:      '#9d8cd9',  // soft violet — visually clusters
    string:    '#e1e6ec',  // bright white — names stand out
    hash:      '#4a5360',  // very muted — visually de-emphasized
    email:     '#7cc3f5',  // sky blue
    emailDim:  '#4a6478',
    timestamp: '#d4a574',  // warm tan — calm
    timestampDate: '#8a7560',
    timestampTime: '#d4a574',
    boolTrue:  { fg: '#5eead4', bg: 'rgba(94, 234, 212, 0.12)', dot: '#5eead4' },
    boolFalse: { fg: '#6b7684', bg: 'rgba(140, 150, 165, 0.08)', dot: '#6b7684' },
  },
};

// ============================================================
// 2. SYNTAX — Vibrant, code-editor-grade semantic palette.
//    Every type gets a distinct hue. Maximum scannability.
// ============================================================
const themeSyntax = {
  ...T_BASE_DARK,
  windowBg: '#11141c',
  panelBg: '#11141c',
  canvasBg: '#0d1019',
  toolbarBg: '#11141c',
  tableHeaderBg: '#11141c',
  titleBarBg: '#11141c',
  railBg: '#0a0d14',
  statusBarBg: '#0a0d14',

  name: 'Syntax',
  text: '#d5dae3',
  muted: '#5a6478',
  label: '#5a6478',
  accent: '#7dd3fc',   // shifted to brighter sky blue for primary
  breadcrumb: '#a9b3c4',
  tableHeaderFg: '#7dd3fc',

  itemActiveBg: 'rgba(125, 211, 252, 0.10)',
  badgePostgresBg: 'rgba(56, 189, 248, 0.15)',
  badgePostgresFg: '#7dd3fc',
  badgePostgresBorder: 'transparent',
  railActiveBg: 'rgba(125, 211, 252, 0.10)',

  btnBg: 'transparent',
  btnFg: '#d5dae3',
  btnBorder: 'rgba(255,255,255,0.10)',
  primaryBtnBg: 'rgba(125, 211, 252, 0.15)',
  primaryBtnFg: '#7dd3fc',
  primaryBtnBorder: 'rgba(125, 211, 252, 0.4)',

  cellFocusBorder: '#7dd3fc',
  cellFocusBg: 'rgba(125, 211, 252, 0.06)',
  successDot: '#4ade80',

  boolStyle: 'pill',
  showTypeChip: false,
  timestampSplit: true,
  emailSplit: true,

  types: {
    int:       '#fbbf24',  // amber — numeric
    uuid:      '#c084fc',  // violet — identifier
    string:    '#f1f5f9',  // bright white
    hash:      '#475569',  // dim slate — security blob
    email:     '#60a5fa',  // blue local part
    emailDim:  '#3b82f6aa',
    timestamp: '#fbbf24',
    timestampDate: '#7c8aa0',
    timestampTime: '#fbbf24',
    boolTrue:  { fg: '#4ade80', bg: 'rgba(74, 222, 128, 0.14)', dot: '#4ade80' },
    boolFalse: { fg: '#f87171', bg: 'rgba(248, 113, 113, 0.10)', dot: '#f87171' },
  },
};

// ============================================================
// 3. WARMTH — Warm dark, gruvbox-inspired earth tones.
//    Easier on eyes for long sessions. Lower contrast hues.
// ============================================================
const themeWarmth = {
  name: 'Warmth',
  windowBg: '#1c1815',
  windowBorder: '#332a25',
  windowShadow: '0 24px 60px rgba(0,0,0,0.4)',
  titleBarBg: '#1c1815',
  titleBarFg: '#d6c8b8',
  titleBarIcon: '#7a6d5f',
  railBg: '#181410',
  railActiveBg: 'rgba(232, 168, 124, 0.12)',
  panelBg: '#1c1815',
  canvasBg: '#181410',
  toolbarBg: '#1c1815',
  tableHeaderBg: '#1c1815',
  statusBarBg: '#181410',
  rowStripe: 'rgba(255,200,160,0.02)',
  rowDivider: 'rgba(255,200,160,0.05)',

  text: '#d6c8b8',
  muted: '#7a6d5f',
  label: '#7a6d5f',
  accent: '#e8a87c', // warm coral
  breadcrumb: '#b8a890',
  tableHeaderFg: '#e8a87c',

  itemActiveBg: 'rgba(232, 168, 124, 0.10)',
  badgePostgresBg: 'rgba(232, 168, 124, 0.12)',
  badgePostgresFg: '#e8a87c',
  badgePostgresBorder: 'transparent',

  btnBg: 'transparent',
  btnFg: '#d6c8b8',
  btnBorder: 'rgba(232, 168, 124, 0.15)',
  primaryBtnBg: 'rgba(232, 168, 124, 0.15)',
  primaryBtnFg: '#e8a87c',
  primaryBtnBorder: 'rgba(232, 168, 124, 0.35)',

  cellFocusBorder: '#e8a87c',
  cellFocusBg: 'rgba(232, 168, 124, 0.06)',
  successDot: '#a3be8c',

  boolStyle: 'check',
  showTypeChip: false,
  timestampSplit: true,
  emailSplit: true,

  types: {
    int:       '#d8a657',  // mustard
    uuid:      '#d3869b',  // dusty pink
    string:    '#ebdbb2',  // cream
    hash:      '#665043',  // dim umber
    email:     '#83a598',  // sage
    emailDim:  '#52746a',
    timestamp: '#fabd2f',
    timestampDate: '#8e7e60',
    timestampTime: '#fabd2f',
    boolTrue:  { fg: '#b8bb26', bg: 'transparent', dot: '#b8bb26' },
    boolFalse: { fg: '#665043', bg: 'transparent', dot: '#665043' },
  },
};

// ============================================================
// 4. PAPER — Light mode. Maximum readability for daytime/print.
// ============================================================
const themePaper = {
  name: 'Paper',
  windowBg: '#fafaf7',
  windowBorder: '#e2e2dc',
  windowShadow: '0 24px 60px rgba(40,40,30,0.12)',
  titleBarBg: '#f1f1ec',
  titleBarFg: '#3a3a36',
  titleBarIcon: '#8a8a82',
  railBg: '#f1f1ec',
  railActiveBg: 'rgba(20, 124, 138, 0.10)',
  panelBg: '#f5f5f1',
  canvasBg: '#ffffff',
  toolbarBg: '#fafaf7',
  tableHeaderBg: '#f5f5f1',
  statusBarBg: '#f1f1ec',
  rowStripe: 'rgba(0,0,0,0.018)',
  rowDivider: 'rgba(0,0,0,0.06)',

  text: '#2a2a26',
  muted: '#8a8a82',
  label: '#8a8a82',
  accent: '#0d8b91',     // deep teal — works on light
  breadcrumb: '#5a5a55',
  tableHeaderFg: '#0d8b91',

  itemActiveBg: 'rgba(13, 139, 145, 0.10)',
  badgePostgresBg: '#0d8b91',
  badgePostgresFg: '#ffffff',
  badgePostgresBorder: 'transparent',

  btnBg: '#ffffff',
  btnFg: '#2a2a26',
  btnBorder: '#d8d8d0',
  primaryBtnBg: '#0d8b91',
  primaryBtnFg: '#ffffff',
  primaryBtnBorder: '#0d8b91',

  cellFocusBorder: '#0d8b91',
  cellFocusBg: 'rgba(13, 139, 145, 0.06)',
  successDot: '#2d9d5b',

  boolStyle: 'pill',
  showTypeChip: false,
  timestampSplit: true,
  emailSplit: true,

  types: {
    int:       '#a05a00',  // burnt amber
    uuid:      '#7b3fbf',  // deep violet
    string:    '#1a1a18',  // near-black
    hash:      '#a0a09a',  // light gray
    email:     '#1f5fbf',  // royal blue
    emailDim:  '#7a96bf',
    timestamp: '#b35a00',
    timestampDate: '#8a7a60',
    timestampTime: '#b35a00',
    boolTrue:  { fg: '#137a3d', bg: 'rgba(45, 157, 91, 0.12)', dot: '#2d9d5b', border: 'rgba(45,157,91,0.3)' },
    boolFalse: { fg: '#999088', bg: 'rgba(0,0,0,0.04)', dot: '#b8b0a8', border: 'rgba(0,0,0,0.08)' },
  },
};

// ============================================================
// 5. LINEN — Light + warm. Paper × Warmth hybrid.
//    Cream paper bg, sepia ink, earth-tone semantics.
//    Reads like a well-designed editorial / Linear in warm mode.
// ============================================================
const themeLinen = {
  name: 'Linen',
  windowBg: '#f7f1e6',
  windowBorder: '#e2d8c4',
  windowShadow: '0 24px 60px rgba(80,60,30,0.10)',
  titleBarBg: '#efe6d4',
  titleBarFg: '#3c3528',
  titleBarIcon: '#8a7e68',
  railBg: '#efe6d4',
  railActiveBg: 'rgba(166, 99, 64, 0.12)',
  panelBg: '#f1e9d8',
  canvasBg: '#fbf6ec',
  toolbarBg: '#f7f1e6',
  tableHeaderBg: '#f1e9d8',
  statusBarBg: '#efe6d4',
  rowStripe: 'rgba(110, 75, 30, 0.025)',
  rowDivider: 'rgba(110, 75, 30, 0.08)',

  text: '#3c3528',
  muted: '#8a7e68',
  label: '#8a7e68',
  accent: '#a66340',        // terracotta — warm but legible on cream
  breadcrumb: '#5f5440',
  tableHeaderFg: '#a66340',

  itemActiveBg: 'rgba(166, 99, 64, 0.10)',
  badgePostgresBg: '#a66340',
  badgePostgresFg: '#fbf6ec',
  badgePostgresBorder: 'transparent',

  btnBg: '#fbf6ec',
  btnFg: '#3c3528',
  btnBorder: '#d8cdb4',
  primaryBtnBg: '#a66340',
  primaryBtnFg: '#fbf6ec',
  primaryBtnBorder: '#a66340',

  cellFocusBorder: '#a66340',
  cellFocusBg: 'rgba(166, 99, 64, 0.06)',
  successDot: '#6b8e3a',

  boolStyle: 'pill',
  showTypeChip: false,
  timestampSplit: true,
  emailSplit: true,

  types: {
    int:       '#9a5c1a',  // burnt sienna for numerics
    uuid:      '#6e4a8a',  // muted aubergine
    string:    '#2a241a',  // ink black
    hash:      '#b8ad94',  // very light tan — visually mute
    email:     '#3d6b8e',  // muted slate blue
    emailDim:  '#8aa3b8',
    timestamp: '#a86a1f',  // amber
    timestampDate: '#a89878',
    timestampTime: '#a86a1f',
    boolTrue:  { fg: '#456a1f', bg: 'rgba(107, 142, 58, 0.16)', dot: '#6b8e3a', border: 'rgba(107,142,58,0.3)' },
    boolFalse: { fg: '#a89878', bg: 'rgba(110, 75, 30, 0.06)', dot: '#bfae8e', border: 'rgba(110, 75, 30, 0.12)' },
  },
};

// ============================================================
// 6. MOCHA — Refined dark warm. Warmth taken further:
//    deeper espresso base, higher contrast, more saturated accents.
//    Feels like a premium pro-tool dark theme.
// ============================================================
const themeMocha = {
  name: 'Mocha',
  windowBg: '#1a1410',
  windowBorder: '#322620',
  windowShadow: '0 24px 60px rgba(0,0,0,0.55)',
  titleBarBg: '#1a1410',
  titleBarFg: '#e8d9c4',
  titleBarIcon: '#8a7460',
  railBg: '#130e0a',
  railActiveBg: 'rgba(245, 158, 80, 0.14)',
  panelBg: '#1a1410',
  canvasBg: '#0f0b08',
  toolbarBg: '#1a1410',
  tableHeaderBg: '#1a1410',
  statusBarBg: '#130e0a',
  rowStripe: 'rgba(255, 200, 140, 0.020)',
  rowDivider: 'rgba(255, 200, 140, 0.055)',

  text: '#e8d9c4',
  muted: '#857060',
  label: '#857060',
  accent: '#f59e50',           // saturated amber
  breadcrumb: '#c4ad92',
  tableHeaderFg: '#f59e50',

  itemActiveBg: 'rgba(245, 158, 80, 0.10)',
  badgePostgresBg: 'rgba(245, 158, 80, 0.16)',
  badgePostgresFg: '#f59e50',
  badgePostgresBorder: 'transparent',

  btnBg: 'transparent',
  btnFg: '#e8d9c4',
  btnBorder: 'rgba(245, 158, 80, 0.18)',
  primaryBtnBg: '#f59e50',
  primaryBtnFg: '#1a1410',
  primaryBtnBorder: '#f59e50',

  cellFocusBorder: '#f59e50',
  cellFocusBg: 'rgba(245, 158, 80, 0.08)',
  successDot: '#a3c47a',

  boolStyle: 'pill',
  showTypeChip: false,
  timestampSplit: true,
  emailSplit: true,

  types: {
    int:       '#f5c06b',     // gold
    uuid:      '#d49ad4',     // soft magenta — pops against warm base
    string:    '#fbe9d0',     // cream
    hash:      '#5a4a3c',     // dim coffee
    email:     '#7ec4e0',     // cool blue — color contrast against warm
    emailDim:  '#4d7a8c',
    timestamp: '#f5c06b',
    timestampDate: '#8e7a60',
    timestampTime: '#f5c06b',
    boolTrue:  { fg: '#bdd66a', bg: 'rgba(189, 214, 106, 0.15)', dot: '#bdd66a' },
    boolFalse: { fg: '#e08566', bg: 'rgba(224, 133, 102, 0.10)', dot: '#e08566' },
  },
};

window.HARNESS_THEMES = {
  signal: themeSignal,
  syntax: themeSyntax,
  warmth: themeWarmth,
  paper: themePaper,
  linen: themeLinen,
  mocha: themeMocha,
};
