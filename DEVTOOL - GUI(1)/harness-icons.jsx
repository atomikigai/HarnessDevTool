// Centralized inline-SVG icons for Harness GUI v2.
// All use currentColor + 1.4 stroke for visual consistency.

const HIcon = {
  Sparkles: ({ size = 16 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M8 1.5l1.5 4 4 1.5-4 1.5L8 12.5 6.5 8.5l-4-1.5 4-1.5L8 1.5z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/>
    </svg>
  ),
  SQL: ({ size = 16 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <ellipse cx="8" cy="3.5" rx="5" ry="1.6" stroke="currentColor" strokeWidth="1.2"/>
      <path d="M3 3.5v9c0 .88 2.24 1.6 5 1.6s5-.72 5-1.6v-9" stroke="currentColor" strokeWidth="1.2"/>
      <path d="M3 8c0 .88 2.24 1.6 5 1.6s5-.72 5-1.6" stroke="currentColor" strokeWidth="1.2"/>
    </svg>
  ),
  SSH: ({ size = 16 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <rect x="1.5" y="3" width="13" height="10" rx="1.5" stroke="currentColor" strokeWidth="1.2"/>
      <path d="M4 7l2 1.5L4 10M8 10h4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  Memory: ({ size = 16 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M3 3.5h6a2.5 2.5 0 010 5H3M3 3.5v9M3 8.5h6.5a2.5 2.5 0 010 5H3" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/>
    </svg>
  ),
  Settings: ({ size = 16 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="2" stroke="currentColor" strokeWidth="1.2"/>
      <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5L13 13M3 13l1.5-1.5M11.5 4.5L13 3" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
    </svg>
  ),
  Database: ({ size = 14 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <ellipse cx="8" cy="3" rx="5" ry="1.6" stroke="currentColor" strokeWidth="1.3"/>
      <path d="M3 3v10c0 .88 2.24 1.6 5 1.6s5-.72 5-1.6V3" stroke="currentColor" strokeWidth="1.3"/>
      <path d="M3 8c0 .88 2.24 1.6 5 1.6s5-.72 5-1.6" stroke="currentColor" strokeWidth="1.3"/>
    </svg>
  ),
  Schema: ({ size = 14 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <rect x="2" y="2.5" width="12" height="3" rx="0.5" stroke="currentColor" strokeWidth="1.2"/>
      <rect x="2" y="6.5" width="12" height="3" rx="0.5" stroke="currentColor" strokeWidth="1.2"/>
      <rect x="2" y="10.5" width="12" height="3" rx="0.5" stroke="currentColor" strokeWidth="1.2"/>
    </svg>
  ),
  Table: ({ size = 14 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <rect x="2" y="2.5" width="12" height="11" rx="1" stroke="currentColor" strokeWidth="1.2"/>
      <path d="M2 6.5h12M2 9.5h12M6 2.5v11" stroke="currentColor" strokeWidth="1.2"/>
    </svg>
  ),
  View: ({ size = 14 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8s-2.5 4.5-6.5 4.5S1.5 8 1.5 8z" stroke="currentColor" strokeWidth="1.2"/>
      <circle cx="8" cy="8" r="2" stroke="currentColor" strokeWidth="1.2"/>
    </svg>
  ),
  Chevron: ({ open, size = 10 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" style={{ transform: open ? 'rotate(90deg)' : 'rotate(0deg)', transition: 'transform .12s' }}>
      <path d="M6 3.5l4 4.5-4 4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  Search: ({ size = 13 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.4"/>
      <path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
    </svg>
  ),
  Plus: ({ size = 12 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M8 3v10M3 8h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"/>
    </svg>
  ),
  Filter: ({ size = 12 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M2 3.5h12L9.5 9v4l-3-1.5V9L2 3.5z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
    </svg>
  ),
  SortAsc: ({ size = 10 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M8 3.5v9M5 6.5L8 3.5l3 3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  SortNone: ({ size = 10 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" opacity="0.4">
      <path d="M5 6L8 3l3 3M5 10l3 3 3-3" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  ArrowLeft: ({ size = 10 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M10 3.5l-4 4.5 4 4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  ArrowRight: ({ size = 10 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M6 3.5l4 4.5-4 4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  Key: ({ size = 10 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <circle cx="5" cy="11" r="2.5" stroke="currentColor" strokeWidth="1.3"/>
      <path d="M7 9l5-5M10 5l1.5 1.5M12.5 3.5L14 5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
    </svg>
  ),
  Command: ({ size = 11 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M5 1.5a2 2 0 100 4h2v-2a2 2 0 00-2-2zM11 1.5a2 2 0 110 4H9v-2a2 2 0 012-2zM5 14.5a2 2 0 110-4h2v2a2 2 0 01-2 2zM11 14.5a2 2 0 100-4H9v2a2 2 0 002 2zM5 5.5h6v5H5z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/>
    </svg>
  ),
  Dot: ({ color, size = 6 }) => (
    <span style={{ display: 'inline-block', width: size, height: size, borderRadius: '50%', background: color, marginRight: 6, verticalAlign: 'middle', flexShrink: 0 }} />
  ),
  Minimize: ({ size = 12 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="6" fill="currentColor"/></svg>
  ),
  Maximize: ({ size = 12 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="6" fill="currentColor"/></svg>
  ),
  Close: ({ size = 12 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="6" fill="currentColor"/></svg>
  ),
  Copy: ({ size = 12 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <rect x="4" y="4" width="9" height="9" rx="1" stroke="currentColor" strokeWidth="1.3"/>
      <path d="M3 11V4a1 1 0 011-1h7" stroke="currentColor" strokeWidth="1.3"/>
    </svg>
  ),
  Pencil: ({ size = 12 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M11 2.5l2.5 2.5L5 13.5l-3 .5.5-3L11 2.5z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
    </svg>
  ),
  Trash: ({ size = 12 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M3 4h10M6 4V2.5h4V4M4.5 4l.5 9.5h6L11.5 4" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
    </svg>
  ),
  GripDots: ({ size = 16 }) => (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" opacity="0.5">
      <circle cx="5" cy="4" r="0.9" fill="currentColor"/>
      <circle cx="5" cy="8" r="0.9" fill="currentColor"/>
      <circle cx="5" cy="12" r="0.9" fill="currentColor"/>
      <circle cx="11" cy="4" r="0.9" fill="currentColor"/>
      <circle cx="11" cy="8" r="0.9" fill="currentColor"/>
      <circle cx="11" cy="12" r="0.9" fill="currentColor"/>
    </svg>
  ),
};

window.HIcon = HIcon;
