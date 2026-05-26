// Harness GUI window — parametrized by `theme` prop.
// Reproduces the structure of the screenshot, with semantic per-type cell colors.

const HARNESS_DATA = {
  connections: [
    {
      name: 'local = sara',
      badge: 'POSTGRES',
      expanded: true,
      schemas: [
        {
          name: 'public',
          expanded: true,
          tables: [
            '_sqlx_migrations', 'member_type', 'member_type_translat…',
            'members', 'members_invitations', 'members_invitations_…',
            'members_invitations_…', 'oauth_accounts', 'oauth_auth_code',
            'oauth_client', 'oauth_endpoints', 'oauth_providers',
            'oauth_refresh_token', 'passkeys', 'password_reset_codes',
            'refresh_tokens', 'settings', { name: 'users', active: true },
            'users_emails', 'users_otp', 'users_preferences',
            'users_profiles', 'users_roles'
          ],
          views: [
            'view_oauth_authoriza…', 'view_oauth_token_list',
            'view_oauth_userinfo_…', 'view_user_oauth_vali…',
            'view_users_profile', 'view_users_profile_f_…',
            'view_users_summary', 'view_users_teams'
          ]
        },
        { name: 'audit' },
        { name: 'catalogs' },
        { name: 'ciat_data' },
        { name: 'consultores' }
      ]
    }
  ],
  columns: [
    { name: 'id', type: 'int' },
    { name: 'uuid', type: 'uuid' },
    { name: 'firstname', type: 'string' },
    { name: 'lastname', type: 'string' },
    { name: 'password', type: 'hash' },
    { name: 'email', type: 'email' },
    { name: 'created_at', type: 'timestamp' },
    { name: 'updated_at', type: 'timestamp' },
    { name: 'is_active', type: 'bool' },
    { name: 'is_consultant', type: 'bool' }
  ],
  rows: [
    [1, '015cb15a-86d8-77b0-b', 'Jostick', 'Quiel', '$argon2id$v=19$m=194', 'jostick516@gmail.com', '2025-06-26 18:24:32.', '2025-06-26 18:24:32.', true, false],
    [2, '015cb15a-86d8-75cc-b', 'Jostick', 'Quiel', '$argon2id$v=19$m=194', 'treyes@ciat.org',       '2025-06-26 18:25:13.', '2025-06-26 18:25:13.', true, false],
    [3, '015cb15a-86d8-718e-9', 'Jostick', 'Quiel', '$argon2id$v=19$m=194', 'ttorres@ciat.org',      '2025-06-26 18:25:29.', '2025-06-26 18:25:29.', true, false],
    [4, '015cb15a-86d8-786f-9', 'Jose',    'Baules','$argon2id$v=19$m=194', 'jbaules@ciat.org',      '2025-06-26 18:25:49.', '2026-05-26 08:23:10.', true, false]
  ]
};

