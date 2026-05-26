// themes-v2.jsx — 4 themes: 2 Warmth variations + 2 Paper variations
// All built to work with HarnessGUIv2.

const MONO = "'JetBrains Mono', 'Fira Code', ui-monospace, Menlo, monospace";
const UI   = "'Inter', ui-sans-serif, system-ui, sans-serif";

// ============================================================
// WARMTH A — Original gruvbox earth tones
// ============================================================
const warmthA = {
  name: 'Warmth',
  fontMono: MONO,
  fontUI: UI,

  windowBg: '#1c1915',
  windowBorder: '#2e2620',
  windowShadow: '0 32px 80px rgba(0,0,0,0.45)',

  titleBarBg: '#181410',
  titleBarFg: '#d6c8b8',
  titleBarPattern: 'none',
  closeBtn: '#ed6a5e', minBtn: '#f4bf4f', maxBtn: '#61c554',

  railBg: '#141210',
  railActiveBg: 'rgba(232,168,124,0.12)',

  panelBg: '#1c1915',
  canvasBg: '#17140f',
  toolbarBg: '#1c1915',
  tableHeaderBg: '#1c1915',
  statusBarBg: '#141210',

  rowStripe: 'rgba(255,220,180,0.025)',
  rowDivider: 'rgba(255,200,160,0.06)',
  rowSelectedBg: 'rgba(232,168,124,0.10)',

  text: '#d6c8b8',
  muted: '#7a6d5f',
  label: '#6a5e52',
  accent: '#e8a87c',
  breadcrumb: '#b8a890',
  iconMuted: '#6a5e52',
  tableIcon: '#b8a060',
  viewIcon: '#8a9870',
  activeText: '#e8a87c',

  itemActiveBg: 'rgba(232,168,124,0.10)',
  connBg: 'rgba(232,168,124,0.06)',
  connBorder: 'rgba(232,168,124,0.14)',
  countBg: 'rgba(255,200,160,0.08)',

  inputBg: 'rgba(255,200,160,0.05)',
  inputBorder: '#2e2620',

  searchBg: 'rgba(255,200,160,0.05)',
  searchBorder: '#2e2620',
  searchFg: '#d6c8b8',
  kbdBg: 'rgba(255,200,160,0.05)',
  kbdBorder: '#3e342c',

  tabsBg: 'rgba(0,0,0,0.15)',
  tabActiveBg: '#2e2620',

  badgePostgresBg: 'rgba(232,168,124,0.14)',
  badgePostgresFg: '#e8a87c',
  badgePostgresBorder: 'rgba(232,168,124,0.25)',

  btnFg: '#b8a890',
  primaryBtnBg: 'rgba(232,168,124,0.15)',
  primaryBtnFg: '#e8a87c',
  primaryBtnBorder: 'rgba(232,168,124,0.35)',
  primaryBtnShadow: 'none',
  danger: '#e07070',

  cellFocusBorder: '#e8a87c',
  cellFocusRing: 'rgba(232,168,124,0.15)',
  cellEditBg: 'rgba(232,168,124,0.06)',
  checkBorder: '#4a3e32',
  checkColor: '#141210',
  typeChipBg: 'rgba(255,200,160,0.07)',
  dirtyDot: '#e8c460',
  successDot: '#a3be8c',

  types: {
    int:       '#d8a657',
    pk:        '#f0c060',
    uuid:      '#d3869b',
    uuidBg:    'rgba(211,134,155,0.10)',
    string:    '#ebdbb2',
    hash:      '#5a4a3a',
    email:     '#83a598',
    emailDim:  '#52746a',
    timestamp: '#fabd2f',
    timestampDate: '#7a6a50',
    timestampTime: '#d8a040',
    boolTrue:  { fg: '#b8bb26', bg: 'rgba(184,187,38,0.12)', dot: '#b8bb26', border: 'rgba(184,187,38,0.25)' },
    boolFalse: { fg: '#7a6d5f', bg: 'rgba(255,200,160,0.04)', dot: '#5a4e42', border: 'rgba(255,200,160,0.08)' },
  },
};

