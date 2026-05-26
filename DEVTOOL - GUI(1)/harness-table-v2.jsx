// Harness GUI v2 — Improved database manager with all visual polish applied.
// Renders: sidebar with search, hierarchical tree, full data table with row selection,
// detail panel, rich status bar, command-K search, sort indicators, row numbers.

const HARNESS_V2_DATA = {
  conn: {
    name: 'local = sara',
    badge: 'POSTGRES',
    host: 'localhost:5432',
  },
  schemas: [
    {
      name: 'public', expanded: true,
      tables: [
        '_sqlx_migrations', 'member_type', 'member_type_translations',
        'members', 'members_invitations', 'members_invitations_emails',
        'members_invitations_roles', 'oauth_accounts', 'oauth_auth_code',
        'oauth_client', 'oauth_endpoints', 'oauth_providers',
        'oauth_refresh_token', 'passkeys', 'password_reset_codes',
        'refresh_tokens', 'settings', { name: 'users', active: true, rows: 4 },
        'users_emails', 'users_otp', 'users_preferences',
        'users_profiles', 'users_roles'
      ],
      views: [
        'view_oauth_authorizations', 'view_oauth_token_list',
        'view_oauth_userinfo_full', 'view_user_oauth_validations',
        'view_users_profile', 'view_users_profile_full',
        'view_users_summary', 'view_users_teams'
      ]
    },
    { name: 'audit', expanded: false, tables: [], views: [] },
    { name: 'catalogs', expanded: false, tables: [], views: [] },
    { name: 'ciat_data', expanded: false, tables: [], views: [] },
    { name: 'consultores', expanded: false, tables: [], views: [] },
  ],
  columns: [
    { name: 'id', type: 'int', pk: true, sort: 'asc' },
    { name: 'uuid', type: 'uuid' },
    { name: 'firstname', type: 'string' },
    { name: 'lastname', type: 'string' },
    { name: 'password', type: 'hash' },
    { name: 'email', type: 'email' },
    { name: 'created_at', type: 'timestamp' },
    { name: 'updated_at', type: 'timestamp' },
    { name: 'is_active', type: 'bool' },
    { name: 'is_consultant', type: 'bool' },
  ],
  rows: [
    { selected: true, dirty: true, data: [1, '015cb15a-86d8-77b0-bb2e-a7c4', 'Jostick', 'Quiel',   '$argon2id$v=19$m=19456,t=2,p=1', 'jostick516@gmail.com', '2025-06-26 18:24:32', '2025-06-26 18:24:32', true,  false] },
    { selected: false, dirty: false, data: [2, '015cb15a-86d8-75cc-b3a1-d2f8', 'Jostick', 'Quiel',   '$argon2id$v=19$m=19456,t=2,p=1', 'treyes@ciat.org',       '2025-06-26 18:25:13', '2025-06-26 18:25:13', true,  false] },
    { selected: false, dirty: false, data: [3, '015cb15a-86d8-718e-9c4b-e6a9', 'Jostick', 'Quiel',   '$argon2id$v=19$m=19456,t=2,p=1', 'ttorres@ciat.org',      '2025-06-26 18:25:29', '2025-06-26 18:25:29', true,  false] },
    { selected: false, dirty: false, data: [4, '015cb15a-86d8-786f-9f1d-c3b2', 'Jose',    'Baules',  '$argon2id$v=19$m=19456,t=2,p=1', 'jbaules@ciat.org',      '2025-06-26 18:25:49', '2026-05-26 08:23:10', true,  false] },
  ],
};

// ============================================================================
// Truncate helper — middle ellipsis for UUIDs/hashes so both ends are visible.
// ============================================================================
function truncateMid(str, maxLen = 18) {
  if (!str || str.length <= maxLen) return str;
  const keep = Math.floor((maxLen - 1) / 2);
  return str.slice(0, keep) + '…' + str.slice(-keep);
}