// Icons (inline SVG, currentColor)
const Icon = {
  Sparkles: () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M8 1.5l1.5 4 4 1.5-4 1.5L8 12.5 6.5 8.5l-4-1.5 4-1.5L8 1.5z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/></svg>,
  SQL: () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect x="2" y="3" width="12" height="10" rx="1.5" stroke="currentColor" strokeWidth="1.2"/><path d="M2 6.5h12M2 9.5h12" stroke="currentColor" strokeWidth="1.2"/></svg>,
  SSH: () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M3 4l3 3-3 3M7 11h6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  Memory: () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M3 3h7a3 3 0 010 6H3M3 3v10M3 9h7a3 3 0 010 6H3" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/></svg>,
  Settings: () => <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="2" stroke="currentColor" strokeWidth="1.2"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5L13 13M3 13l1.5-1.5M11.5 4.5L13 3" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/></svg>,
  Folder: () => <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><path d="M1.5 3.5h4l1 1.5h8v8a.5.5 0 01-.5.5H2a.5.5 0 01-.5-.5v-9z" stroke="currentColor" strokeWidth="1.2"/></svg>,
  Table: () => <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><rect x="2" y="3" width="12" height="10" rx="1" stroke="currentColor" strokeWidth="1.2"/><path d="M2 6.5h12M6 3v10" stroke="currentColor" strokeWidth="1.2"/></svg>,
  View: () => <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><ellipse cx="8" cy="8" rx="6" ry="3.5" stroke="currentColor" strokeWidth="1.2"/><circle cx="8" cy="8" r="1.5" stroke="currentColor" strokeWidth="1.2"/></svg>,
  Chevron: ({ open }) => <svg width="10" height="10" viewBox="0 0 16 16" fill="none" style={{ transform: open ? 'rotate(90deg)' : 'rotate(0deg)', transition: 'transform .15s' }}><path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  Minimize: () => <svg width="14" height="14" viewBox="0 0 16 16" fill="none"><path d="M4 11l4-4 4 4" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/></svg>,
  Maximize: () => <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><rect x="3" y="3" width="10" height="10" stroke="currentColor" strokeWidth="1.2" transform="rotate(45 8 8)"/></svg>,
  Close: () => <svg width="14" height="14" viewBox="0 0 16 16" fill="none"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>,
  Plus: () => <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><path d="M8 3v10M3 8h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>,
  ArrowLeft: () => <svg width="10" height="10" viewBox="0 0 16 16" fill="none"><path d="M10 4l-4 4 4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  ArrowRight: () => <svg width="10" height="10" viewBox="0 0 16 16" fill="none"><path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  Dot: ({ color }) => <span style={{ display: 'inline-block', width: 6, height: 6, borderRadius: '50%', background: color, marginRight: 6, verticalAlign: 'middle' }} />,
};

// Renders a single cell value with semantic coloring based on type + theme.
function Cell({ value, type, theme, highlight }) {
  const t = theme.types;

  // Boolean rendering — every theme overrides this for clarity.
  if (type === 'bool') {
    const cfg = value ? t.boolTrue : t.boolFalse;
    if (theme.boolStyle === 'pill') {
      return (
        <span style={{
          display: 'inline-flex', alignItems: 'center', gap: 6,
          padding: '2px 10px', borderRadius: 999,
          background: cfg.bg, color: cfg.fg,
          fontSize: 12, fontWeight: 600,
          border: cfg.border ? `1px solid ${cfg.border}` : 'none',
        }}>
          <span style={{ width: 6, height: 6, borderRadius: '50%', background: cfg.dot }} />
          {value ? 'true' : 'false'}
        </span>
      );
    }
    if (theme.boolStyle === 'check') {
      return (
        <span style={{ color: cfg.fg, fontSize: 16, fontWeight: 700 }}>
          {value ? '✓' : '✗'}
        </span>
      );
    }
    // default tf with color
    return <span style={{ color: cfg.fg, fontWeight: 600 }}>{value ? 't' : 'f'}</span>;
  }

  // Format and color text
  const colorMap = {
    int: t.int,
    uuid: t.uuid,
    string: t.string,
    hash: t.hash,
    email: t.email,
    timestamp: t.timestamp,
  };
  const color = colorMap[type] || t.string;

  // Optional structural rendering — split timestamps into date / time
  if (type === 'timestamp' && theme.timestampSplit) {
    const m = String(value).match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})\.?$/);
    if (m) {
      return (
        <span style={{ color: t.timestampDate }}>
          {m[1]}{' '}
          <span style={{ color: t.timestampTime }}>{m[2]}</span>
        </span>
      );
    }
  }

  // Email — dim domain
  if (type === 'email' && theme.emailSplit) {
    const [local, domain] = String(value).split('@');
    if (domain) {
      return (
        <span style={{ color: t.email }}>
          {local}<span style={{ color: t.emailDim }}>@{domain}</span>
        </span>
      );
    }
  }

  // Highlight wrapper (for the focused/edited cell)
  if (highlight) {
    return (
      <span style={{
        color,
        border: `1px solid ${theme.cellFocusBorder}`,
        padding: '1px 6px',
        margin: '-2px -6px',
        borderRadius: 3,
        background: theme.cellFocusBg,
      }}>{value}</span>
    );
  }
  return <span style={{ color }}>{value}</span>;
}