// ============================================================
// WARMTH B — Dusk: deeper contrast, violet-tinged, moodier
// ============================================================
const warmthB = {
  ...warmthA,
  name: 'Warmth Dusk',

  windowBg: '#16131c',
  windowBorder: '#28223a',
  titleBarBg: '#111018',
  railBg: '#0e0d14',
  panelBg: '#16131c',
  canvasBg: '#121019',
  toolbarBg: '#16131c',
  tableHeaderBg: '#16131c',
  statusBarBg: '#0e0d14',

  rowStripe: 'rgba(200,180,255,0.025)',
  rowDivider: 'rgba(200,180,255,0.06)',
  rowSelectedBg: 'rgba(208,156,232,0.10)',

  text: '#d8d0e8',
  muted: '#7a6e8a',
  label: '#645a78',
  accent: '#d09ce8',            // shifted to lavender
  breadcrumb: '#b0a8c8',
  iconMuted: '#645a78',
  tableIcon: '#a898c0',
  viewIcon: '#8a98b0',
  activeText: '#d09ce8',

  itemActiveBg: 'rgba(208,156,232,0.10)',
  connBg: 'rgba(208,156,232,0.06)',
  connBorder: 'rgba(208,156,232,0.14)',
  countBg: 'rgba(200,180,255,0.08)',

  inputBg: 'rgba(200,180,255,0.05)',
  inputBorder: '#28223a',
  searchBg: 'rgba(200,180,255,0.05)',
  searchBorder: '#28223a',
  kbdBg: 'rgba(200,180,255,0.05)',
  kbdBorder: '#3a2e50',

  tabsBg: 'rgba(0,0,0,0.20)',
  tabActiveBg: '#28223a',

  railActiveBg: 'rgba(208,156,232,0.12)',
  badgePostgresBg: 'rgba(208,156,232,0.14)',
  badgePostgresFg: '#d09ce8',
  badgePostgresBorder: 'rgba(208,156,232,0.25)',

  primaryBtnBg: 'rgba(208,156,232,0.15)',
  primaryBtnFg: '#d09ce8',
  primaryBtnBorder: 'rgba(208,156,232,0.35)',
  checkBorder: '#3a3050',
  checkColor: '#111018',
  dirtyDot: '#e8c460',
  successDot: '#88c87a',

  types: {
    int:       '#e8a87c',        // warm amber — PK stays warm
    pk:        '#f0c060',
    uuid:      '#9d8cd9',        // violet UUID
    uuidBg:    'rgba(157,140,217,0.10)',
    string:    '#ddd8ee',
    hash:      '#4a3e5a',
    email:     '#7cc3f5',
    emailDim:  '#4a6478',
    timestamp: '#c8a8d0',
    timestampDate: '#6a5e78',
    timestampTime: '#c0a0d8',
    boolTrue:  { fg: '#88c87a', bg: 'rgba(136,200,122,0.12)', dot: '#88c87a', border: 'rgba(136,200,122,0.25)' },
    boolFalse: { fg: '#7a6e8a', bg: 'rgba(200,180,255,0.04)', dot: '#5a5068', border: 'rgba(200,180,255,0.08)' },
  },
};

// ============================================================
// PAPER A — Warm cream light mode
// ============================================================
const paperA = {
  name: 'Paper',
  fontMono: MONO,
  fontUI: UI,

  windowBg: '#faf8f2',
  windowBorder: '#e2ddd4',
  windowShadow: '0 32px 80px rgba(40,35,25,0.14)',

  titleBarBg: '#f0ece2',
  titleBarFg: '#3a3428',
  titleBarPattern: 'none',
  closeBtn: '#ed6a5e', minBtn: '#f4bf4f', maxBtn: '#61c554',

  railBg: '#ede9e0',
  railActiveBg: 'rgba(14,120,100,0.10)',

  panelBg: '#f5f2ea',
  canvasBg: '#ffffff',
  toolbarBg: '#faf8f2',
  tableHeaderBg: '#f5f2ea',
  statusBarBg: '#ede9e0',

  rowStripe: 'rgba(0,0,0,0.018)',
  rowDivider: 'rgba(0,0,0,0.07)',
  rowSelectedBg: 'rgba(14,120,100,0.07)',

  text: '#2e2a22',
  muted: '#8a8278',
  label: '#9a9288',
  accent: '#0e7864',
  breadcrumb: '#5a5448',
  iconMuted: '#9a9288',
  tableIcon: '#a07838',
  viewIcon: '#5878a0',
  activeText: '#0e7864',

  itemActiveBg: 'rgba(14,120,100,0.09)',
  connBg: 'rgba(14,120,100,0.06)',
  connBorder: 'rgba(14,120,100,0.15)',
  countBg: 'rgba(0,0,0,0.05)',

  inputBg: '#f0ece2',
  inputBorder: '#d8d4ca',
  searchBg: '#f0ece2',
  searchBorder: '#d8d4ca',
  searchFg: '#2e2a22',
  kbdBg: '#e8e4da',
  kbdBorder: '#ccc8be',

  tabsBg: 'rgba(0,0,0,0.05)',
  tabActiveBg: '#faf8f2',

  badgePostgresBg: '#0e7864',
  badgePostgresFg: '#ffffff',
  badgePostgresBorder: 'none',

  btnFg: '#4a4438',
  primaryBtnBg: '#0e7864',
  primaryBtnFg: '#ffffff',
  primaryBtnBorder: '#0e7864',
  primaryBtnShadow: '0 2px 6px rgba(14,120,100,0.25)',
  danger: '#cc4444',

  cellFocusBorder: '#0e7864',
  cellFocusRing: 'rgba(14,120,100,0.12)',
  cellEditBg: 'rgba(14,120,100,0.05)',
  checkBorder: '#c8c4ba',
  checkColor: '#ffffff',
  typeChipBg: 'rgba(0,0,0,0.06)',
  dirtyDot: '#c08030',
  successDot: '#2d9d5b',

  types: {
    int:       '#a05820',
    pk:        '#c07020',
    uuid:      '#7b3fbf',
    uuidBg:    'rgba(123,63,191,0.08)',
    string:    '#1a1814',
    hash:      '#b0aa9e',
    email:     '#1f5fbf',
    emailDim:  '#8a9fbf',
    timestamp: '#a05820',
    timestampDate: '#8a8278',
    timestampTime: '#a05820',
    boolTrue:  { fg: '#137a3d', bg: 'rgba(45,157,91,0.12)', dot: '#2d9d5b', border: 'rgba(45,157,91,0.28)' },
    boolFalse: { fg: '#9a9288', bg: 'rgba(0,0,0,0.04)', dot: '#c8c4ba', border: 'rgba(0,0,0,0.10)' },
  },
};