// ============================================================================
// Semantic cell renderer — colors + formatting per data type
// ============================================================================
function CellV2({ value, type, theme, highlight, dirty, focused, columnName }) {
  const t = theme.types;

  // NULL handling
  if (value === null || value === undefined) {
    return <span style={{ color: theme.muted, fontStyle: 'italic', opacity: 0.6 }}>NULL</span>;
  }

  // Boolean — pill
  if (type === 'bool') {
    const cfg = value ? t.boolTrue : t.boolFalse;
    return (
      <span style={{
        display: 'inline-flex', alignItems: 'center', gap: 6,
        padding: '3px 10px 3px 8px', borderRadius: 999,
        background: cfg.bg, color: cfg.fg,
        fontSize: 11, fontWeight: 600,
        border: cfg.border ? `1px solid ${cfg.border}` : '1px solid transparent',
        letterSpacing: 0.2,
      }}>
        <span style={{ width: 6, height: 6, borderRadius: '50%', background: cfg.dot }} />
        {value ? 'true' : 'false'}
      </span>
    );
  }

  // Integer (PK gets a key icon)
  if (type === 'int') {
    return (
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, color: t.int, fontWeight: 600 }}>
        {columnName === 'id' && <span style={{ color: t.pk, opacity: 0.85 }}><HIcon.Key /></span>}
        {value}
      </span>
    );
  }

  // UUID — mono, mid-truncate, monospace prominent
  if (type === 'uuid') {
    return (
      <span style={{
        color: t.uuid,
        fontFamily: theme.fontMono,
        background: t.uuidBg || 'transparent',
        padding: t.uuidBg ? '2px 6px' : 0,
        borderRadius: 3,
        fontSize: 12,
      }}>
        {truncateMid(String(value), 16)}
      </span>
    );
  }

  // Hash — very dim, mid-truncate
  if (type === 'hash') {
    return (
      <span style={{
        color: t.hash, fontFamily: theme.fontMono, fontSize: 11,
        opacity: 0.85,
      }}>
        {truncateMid(String(value), 14)}
      </span>
    );
  }

  // Email — split local/domain
  if (type === 'email') {
    const [local, domain] = String(value).split('@');
    if (domain) {
      return (
        <span style={{ display: 'inline-flex', alignItems: 'baseline' }}>
          <span style={{ color: t.email, fontWeight: 500 }}>{local}</span>
          <span style={{ color: t.emailDim }}>@{domain}</span>
        </span>
      );
    }
    return <span style={{ color: t.email }}>{value}</span>;
  }

  // Timestamp — date dim, time accented
  if (type === 'timestamp') {
    const m = String(value).match(/^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})/);
    if (m) {
      return (
        <span style={{ display: 'inline-flex', alignItems: 'baseline', gap: 6, fontFamily: theme.fontMono, fontSize: 12 }}>
          <span style={{ color: t.timestampDate }}>{m[1]}</span>
          <span style={{ color: t.timestampTime, fontWeight: 500 }}>{m[2]}</span>
        </span>
      );
    }
    return <span style={{ color: t.timestamp }}>{value}</span>;
  }

  // String — bright default
  let displayValue = value;
  if (focused) {
    // Show as editable
    return (
      <span style={{
        color: t.string,
        background: theme.cellEditBg,
        border: `1px solid ${theme.cellFocusBorder}`,
        padding: '3px 8px',
        margin: '-4px -8px',
        borderRadius: 4,
        boxShadow: `0 0 0 3px ${theme.cellFocusRing}`,
        fontWeight: 500,
      }}>
        {displayValue}
        {dirty && <span style={{
          marginLeft: 6, display: 'inline-block', width: 6, height: 6,
          borderRadius: '50%', background: theme.dirtyDot,
          verticalAlign: 'middle',
        }} />}
      </span>
    );
  }
  return <span style={{ color: t.string }}>{displayValue}</span>;
}

// ============================================================================
// Sidebar tree item
// ============================================================================
function TreeItem({ icon, label, depth = 0, active, expanded, hasChildren, count, color, theme, badge }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 8,
      padding: `5px 10px 5px ${10 + depth * 14}px`,
      color: active ? theme.activeText : theme.text,
      background: active ? theme.itemActiveBg : 'transparent',
      borderLeft: active ? `2px solid ${theme.accent}` : '2px solid transparent',
      marginLeft: active ? 0 : 2,
      cursor: 'pointer',
      fontSize: 12.5,
      fontWeight: active ? 600 : 400,
      position: 'relative',
    }}>
      {hasChildren !== undefined && (
        <span style={{ color: theme.muted, display: 'inline-flex', width: 10 }}>
          {hasChildren && <HIcon.Chevron open={expanded} />}
        </span>
      )}
      <span style={{ color: color || (active ? theme.accent : theme.iconMuted), display: 'inline-flex' }}>
        {icon}
      </span>
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {label}
      </span>
      {count !== undefined && (
        <span style={{
          fontSize: 10, color: theme.muted, fontFamily: theme.fontMono,
          background: theme.countBg, padding: '1px 6px', borderRadius: 8,
        }}>{count}</span>
      )}
      {badge && (
        <span style={{
          fontSize: 9, fontWeight: 700, letterSpacing: 0.5,
          padding: '2px 6px', borderRadius: 3,
          background: theme.badgePostgresBg,
          color: theme.badgePostgresFg,
          border: theme.badgePostgresBorder ? `1px solid ${theme.badgePostgresBorder}` : 'none',
        }}>{badge}</span>
      )}
    </div>
  );
}