function HarnessGUI({ theme, label }) {
  const fontMono = theme.fontMono || "'JetBrains Mono', 'Fira Code', 'Berkeley Mono', ui-monospace, Menlo, monospace";
  const fontUI = theme.fontUI || fontMono;

  const sidebarRail = [
    { icon: <Icon.Sparkles />, label: 'Agents' },
    { icon: <Icon.SQL />, label: 'SQL', active: true },
    { icon: <Icon.SSH />, label: 'SSH' },
    { icon: <Icon.Memory />, label: 'Memory' },
    { icon: <Icon.Settings />, label: 'Settings' },
  ];

  return (
    <div style={{
      width: '100%', height: '100%',
      background: theme.windowBg,
      color: theme.text,
      fontFamily: fontMono,
      fontSize: 13,
      display: 'flex', flexDirection: 'column',
      borderRadius: 8, overflow: 'hidden',
      border: `1px solid ${theme.windowBorder}`,
      boxShadow: theme.windowShadow || 'none',
    }}>
      {/* Title bar */}
      <div style={{
        height: 36,
        background: theme.titleBarBg,
        borderBottom: `1px solid ${theme.windowBorder}`,
        display: 'grid', gridTemplateColumns: '1fr auto 1fr',
        alignItems: 'center', padding: '0 12px',
        color: theme.titleBarFg,
      }}>
        <div />
        <div style={{ fontSize: 13, letterSpacing: 0.3 }}>Harness GUI</div>
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 14, color: theme.titleBarIcon }}>
          <Icon.Minimize />
          <Icon.Maximize />
          <Icon.Close />
        </div>
      </div>

      <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
        {/* Left rail */}
        <div style={{
          width: 64,
          background: theme.railBg,
          borderRight: `1px solid ${theme.windowBorder}`,
          display: 'flex', flexDirection: 'column',
          alignItems: 'center', paddingTop: 16, gap: 18,
        }}>
          {sidebarRail.map((s, i) => (
            <div key={i} style={{
              display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4,
              color: s.active ? theme.accent : theme.muted,
              padding: '6px 8px', borderRadius: 6,
              background: s.active ? theme.railActiveBg : 'transparent',
              cursor: 'pointer',
            }}>
              {s.icon}
              <span style={{ fontSize: 10, fontFamily: fontUI }}>{s.label}</span>
            </div>
          ))}
        </div>

        {/* Connections panel */}
        <div style={{
          width: 256,
          background: theme.panelBg,
          borderRight: `1px solid ${theme.windowBorder}`,
          display: 'flex', flexDirection: 'column',
          overflow: 'hidden',
        }}>
          <div style={{
            padding: '14px 16px 10px',
            display: 'flex', justifyContent: 'space-between', alignItems: 'center',
          }}>
            <span style={{
              fontSize: 11, letterSpacing: 1.2, color: theme.label,
              textTransform: 'uppercase',
            }}>Connections</span>
            <span style={{ color: theme.muted, cursor: 'pointer' }}><Icon.Plus /></span>
          </div>

          <div style={{ flex: 1, overflowY: 'auto', padding: '0 8px 16px', fontSize: 12 }}>
            {HARNESS_DATA.connections.map((conn, ci) => (
              <div key={ci}>
                <div style={{
                  display: 'flex', alignItems: 'center', gap: 6,
                  padding: '6px 8px', color: theme.text,
                }}>
                  <span style={{ color: theme.muted }}><Icon.Chevron open /></span>
                  <span style={{ color: theme.accent }}>≡</span>
                  <span>local = </span><span style={{ color: theme.accent }}>sara</span>
                  <span style={{
                    marginLeft: 'auto',
                    fontSize: 9, fontWeight: 600, letterSpacing: 0.5,
                    padding: '2px 6px',
                    background: theme.badgePostgresBg,
                    color: theme.badgePostgresFg,
                    border: theme.badgePostgresBorder ? `1px solid ${theme.badgePostgresBorder}` : 'none',
                    borderRadius: 3,
                  }}>POSTGRES</span>
                </div>

                {conn.schemas.map((schema, si) => {
                  if (!schema.tables) {
                    return (
                      <div key={si} style={{
                        display: 'flex', alignItems: 'center', gap: 6,
                        padding: '5px 8px 5px 16px', color: theme.text,
                      }}>
                        <span style={{ color: theme.muted }}><Icon.Chevron /></span>
                        <span style={{ color: theme.muted }}>≡</span>
                        <span>{schema.name}</span>
                      </div>
                    );
                  }
                  return (
                    <div key={si}>
                      <div style={{
                        display: 'flex', alignItems: 'center', gap: 6,
                        padding: '5px 8px 5px 16px', color: theme.text,
                      }}>
                        <span style={{ color: theme.muted }}><Icon.Chevron open /></span>
                        <span style={{ color: theme.muted }}>≡</span>
                        <span>{schema.name}</span>
                      </div>

                      <div style={{ fontSize: 10, letterSpacing: 1.2, color: theme.label, padding: '10px 8px 4px 30px', textTransform: 'uppercase' }}>Tables</div>
                      {schema.tables.map((tbl, ti) => {
                        const isObj = typeof tbl === 'object';
                        const name = isObj ? tbl.name : tbl;
                        const active = isObj && tbl.active;
                        return (
                          <div key={ti} style={{
                            display: 'flex', alignItems: 'center', gap: 6,
                            padding: '4px 8px 4px 30px',
                            color: active ? theme.accent : theme.text,
                            background: active ? theme.itemActiveBg : 'transparent',
                            borderLeft: active ? `2px solid ${theme.accent}` : '2px solid transparent',
                          }}>
                            <span style={{ color: active ? theme.accent : theme.muted }}><Icon.Table /></span>
                            <span>{name}</span>
                          </div>
                        );
                      })}

                      <div style={{ fontSize: 10, letterSpacing: 1.2, color: theme.label, padding: '10px 8px 4px 30px', textTransform: 'uppercase' }}>Views</div>
                      {schema.views.map((v, vi) => (
                        <div key={vi} style={{
                          display: 'flex', alignItems: 'center', gap: 6,
                          padding: '4px 8px 4px 30px', color: theme.text,
                        }}>
                          <span style={{ color: theme.muted }}><Icon.View /></span>
                          <span>{v}</span>
                        </div>
                      ))}
                    </div>
                  );
                })}
              </div>
            ))}
          </div>
        </div>

        {/* Main */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: theme.canvasBg, minWidth: 0 }}>
          {/* Top bar: breadcrumb + tabs */}
          <div style={{
            height: 48,
            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            padding: '0 20px',
            borderBottom: `1px solid ${theme.windowBorder}`,
            background: theme.toolbarBg,
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, color: theme.text }}>
              <span>local = </span><span style={{ color: theme.accent }}>sara</span>
              <span style={{ color: theme.muted }}>›</span>
              <span style={{ color: theme.breadcrumb }}>public</span>
              <span style={{ color: theme.muted }}>›</span>
              <span style={{ color: theme.accent }}>users</span>
            </div>
            <div style={{ display: 'flex', gap: 28, fontSize: 13 }}>
              <span style={{ color: theme.accent, borderBottom: `2px solid ${theme.accent}`, paddingBottom: 4 }}>Data</span>
              <span style={{ color: theme.muted }}>Query</span>
              <span style={{ color: theme.muted }}>Schema</span>
            </div>
          </div>

          {/* Pagination bar */}
          <div style={{
            height: 44,
            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            padding: '0 20px',
            borderBottom: `1px solid ${theme.windowBorder}`,
            background: theme.toolbarBg,
          }}>
            <div style={{ display: 'flex', gap: 8, alignItems: 'center', color: theme.text }}>
              <button style={btnStyle(theme)}>
                <Icon.ArrowLeft /> <span style={{ marginLeft: 4 }}>Prev</span>
              </button>
              <button style={btnStyle(theme)}>Page 1</button>
              <button style={btnStyle(theme)}>
                <span style={{ marginRight: 4 }}>Next</span> <Icon.ArrowRight />
              </button>
              <span style={{ marginLeft: 12, color: theme.muted, fontSize: 12 }}>4 rows</span>
            </div>
            <button style={{
              ...btnStyle(theme),
              background: theme.primaryBtnBg,
              color: theme.primaryBtnFg,
              border: `1px solid ${theme.primaryBtnBorder}`,
              fontWeight: 600,
            }}>
              <Icon.Plus /> <span style={{ marginLeft: 4 }}>Insert Row</span>
            </button>
          </div>

          {/* Table */}
          <div style={{ flex: 1, overflow: 'auto' }}>
            <table style={{
              width: '100%',
              borderCollapse: 'collapse',
              fontFamily: fontMono,
              fontSize: 13,
            }}>
              <thead>
                <tr>
                  {HARNESS_DATA.columns.map((col, i) => (
                    <th key={i} style={{
                      textAlign: 'left',
                      padding: '12px 16px',
                      borderBottom: `1px solid ${theme.windowBorder}`,
                      background: theme.tableHeaderBg,
                      color: theme.tableHeaderFg,
                      fontWeight: 600,
                      whiteSpace: 'nowrap',
                      position: 'sticky', top: 0,
                    }}>
                      {col.name}
                      {theme.showTypeChip && (
                        <span style={{
                          marginLeft: 6,
                          fontSize: 9,
                          color: theme.types[col.type === 'int' ? 'int' : col.type],
                          opacity: 0.7,
                          textTransform: 'uppercase',
                        }}>{col.type}</span>
                      )}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {HARNESS_DATA.rows.map((row, ri) => (
                  <tr key={ri} style={{
                    background: ri % 2 === 1 && theme.rowStripe ? theme.rowStripe : 'transparent',
                  }}>
                    {row.map((val, ci) => {
                      const col = HARNESS_DATA.columns[ci];
                      const isFocused = ri === 0 && col.name === 'email';
                      return (
                        <td key={ci} style={{
                          padding: '10px 16px',
                          borderBottom: `1px solid ${theme.rowDivider}`,
                          whiteSpace: 'nowrap',
                          verticalAlign: 'middle',
                        }}>
                          <Cell value={val} type={col.type} theme={theme} highlight={isFocused} />
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {/* Footer */}
          <div style={{
            height: 28,
            display: 'flex', justifyContent: 'space-between', alignItems: 'center',
            padding: '0 16px',
            borderTop: `1px solid ${theme.windowBorder}`,
            background: theme.statusBarBg,
            color: theme.muted, fontSize: 11,
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <Icon.Dot color={theme.accent} /> SQL
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
              <span>harness v0.3.0</span>
              <span style={{ display: 'flex', alignItems: 'center' }}>
                <Icon.Dot color={theme.successDot || theme.accent} /> local
              </span>
            </div>
          </div>
        </div>
      </div>

      {label && (
        <div style={{
          position: 'absolute', top: 8, left: 12,
          fontFamily: fontMono, fontSize: 10,
          color: theme.muted, letterSpacing: 1, textTransform: 'uppercase',
        }}>{label}</div>
      )}
    </div>
  );
}

function btnStyle(theme) {
  return {
    display: 'inline-flex', alignItems: 'center',
    padding: '6px 12px',
    background: theme.btnBg,
    color: theme.btnFg,
    border: `1px solid ${theme.btnBorder}`,
    borderRadius: 4,
    fontFamily: 'inherit', fontSize: 12,
    cursor: 'pointer',
  };
}

window.HarnessGUI = HarnessGUI;