// ============================================================
// PAPER B — Slate: cool, high-contrast, blueish light mode
// ============================================================
const paperB = {
  ...paperA,
  name: 'Paper Slate',

  windowBg: '#f5f6f8',
  windowBorder: '#d8dce4',
  titleBarBg: '#eaedf2',
  railBg: '#e4e8f0',
  panelBg: '#eef0f5',
  canvasBg: '#ffffff',
  toolbarBg: '#f5f6f8',
  tableHeaderBg: '#eef0f5',
  statusBarBg: '#e4e8f0',

  rowStripe: 'rgba(60,80,140,0.025)',
  rowDivider: 'rgba(60,80,140,0.07)',
  rowSelectedBg: 'rgba(42,100,200,0.07)',

  text: '#1e2330',
  muted: '#7a8298',
  label: '#8a92a8',
  accent: '#2a5ec8',            // bold blue
  breadcrumb: '#4a5270',
  iconMuted: '#8a92a8',
  tableIcon: '#5878c0',
  viewIcon: '#7858c0',
  activeText: '#2a5ec8',

  itemActiveBg: 'rgba(42,100,200,0.09)',
  connBg: 'rgba(42,100,200,0.06)',
  connBorder: 'rgba(42,100,200,0.14)',
  countBg: 'rgba(42,100,200,0.07)',

  inputBg: '#eaedf2',
  inputBorder: '#d0d5e0',
  searchBg: '#eaedf2',
  searchBorder: '#d0d5e0',
  kbdBg: '#dce0ea',
  kbdBorder: '#c8cedd',

  tabsBg: 'rgba(60,80,140,0.07)',
  tabActiveBg: '#f5f6f8',

  railActiveBg: 'rgba(42,100,200,0.10)',
  badgePostgresBg: '#2a5ec8',
  badgePostgresFg: '#ffffff',
  badgePostgresBorder: 'none',

  primaryBtnBg: '#2a5ec8',
  primaryBtnFg: '#ffffff',
  primaryBtnBorder: '#2a5ec8',
  primaryBtnShadow: '0 2px 8px rgba(42,100,200,0.28)',
  checkColor: '#ffffff',
  checkBorder: '#c0c8da',
  dirtyDot: '#d07020',
  successDot: '#28a85e',

  types: {
    int:       '#b05a00',
    pk:        '#c07800',
    uuid:      '#8840d0',
    uuidBg:    'rgba(136,64,208,0.08)',
    string:    '#1e2330',
    hash:      '#a8b0c0',
    email:     '#1848b8',
    emailDim:  '#7890b8',
    timestamp: '#2060a0',
    timestampDate: '#7a8298',
    timestampTime: '#2060a0',
    boolTrue:  { fg: '#1a7a40', bg: 'rgba(28,157,80,0.11)', dot: '#28a85e', border: 'rgba(28,157,80,0.28)' },
    boolFalse: { fg: '#8a92a8', bg: 'rgba(60,80,140,0.05)', dot: '#c0c8da', border: 'rgba(60,80,140,0.12)' },
  },
};

window.HARNESS_V2_THEMES = { warmthA, warmthB, paperA, paperB };