// ============================================================================
// MAIN COMPONENT
// ============================================================================
function HarnessGUIv2({ theme }) {
  const fontMono = theme.fontMono;
  const fontUI = theme.fontUI;
  const data = HARNESS_V2_DATA;

  const railItems = [
    { icon: <HIcon.Sparkles />, label: 'Agents' },
    { icon: <HIcon.SQL />, label: 'SQL', active: true },
    { icon: <HIcon.SSH />, label: 'SSH' },
    { icon: <HIcon.Memory />, label: 'Memory' },
    { icon: <HIcon.Settings />, label: 'Settings' },
  ];

  return (
    <div style={{
      width: '100%', height: '100%',
      background: theme.windowBg,
      color: theme.text,
      fontFamily: fontUI,
      fontSize: 13,
      display: 'flex', flexDirection: 'column',
      borderRadius: 10, overflow: 'hidden',
      border: `1px solid ${theme.windowBorder}`,
      boxShadow: theme.windowShadow,
    }}>
      {/* ==== TITLE BAR — with global search ==== */}
      <div style={{
        height: 42,
        background: theme.titleBarBg,
        borderBottom: `1px solid ${theme.windowBorder}`,
        display: 'grid', gridTemplateColumns: '160px 1fr 160px',
        alignItems: 'center', padding: '0 14px',
        color: theme.titleBarFg,
        backgroundImage: theme.titleBarPattern,
      }}>
        {/* Traffic lights */}
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <span style={{ color: theme.closeBtn || '#ed6a5e' }}><HIcon.Close /></span>
          <span style={{ color: theme.minBtn || '#f4bf4f' }}><HIcon.Minimize /></span>
          <span style={{ color: theme.maxBtn || '#61c554' }}><HIcon.Maximize /></span>
        </div>
        {/* Command palette */}
        <div style={{ display: 'flex', justifyContent: 'center' }}>
          <div style={{
            display: 'flex', alignItems: 'center', gap: 10,
            background: theme.searchBg,
            border: `1px solid ${theme.searchBorder}`,
            borderRadius: 6,
            padding: '5px 10px',
            minWidth: 380, maxWidth: 520, width: '60%',
            color: theme.searchFg,
            fontSize: 12,
          }}>
            <span style={{ color: theme.muted, display: 'inline-flex' }}><HIcon.Search /></span>
            <span style={{ color: theme.muted }}>Search tables, queries, settings…</span>
            <span style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 4, color: theme.muted, fontFamily: fontMono, fontSize: 10 }}>
              <span style={{
                padding: '1px 5px', borderRadius: 3,
                background: theme.kbdBg, border: `1px solid ${theme.kbdBorder}`,
                display: 'inline-flex', alignItems: 'center', gap: 3,
              }}>
                <HIcon.Command /> K
              </span>
            </span>
          </div>
        </div>
        {/* Connection chip */}
        <div style={{ display: 'flex', justifyContent: 'flex-end', alignItems: 'center', gap: 8 }}>
          <div style={{
            display: 'flex', alignItems: 'center', gap: 6,
            fontSize: 11, color: theme.muted,
            padding: '4px 8px', borderRadius: 4,
            border: `1px solid ${theme.windowBorder}`,
          }}>
            <HIcon.Dot color={theme.successDot} />
            <span>{data.conn.host}</span>
          </div>
        </div>
      </div>

      <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
        {/* ==== LEFT RAIL ==== */}
        <div style={{
          width: 68,
          background: theme.railBg,
          borderRight: `1px solid ${theme.windowBorder}`,
          display: 'flex', flexDirection: 'column',
          alignItems: 'center', paddingTop: 14, gap: 4,
        }}>
          {railItems.map((s, i) => (
            <div key={i} style={{
              display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 5,
              color: s.active ? theme.accent : theme.iconMuted,
              padding: '8px 10px', borderRadius: 8,
              background: s.active ? theme.railActiveBg : 'transparent',
              cursor: 'pointer', width: 52,
              position: 'relative',
            }}>
              {s.active && <span style={{ position: 'absolute', left: -14, top: '50%', transform: 'translateY(-50%)', width: 3, height: 22, borderRadius: 2, background: theme.accent }} />}
              {s.icon}
              <span style={{ fontSize: 10, fontFamily: fontUI, fontWeight: s.active ? 600 : 400 }}>{s.label}</span>
            </div>
          ))}
        </div>

        {/* ==== SIDEBAR ==== */}
        <div style={{
          width: 280,
          background: theme.panelBg,
          borderRight: `1px solid ${theme.windowBorder}`,
          display: 'flex', flexDirection: 'column',
          overflow: 'hidden',
        }}>
          {/* Connection header */}
          <div style={{
            padding: '14px 14px 10px',
          }}>
            <div style={{
              display: 'flex', alignItems: 'center', gap: 8,
              padding: '8px 10px',
              background: theme.connBg,
              border: `1px solid ${theme.connBorder}`,
              borderRadius: 6,
            }}>
              <span style={{ color: theme.accent, display: 'inline-flex' }}><HIcon.Database /></span>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 12, fontWeight: 600, color: theme.text, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                  local = <span style={{ color: theme.accent }}>sara</span>
                </div>
                <div style={{ fontSize: 10, color: theme.muted, marginTop: 1 }}>
                  PostgreSQL · 15.4
                </div>
              </div>
              <HIcon.Dot color={theme.successDot} />
            </div>
          </div>

          {/* Search filter */}
          <div style={{ padding: '0 14px 10px' }}>
            <div style={{
              display: 'flex', alignItems: 'center', gap: 8,
              padding: '6px 10px',
              background: theme.inputBg,
              border: `1px solid ${theme.inputBorder}`,
              borderRadius: 5,
              fontSize: 12,
            }}>
              <span style={{ color: theme.muted, display: 'inline-flex' }}><HIcon.Search /></span>
              <span style={{ color: theme.muted, flex: 1 }}>Filter tables…</span>
              <span style={{ color: theme.muted, opacity: 0.6, display: 'inline-flex' }}><HIcon.Filter /></span>
            </div>
          </div>

          {/* Schemas section header */}
          <div style={{
            padding: '4px 16px 4px',
            display: 'flex', justifyContent: 'space-between', alignItems: 'center',
          }}>
            <span style={{
              fontSize: 10, letterSpacing: 1.4, color: theme.label,
              textTransform: 'uppercase', fontWeight: 700,
            }}>Schemas</span>
            <span style={{ color: theme.muted, cursor: 'pointer', display: 'inline-flex' }}><HIcon.Plus /></span>
          </div>

          {/* Tree */}
          <div style={{ flex: 1, overflowY: 'auto', paddingBottom: 16 }}>
            {data.schemas.map((schema, si) => (
              <div key={si}>
                <TreeItem
                  icon={<HIcon.Schema />}
                  label={schema.name}
                  depth={0}
                  expanded={schema.expanded}
                  hasChildren={true}
                  theme={theme}
                />

                {schema.expanded && (
                  <>
                    {/* TABLES header */}
                    <div style={{
                      fontSize: 9, letterSpacing: 1.4,
                      color: theme.label, opacity: 0.75,
                      padding: '10px 14px 4px 30px',
                      textTransform: 'uppercase', fontWeight: 700,
                      display: 'flex', justifyContent: 'space-between',
                    }}>
                      <span>Tables · {schema.tables.length}</span>
                    </div>
                    {schema.tables.map((tbl, ti) => {
                      const isObj = typeof tbl === 'object';
                      const name = isObj ? tbl.name : tbl;
                      const active = isObj && tbl.active;
                      return (
                        <TreeItem
                          key={ti}
                          icon={<HIcon.Table />}
                          label={name}
                          depth={1}
                          active={active}
                          count={isObj ? tbl.rows : undefined}
                          theme={theme}
                          color={active ? theme.accent : theme.tableIcon}
                        />
                      );
                    })}

                    {/* VIEWS header */}
                    <div style={{
                      fontSize: 9, letterSpacing: 1.4,
                      color: theme.label, opacity: 0.75,
                      padding: '14px 14px 4px 30px',
                      textTransform: 'uppercase', fontWeight: 700,
                    }}>Views · {schema.views.length}</div>
                    {schema.views.map((v, vi) => (
                      <TreeItem
                        key={vi}
                        icon={<HIcon.View />}
                        label={v}
                        depth={1}
                        theme={theme}
                        color={theme.viewIcon}
                      />
                    ))}
                  </>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* ==== MAIN CONTENT ==== */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: theme.canvasBg, minWidth: 0 }}>
          {/* Breadcrumb row */}
          <div style={{
            height: 44,
            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            padding: '0 22px',
            borderBottom: `1px solid ${theme.windowBorder}`,
            background: theme.toolbarBg,
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 13 }}>
              <span style={{ color: theme.muted, display: 'inline-flex' }}><HIcon.Database /></span>
              <span style={{ color: theme.muted }}>sara</span>
              <span style={{ color: theme.muted, opacity: 0.4 }}>/</span>
              <span style={{ color: theme.breadcrumb }}>public</span>
              <span style={{ color: theme.muted, opacity: 0.4 }}>/</span>
              <span style={{ color: theme.text, fontWeight: 700 }}>users</span>
              <span style={{
                fontSize: 10, color: theme.muted, fontFamily: fontMono,
                padding: '2px 7px', borderRadius: 3,
                background: theme.countBg,
                marginLeft: 4,
              }}>4 rows</span>
            </div>
            {/* Tabs - properly grouped */}
            <div style={{
              display: 'inline-flex', gap: 2,
              padding: 3,
              background: theme.tabsBg,
              border: `1px solid ${theme.windowBorder}`,
              borderRadius: 6,
              fontSize: 12,
            }}>
              {['Data', 'Query', 'Schema', 'Relations'].map((tab, i) => (
                <span key={i} style={{
                  padding: '4px 14px',
                  borderRadius: 4,
                  background: i === 0 ? theme.tabActiveBg : 'transparent',
                  color: i === 0 ? theme.accent : theme.muted,
                  fontWeight: i === 0 ? 600 : 400,
                  cursor: 'pointer',
                }}>{tab}</span>
              ))}
            </div>
          </div>

          {/* Toolbar row */}
          <div style={{
            height: 48,
            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            padding: '0 22px',
            borderBottom: `1px solid ${theme.windowBorder}`,
            background: theme.toolbarBg,
            gap: 16,
          }}>
            {/* Left: row actions */}
            <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
              <button style={iconBtn(theme)}><HIcon.Copy /> <span>Copy</span></button>
              <button style={iconBtn(theme)}><HIcon.Pencil /> <span>Edit</span></button>
              <button style={{ ...iconBtn(theme), color: theme.danger }}><HIcon.Trash /> <span>Delete</span></button>
              <span style={{ width: 1, height: 18, background: theme.windowBorder, margin: '0 4px' }} />
              <span style={{ fontSize: 11, color: theme.muted }}>1 row selected</span>
            </div>

            {/* Right: pagination + primary CTA */}
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <div style={{
                display: 'inline-flex', alignItems: 'center',
                fontSize: 11, color: theme.muted, fontFamily: fontMono,
              }}>
                <span style={subtleBtn(theme)}><HIcon.ArrowLeft /></span>
                <span style={{ padding: '0 10px' }}>1 / 1</span>
                <span style={subtleBtn(theme)}><HIcon.ArrowRight /></span>
              </div>
              <button style={primaryBtn(theme)}>
                <HIcon.Plus /> <span style={{ marginLeft: 4 }}>Insert Row</span>
              </button>
            </div>
          </div>

          {/* ==== TABLE + DETAIL PANEL ==== */}
          <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
            {/* Data table */}
            <div style={{ flex: 1, overflow: 'auto', position: 'relative' }}>
              <table style={{
                width: '100%',
                borderCollapse: 'separate',
                borderSpacing: 0,
                fontFamily: fontMono,
                fontSize: 13,
              }}>
                <thead>
                  <tr>
                    {/* Checkbox column */}
                    <th style={thStyle(theme, { width: 32, textAlign: 'center' })}>
                      <span style={checkboxStyle(theme, false)} />
                    </th>
                    {/* Row # */}
                    <th style={thStyle(theme, { width: 44, textAlign: 'right', paddingRight: 14, color: theme.label })}>#</th>
                    {data.columns.map((col, i) => (
                      <th key={i} style={thStyle(theme)}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                          {col.pk && <span style={{ color: theme.types.pk, display: 'inline-flex' }} title="Primary key"><HIcon.Key /></span>}
                          <span style={{ color: theme.text, fontWeight: 600 }}>{col.name}</span>
                          <span style={{
                            fontSize: 9, fontWeight: 600,
                            color: theme.types[col.type] || theme.muted,
                            opacity: 0.75,
                            textTransform: 'lowercase',
                            letterSpacing: 0.3,
                            padding: '1px 5px',
                            borderRadius: 3,
                            background: theme.typeChipBg,
                          }}>{col.type}</span>
                          <span style={{ marginLeft: 'auto', color: col.sort ? theme.accent : theme.muted, display: 'inline-flex' }}>
                            {col.sort === 'asc' ? <HIcon.SortAsc /> : <HIcon.SortNone />}
                          </span>
                        </div>
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {data.rows.map((row, ri) => {
                    const isFirst = ri === 0;
                    return (
                      <tr key={ri} style={{
                        background: row.selected ? theme.rowSelectedBg : (ri % 2 === 1 && theme.rowStripe ? theme.rowStripe : 'transparent'),
                      }}>
                        <td style={tdStyle(theme, { textAlign: 'center', width: 32 })}>
                          <span style={checkboxStyle(theme, row.selected)}>
                            {row.selected && <svg width="10" height="10" viewBox="0 0 16 16" fill="none"><path d="M3 8.5L6.5 12 13 4" stroke={theme.checkColor} strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"/></svg>}
                          </span>
                        </td>
                        <td style={tdStyle(theme, { textAlign: 'right', paddingRight: 14, color: theme.label, fontSize: 11, fontFamily: fontMono })}>
                          {ri + 1}
                        </td>
                        {row.data.map((val, ci) => {
                          const col = data.columns[ci];
                          const isFocused = isFirst && col.name === 'email';
                          return (
                            <td key={ci} style={tdStyle(theme, {
                              borderLeft: isFocused ? `none` : 'none',
                            })}>
                              <CellV2
                                value={val}
                                type={col.type}
                                theme={theme}
                                focused={isFocused}
                                dirty={isFocused && row.dirty}
                                columnName={col.name}
                              />
                            </td>
                          );
                        })}
                      </tr>
                    );
                  })}
                  {/* "Insert new row" affordance */}
                  <tr>
                    <td colSpan={data.columns.length + 2} style={{
                      padding: '14px 22px',
                      color: theme.muted, fontSize: 12, fontStyle: 'italic',
                      fontFamily: fontUI,
                      borderBottom: `1px solid ${theme.rowDivider}`,
                      cursor: 'pointer',
                    }}>
                      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                        <HIcon.Plus /> Add new row…
                      </span>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            {/* Detail panel — right side */}
            <div style={{
              width: 320,
              borderLeft: `1px solid ${theme.windowBorder}`,
              background: theme.panelBg,
              display: 'flex', flexDirection: 'column',
              fontSize: 12,
            }}>
              <div style={{
                padding: '14px 18px 10px',
                borderBottom: `1px solid ${theme.windowBorder}`,
                display: 'flex', justifyContent: 'space-between', alignItems: 'center',
              }}>
                <div>
                  <div style={{ fontSize: 10, color: theme.label, letterSpacing: 1.2, textTransform: 'uppercase', fontWeight: 700 }}>Row Detail</div>
                  <div style={{ fontSize: 13, color: theme.text, fontWeight: 600, marginTop: 3 }}>id = <span style={{ color: theme.types.int }}>1</span></div>
                </div>
                <div style={{ display: 'flex', gap: 4 }}>
                  <span style={iconBtnSm(theme)}><HIcon.Copy /></span>
                  <span style={iconBtnSm(theme)}><HIcon.Pencil /></span>
                </div>
              </div>
              <div style={{ flex: 1, overflowY: 'auto', padding: '8px 18px 16px' }}>
                {data.columns.map((col, ci) => {
                  const val = data.rows[0].data[ci];
                  const isDirty = col.name === 'email';
                  return (
                    <div key={ci} style={{
                      padding: '8px 0',
                      borderBottom: `1px solid ${theme.rowDivider}`,
                      display: 'grid', gridTemplateColumns: '100px 1fr',
                      gap: 12, alignItems: 'baseline',
                    }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
                        {col.pk && <span style={{ color: theme.types.pk, display: 'inline-flex' }}><HIcon.Key /></span>}
                        <span style={{ fontSize: 11, color: theme.muted, fontFamily: fontMono }}>{col.name}</span>
                        {isDirty && <span style={{ width: 5, height: 5, borderRadius: '50%', background: theme.dirtyDot, marginLeft: 2 }} />}
                      </div>
                      <div style={{ fontFamily: fontMono, fontSize: 12, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        <CellV2 value={val} type={col.type} theme={theme} columnName={col.name} />
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>

          {/* ==== STATUS BAR ==== */}
          <div style={{
            height: 30,
            display: 'flex', justifyContent: 'space-between', alignItems: 'center',
            padding: '0 18px',
            borderTop: `1px solid ${theme.windowBorder}`,
            background: theme.statusBarBg,
            color: theme.muted, fontSize: 11, fontFamily: fontMono,
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
              <span style={{ display: 'flex', alignItems: 'center' }}>
                <HIcon.Dot color={theme.accent} />SQL · public.users
              </span>
              <span style={{ display: 'flex', alignItems: 'center', color: theme.dirtyDot }}>
                <HIcon.Dot color={theme.dirtyDot} />1 unsaved change
              </span>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
              <span>4 rows · 1 selected</span>
              <span>UTF-8</span>
              <span>queried in 12ms</span>
              <span>harness v0.3.0</span>
              <span style={{ display: 'flex', alignItems: 'center' }}>
                <HIcon.Dot color={theme.successDot} />local
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Button styles
// ============================================================================
function thStyle(theme, extra = {}) {
  return {
    textAlign: 'left',
    padding: '10px 14px',
    borderBottom: `1px solid ${theme.windowBorder}`,
    background: theme.tableHeaderBg,
    color: theme.tableHeaderFg,
    fontWeight: 600,
    whiteSpace: 'nowrap',
    position: 'sticky', top: 0,
    fontSize: 11,
    letterSpacing: 0.2,
    ...extra,
  };
}
function tdStyle(theme, extra = {}) {
  return {
    padding: '12px 14px',
    borderBottom: `1px solid ${theme.rowDivider}`,
    whiteSpace: 'nowrap',
    verticalAlign: 'middle',
    ...extra,
  };
}
function checkboxStyle(theme, checked) {
  return {
    display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
    width: 14, height: 14,
    border: `1.5px solid ${checked ? theme.accent : theme.checkBorder}`,
    background: checked ? theme.accent : 'transparent',
    borderRadius: 3,
    cursor: 'pointer',
  };
}
function iconBtn(theme) {
  return {
    display: 'inline-flex', alignItems: 'center', gap: 6,
    padding: '5px 10px',
    background: 'transparent',
    color: theme.btnFg,
    border: `1px solid transparent`,
    borderRadius: 5,
    fontFamily: theme.fontUI,
    fontSize: 12,
    cursor: 'pointer',
  };
}
function iconBtnSm(theme) {
  return {
    display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
    width: 24, height: 24,
    color: theme.muted,
    border: `1px solid ${theme.windowBorder}`,
    borderRadius: 4,
    cursor: 'pointer',
  };
}
function subtleBtn(theme) {
  return {
    display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
    width: 22, height: 22,
    color: theme.muted,
    cursor: 'pointer',
  };
}
function primaryBtn(theme) {
  return {
    display: 'inline-flex', alignItems: 'center',
    padding: '7px 14px',
    background: theme.primaryBtnBg,
    color: theme.primaryBtnFg,
    border: `1px solid ${theme.primaryBtnBorder}`,
    borderRadius: 6,
    fontFamily: theme.fontUI,
    fontSize: 12, fontWeight: 600,
    cursor: 'pointer',
    boxShadow: theme.primaryBtnShadow,
  };
}

window.HarnessGUIv2 = HarnessGUIv2;
