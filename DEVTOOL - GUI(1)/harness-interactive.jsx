// harness-interactive.jsx
// Full interactive Paper-theme database manager.
// Depends on: harness-icons.jsx (HIcon in window scope)

const { useState, useRef, useEffect } = React;

// ── DATA ─────────────────────────────────────────────────────────────────────
const ALL_TABLES = [
  '_sqlx_migrations','member_type','member_type_translations',
  'members','members_invitations','members_invitations_emails',
  'members_invitations_roles','oauth_accounts','oauth_auth_code',
  'oauth_client','oauth_endpoints','oauth_providers',
  'oauth_refresh_token','passkeys','password_reset_codes',
  'refresh_tokens','settings','users',
  'users_emails','users_otp','users_preferences',
  'users_profiles','users_roles',
];
const ALL_VIEWS = [
  'view_oauth_authorizations','view_oauth_token_list',
  'view_oauth_userinfo_full','view_user_oauth_validations',
  'view_users_profile','view_users_profile_full',
  'view_users_summary','view_users_teams',
];
const COLS = [
  { name:'id',           type:'int',       pk:true  },
  { name:'uuid',         type:'uuid'               },
  { name:'firstname',    type:'string'             },
  { name:'lastname',     type:'string'             },
  { name:'password',     type:'hash'               },
  { name:'email',        type:'email'              },
  { name:'created_at',   type:'timestamp'          },
  { name:'updated_at',   type:'timestamp'          },
  { name:'is_active',    type:'bool'               },
  { name:'is_consultant',type:'bool'               },
];
const SEED_ROWS = [
  { id:1, uuid:'015cb15a-86d8-77b0-bb2e-a7c4fde23109', firstname:'Jostick', lastname:'Quiel',  password:'$argon2id$v=19$m=19456,t=2,p=1$cHJpdmF0ZQ$abcdef1234567890', email:'jostick516@gmail.com', created_at:'2025-06-26 18:24:32', updated_at:'2025-06-26 18:24:32', is_active:true,  is_consultant:false },
  { id:2, uuid:'015cb15a-86d8-75cc-b3a1-d2f8abc45672', firstname:'Jostick', lastname:'Quiel',  password:'$argon2id$v=19$m=19456,t=2,p=1$cHJpdmF0ZQ$bcdef12345678901', email:'treyes@ciat.org',       created_at:'2025-06-26 18:25:13', updated_at:'2025-06-26 18:25:13', is_active:true,  is_consultant:false },
  { id:3, uuid:'015cb15a-86d8-718e-9c4b-e6a9bcd78903', firstname:'Jostick', lastname:'Quiel',  password:'$argon2id$v=19$m=19456,t=2,p=1$cHJpdmF0ZQ$cdef123456789012', email:'ttorres@ciat.org',      created_at:'2025-06-26 18:25:29', updated_at:'2025-06-26 18:25:29', is_active:true,  is_consultant:false },
  { id:4, uuid:'015cb15a-86d8-786f-9f1d-c3b2def01234', firstname:'Jose',    lastname:'Baules', password:'$argon2id$v=19$m=19456,t=2,p=1$cHJpdmF0ZQ$def1234567890123', email:'jbaules@ciat.org',      created_at:'2025-06-26 18:25:49', updated_at:'2026-05-26 08:23:10', is_active:true,  is_consultant:false },
];

// ── THEME ─────────────────────────────────────────────────────────────────────
const P = {
  mono: "'JetBrains Mono','Fira Code',ui-monospace,Menlo,monospace",
  ui:   "'Inter',ui-sans-serif,system-ui,sans-serif",
  windowBg:   '#fdfcf8', panelBg:'#f5f3ed', canvasBg:'#ffffff',
  toolbarBg:  '#fdfcf8', headerBg:'#f8f7f2', statusBg:'#f0ede5',
  railBg:     '#ede9df', titleBarBg:'#f0ede5',
  border:     '#e2ded6', rowDivider:'rgba(0,0,0,0.055)',
  rowHover:   'rgba(13,122,98,0.04)', rowSelected:'rgba(13,122,98,0.08)',
  rowStripe:  'rgba(0,0,0,0.014)',
  text:       '#1c1a16', textSec:'#5c5850', muted:'#9e9890', label:'#b0a898',
  accent:     '#0d7a62', accentBg:'rgba(13,122,98,0.08)', accentRing:'rgba(13,122,98,0.20)',
  railActive: 'rgba(13,122,98,0.10)', sideActive:'rgba(13,122,98,0.09)',
  inputBg:    '#ede9df', inputBorder:'#d8d4cc',
  btnFg:      '#3c3a36', primaryBg:'#0d7a62', primaryFg:'#ffffff', dangerFg:'#c53030',
  checkBorder:'#c8c4bc', checkActiveBg:'#0d7a62', checkFg:'#ffffff',
  dirtyDot:   '#d08030', successDot:'#2d9d5b',
  tooltipBg:  '#1c1a16', tooltipFg:'#f5f3ed',
  kbdBg:      '#e8e4da', kbdBorder:'#ccc8be',
  countBg:    'rgba(0,0,0,0.06)', tabsBg:'rgba(0,0,0,0.05)', tabActiveBg:'#fdfcf8',
  badgeBg:    '#0d7a62', badgeFg:'#ffffff',
  connBg:     'rgba(13,122,98,0.06)', connBorder:'rgba(13,122,98,0.15)',
  typeChipBg: 'rgba(0,0,0,0.055)',
  cellEditBg: '#ffffff', cellBorder:'#0d7a62', cellRing:'rgba(13,122,98,0.16)',
  types: {
    int:'#b85c00', pk:'#c07020',
    uuid:'#6b35b8', uuidBg:'rgba(107,53,184,0.07)',
    string:'#1c1a16',
    hash:'#b0a898',
    email:'#0f52c8', emailDim:'#8a9fd8',
    timestamp:'#c06820', timestampDate:'#8a7a60', timestampTime:'#c06820',
    boolTrue:  { fg:'#16a34a', bg:'rgba(22,163,74,0.10)',  dot:'#16a34a', border:'rgba(22,163,74,0.25)'  },
    boolFalse: { fg:'#9a9288', bg:'rgba(0,0,0,0.04)',      dot:'#c8c4bc', border:'rgba(0,0,0,0.09)'      },
  },
};

// ── HELPERS ───────────────────────────────────────────────────────────────────
function mid(str, n=18) {
  if (!str || str.length <= n) return str;
  const k = Math.floor((n-1)/2);
  return str.slice(0,k)+'…'+str.slice(-k);
}
function applySortOnRows(rows, col, dir) {
  if (!col || dir === 'none') return rows;
  return [...rows].sort((a,b) => {
    let va = a[col], vb = b[col];
    if (typeof va === 'boolean') { va = va?1:0; vb = vb?1:0; }
    if (va < vb) return dir==='asc'?-1:1;
    if (va > vb) return dir==='asc'?1:-1;
    return 0;
  });
}

// ── CELL ──────────────────────────────────────────────────────────────────────
function DataCell({ val, col, editing, editVal, onChange, onCommit, onCancel, onTipEnter, onTipLeave }) {
  const T = P.types;
  if (editing) {
    return (
      <input autoFocus value={editVal}
        onChange={e=>onChange(e.target.value)}
        onBlur={onCommit}
        onKeyDown={e=>{if(e.key==='Enter')onCommit();if(e.key==='Escape')onCancel();}}
        style={{
          fontFamily:P.mono, fontSize:12, padding:'4px 8px',
          border:`1.5px solid ${P.cellBorder}`, borderRadius:4,
          background:P.cellEditBg, boxShadow:`0 0 0 3px ${P.cellRing}`,
          color:P.text, outline:'none', width:'90%', minWidth:80,
        }}
      />
    );
  }
  if (val === null || val === undefined) {
    return <span style={{color:P.muted,fontStyle:'italic',fontSize:11}}>NULL</span>;
  }
  if (col.type==='bool') {
    const cfg = val ? T.boolTrue : T.boolFalse;
    return (
      <span style={{
        display:'inline-flex',alignItems:'center',gap:6,
        padding:'3px 10px 3px 8px',borderRadius:999,
        background:cfg.bg,color:cfg.fg,fontSize:11,fontWeight:600,
        border:`1px solid ${cfg.border}`,cursor:'pointer',userSelect:'none',
      }}>
        <span style={{width:6,height:6,borderRadius:'50%',background:cfg.dot}}/>
        {val?'true':'false'}
      </span>
    );
  }
  if (col.type==='int') {
    return (
      <span style={{color:T.int,fontWeight:600,fontFamily:P.mono,fontSize:12,display:'inline-flex',alignItems:'center',gap:5}}>
        {col.pk&&<span style={{color:T.pk,opacity:.8,display:'inline-flex'}}><HIcon.Key/></span>}
        {val}
      </span>
    );
  }
  if (col.type==='uuid') {
    return (
      <span onMouseEnter={onTipEnter} onMouseLeave={onTipLeave}
        style={{color:T.uuid,fontFamily:P.mono,fontSize:12,background:T.uuidBg,padding:'2px 7px',borderRadius:3,cursor:'default'}}>
        {mid(String(val),18)}
      </span>
    );
  }
  if (col.type==='hash') {
    return (
      <span onMouseEnter={onTipEnter} onMouseLeave={onTipLeave}
        style={{color:T.hash,fontFamily:P.mono,fontSize:11,cursor:'default'}}>
        {mid(String(val),16)}
      </span>
    );
  }
  if (col.type==='email') {
    const [local,domain] = String(val).split('@');
    return (
      <span style={{fontFamily:P.mono,fontSize:12}}>
        <span style={{color:T.email,fontWeight:500}}>{local}</span>
        {domain&&<span style={{color:T.emailDim}}>@{domain}</span>}
      </span>
    );
  }
  if (col.type==='timestamp') {
    const m = String(val).match(/^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})/);
    if (m) return (
      <span style={{fontFamily:P.mono,fontSize:12,display:'inline-flex',alignItems:'baseline',gap:7}}>
        <span style={{color:T.timestampDate}}>{m[1]}</span>
        <span style={{color:T.timestampTime,fontWeight:500}}>{m[2]}</span>
      </span>
    );
    return <span style={{color:T.timestamp,fontFamily:P.mono,fontSize:12}}>{val}</span>;
  }
  return <span style={{color:T.string,fontSize:13}}>{val}</span>;
}

// ── QUERY TAB ─────────────────────────────────────────────────────────────────
function QueryTab({ table }) {
  const sql = `SELECT *\nFROM public.${table}\nORDER BY id ASC\nLIMIT 100;`;
  return (
    <div style={{flex:1,display:'flex',flexDirection:'column',padding:22,gap:12,background:P.canvasBg}}>
      <div style={{borderRadius:8,overflow:'hidden',border:`1px solid ${P.border}`,flex:1,display:'flex',flexDirection:'column'}}>
        <div style={{padding:'8px 14px',background:P.panelBg,borderBottom:`1px solid ${P.border}`,display:'flex',justifyContent:'space-between',alignItems:'center',fontSize:11,color:P.muted}}>
          <span style={{fontFamily:P.mono}}>SQL Editor</span>
          <button style={{padding:'4px 14px',borderRadius:5,fontSize:11,background:P.primaryBg,color:P.primaryFg,border:'none',cursor:'pointer',fontWeight:600,fontFamily:P.ui,boxShadow:'0 2px 6px rgba(13,122,98,0.25)'}}>▶  Run</button>
        </div>
        <textarea defaultValue={sql} style={{flex:1,padding:'16px 18px',fontFamily:P.mono,fontSize:13,color:P.text,background:P.canvasBg,border:'none',outline:'none',resize:'none',lineHeight:1.8,minHeight:120}} />
      </div>
      <div style={{padding:'10px 14px',background:P.panelBg,border:`1px solid ${P.border}`,borderRadius:6,fontSize:11,color:P.muted,fontFamily:P.mono}}>
        ✓ &nbsp;Query returned 4 rows in 8ms
      </div>
    </div>
  );
}

// ── SCHEMA TAB ────────────────────────────────────────────────────────────────
function SchemaTab() {
  const typeLabel = t => ({'int':'integer','uuid':'uuid','string':'varchar(255)','hash':'text','email':'varchar(255)','timestamp':'timestamptz','bool':'boolean'}[t]||t);
  const typeColor = t => ({'int':'#b85c00','uuid':'#6b35b8','string':'#0f52c8','hash':'#9e9890','email':'#0f52c8','timestamp':'#c06820','bool':'#16a34a'}[t]||P.muted);
  return (
    <div style={{flex:1,overflow:'auto',padding:22,background:P.canvasBg}}>
      <table style={{width:'100%',borderCollapse:'collapse',fontFamily:P.mono,fontSize:12}}>
        <thead>
          <tr>
            {['Column','Type','Nullable','Default','Constraint'].map(h=>(
              <th key={h} style={{textAlign:'left',padding:'8px 14px',borderBottom:`2px solid ${P.border}`,color:P.muted,fontWeight:700,fontSize:10,letterSpacing:1,textTransform:'uppercase'}}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {COLS.map((col,i)=>(
            <tr key={i} style={{background:i%2?P.rowStripe:'transparent'}}>
              <td style={{padding:'10px 14px',borderBottom:`1px solid ${P.rowDivider}`,color:P.text,fontWeight:col.pk?600:400}}>
                <span style={{display:'inline-flex',alignItems:'center',gap:6}}>
                  {col.pk&&<span style={{color:'#c07020',display:'inline-flex'}}><HIcon.Key/></span>}
                  {col.name}
                </span>
              </td>
              <td style={{padding:'10px 14px',borderBottom:`1px solid ${P.rowDivider}`}}>
                <span style={{color:typeColor(col.type),fontWeight:600,background:'rgba(0,0,0,0.04)',padding:'2px 7px',borderRadius:3}}>{typeLabel(col.type)}</span>
              </td>
              <td style={{padding:'10px 14px',borderBottom:`1px solid ${P.rowDivider}`,color:P.muted}}>{col.pk?'NO':'YES'}</td>
              <td style={{padding:'10px 14px',borderBottom:`1px solid ${P.rowDivider}`,color:P.muted}}>{col.pk?'nextval(…)':col.type==='uuid'?'gen_random_uuid()':col.type==='bool'?'false':'—'}</td>
              <td style={{padding:'10px 14px',borderBottom:`1px solid ${P.rowDivider}`,color:col.pk?P.types.pk:P.muted,fontWeight:col.pk?600:400}}>{col.pk?'PRIMARY KEY':''}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ── TOOLTIP ───────────────────────────────────────────────────────────────────
function TipBox({ text, x, y }) {
  return (
    <div style={{
      position:'fixed',left:x+12,top:y-10,
      background:P.tooltipBg,color:P.tooltipFg,
      padding:'6px 10px',borderRadius:5,
      fontSize:11,fontFamily:P.mono,
      maxWidth:420,wordBreak:'break-all',
      zIndex:9999,pointerEvents:'none',
      boxShadow:'0 4px 16px rgba(0,0,0,0.2)',letterSpacing:.3,
    }}>{text}</div>
  );
}

// ── MAIN ──────────────────────────────────────────────────────────────────────
function HarnessPaper() {
  const [expandedSchemas, setExpandedSchemas] = useState(new Set(['public']));
  const [activeTable, setActiveTable]         = useState('users');
  const [filter, setFilter]                   = useState('');
  const [sortCol, setSortCol]                 = useState('id');
  const [sortDir, setSortDir]                 = useState('asc');
  const [selected, setSelected]               = useState(new Set([0]));
  const [activeTab, setActiveTab]             = useState('Data');
  const [hoveredRow, setHoveredRow]           = useState(null);
  const [editCell, setEditCell]               = useState(null);  // {ri, name}
  const [editVal, setEditVal]                 = useState('');
  const [rows, setRows]                       = useState(SEED_ROWS);
  const [dirty, setDirty]                     = useState(new Set());
  const [tooltip, setTooltip]                 = useState(null);  // {text,x,y}
  const [detailIdx, setDetailIdx]             = useState(0);
  const tipTimer                              = useRef(null);

  const filteredTables = ALL_TABLES.filter(t => t.toLowerCase().includes(filter.toLowerCase()));
  const displayRows    = applySortOnRows(rows, sortCol, sortDir);
  const detailRow      = displayRows[detailIdx] || displayRows[0];
  const selCount       = selected.size;

  function toggleSort(colName) {
    if (sortCol===colName) setSortDir(d=>d==='asc'?'desc':d==='desc'?'none':'asc');
    else { setSortCol(colName); setSortDir('asc'); }
  }

  function toggleRow(ri) {
    setSelected(prev=>{ const n=new Set(prev); n.has(ri)?n.delete(ri):n.add(ri); return n; });
    setDetailIdx(ri);
  }

  function toggleAll() {
    setSelected(selected.size===displayRows.length?new Set():new Set(displayRows.map((_,i)=>i)));
  }

  function startEdit(ri, colName, val) {
    const col = COLS.find(c=>c.name===colName);
    if (!col||col.type==='uuid'||col.type==='hash'||col.type==='timestamp') return;
    if (col.type==='bool') {
      const realIdx = rows.indexOf(displayRows[ri]);
      const next = [...rows]; next[realIdx]={...next[realIdx],[colName]:!next[realIdx][colName]};
      setRows(next); setDirty(prev=>new Set([...prev,realIdx])); return;
    }
    setEditCell({ri,name:colName}); setEditVal(String(val));
  }

  function commitEdit() {
    if (!editCell) return;
    const {ri,name} = editCell;
    const col = COLS.find(c=>c.name===name);
    const realIdx = rows.indexOf(displayRows[ri]);
    const parsed = col.type==='int'?parseInt(editVal)||displayRows[ri][name]:editVal;
    const next=[...rows]; next[realIdx]={...next[realIdx],[name]:parsed};
    setRows(next); setDirty(prev=>new Set([...prev,realIdx])); setEditCell(null);
  }

  function showTip(text,e) {
    if(tipTimer.current) clearTimeout(tipTimer.current);
    const {clientX:x,clientY:y}=e;
    tipTimer.current=setTimeout(()=>setTooltip({text,x,y}),400);
  }
  function hideTip() { if(tipTimer.current) clearTimeout(tipTimer.current); setTooltip(null); }

  const SCHEMAS = [{name:'public'},{name:'audit'},{name:'catalogs'},{name:'ciat_data'},{name:'consultores'}];

  const s = {
    thBase: {textAlign:'left',padding:'10px 12px',borderBottom:`1px solid ${P.border}`,background:P.headerBg,whiteSpace:'nowrap',position:'sticky',top:0,fontSize:12,fontWeight:600},
    tdBase: {padding:'12px 12px',borderBottom:`1px solid ${P.rowDivider}`,whiteSpace:'nowrap',verticalAlign:'middle'},
  };

  return (
    <div style={{width:'100%',height:'100%',background:P.windowBg,color:P.text,fontFamily:P.ui,fontSize:13,display:'flex',flexDirection:'column',borderRadius:10,overflow:'hidden',border:`1px solid ${P.border}`,boxShadow:'0 32px 80px rgba(40,35,25,0.16)'}}>

      {/* TITLE BAR */}
      <div style={{height:42,background:P.titleBarBg,borderBottom:`1px solid ${P.border}`,display:'grid',gridTemplateColumns:'140px 1fr 140px',alignItems:'center',padding:'0 14px'}}>
        <div style={{display:'flex',gap:8}}>
          {['#ed6a5e','#f4bf4f','#61c554'].map((c,i)=>(
            <span key={i} style={{width:12,height:12,borderRadius:'50%',background:c,cursor:'pointer'}}/>
          ))}
        </div>
        <div style={{display:'flex',justifyContent:'center'}}>
          <div style={{display:'flex',alignItems:'center',gap:8,background:P.inputBg,border:`1px solid ${P.inputBorder}`,borderRadius:6,padding:'5px 12px',minWidth:300,maxWidth:480,fontSize:12,color:P.muted,cursor:'text'}}>
            <HIcon.Search size={12}/>
            <span>Search tables, queries, settings…</span>
            <span style={{marginLeft:'auto',display:'inline-flex',alignItems:'center',gap:3}}>
              <kbd style={{padding:'1px 5px',borderRadius:3,background:P.kbdBg,border:`1px solid ${P.kbdBorder}`,fontSize:10,fontFamily:P.mono,display:'inline-flex',alignItems:'center',gap:2}}>
                <HIcon.Command size={10}/>&nbsp;K
              </kbd>
            </span>
          </div>
        </div>
        <div style={{display:'flex',justifyContent:'flex-end'}}>
          <div style={{display:'flex',alignItems:'center',gap:6,fontSize:11,color:P.muted,padding:'4px 8px',border:`1px solid ${P.border}`,borderRadius:4}}>
            <HIcon.Dot color={P.successDot}/>localhost:5432
          </div>
        </div>
      </div>

      <div style={{flex:1,display:'flex',minHeight:0}}>
        {/* RAIL */}
        <div style={{width:68,background:P.railBg,borderRight:`1px solid ${P.border}`,display:'flex',flexDirection:'column',alignItems:'center',paddingTop:14,gap:4}}>
          {[{icon:<HIcon.Sparkles/>,label:'Agents'},{icon:<HIcon.SQL/>,label:'SQL',active:true},{icon:<HIcon.SSH/>,label:'SSH'},{icon:<HIcon.Memory/>,label:'Memory'},{icon:<HIcon.Settings/>,label:'Settings'}].map((item,i)=>(
            <div key={i} style={{display:'flex',flexDirection:'column',alignItems:'center',gap:5,color:item.active?P.accent:P.muted,padding:'8px 10px',borderRadius:8,width:54,background:item.active?P.railActive:'transparent',cursor:'pointer',position:'relative'}}>
              {item.active&&<span style={{position:'absolute',left:-14,width:3,height:22,borderRadius:2,background:P.accent,top:'50%',transform:'translateY(-50%)'}}/>}
              {item.icon}
              <span style={{fontSize:10,fontWeight:item.active?600:400}}>{item.label}</span>
            </div>
          ))}
        </div>

        {/* SIDEBAR */}
        <div style={{width:272,background:P.panelBg,borderRight:`1px solid ${P.border}`,display:'flex',flexDirection:'column',overflow:'hidden'}}>
          <div style={{padding:'12px 12px 8px'}}>
            <div style={{display:'flex',alignItems:'center',gap:8,padding:'8px 10px',borderRadius:7,background:P.connBg,border:`1px solid ${P.connBorder}`}}>
              <span style={{color:P.accent,display:'inline-flex'}}><HIcon.Database/></span>
              <div style={{flex:1,minWidth:0}}>
                <div style={{fontSize:12.5,fontWeight:600,whiteSpace:'nowrap',overflow:'hidden',textOverflow:'ellipsis'}}>
                  local = <span style={{color:P.accent}}>sara</span>
                </div>
                <div style={{fontSize:10,color:P.muted,marginTop:1}}>PostgreSQL · 15.4</div>
              </div>
              <HIcon.Dot color={P.successDot}/>
            </div>
          </div>
          <div style={{padding:'0 12px 8px'}}>
            <div style={{display:'flex',alignItems:'center',gap:7,background:P.inputBg,border:`1px solid ${P.inputBorder}`,borderRadius:5,padding:'6px 10px',fontSize:12}}>
              <span style={{color:P.muted,display:'inline-flex'}}><HIcon.Search size={12}/></span>
              <input value={filter} onChange={e=>setFilter(e.target.value)} placeholder="Filter tables…"
                style={{border:'none',background:'transparent',color:P.text,fontSize:12,outline:'none',fontFamily:P.ui,flex:1,width:'100%'}}/>
              {filter&&<span onClick={()=>setFilter('')} style={{color:P.muted,cursor:'pointer',fontSize:14,lineHeight:1}}>×</span>}
            </div>
          </div>
          <div style={{padding:'4px 14px 4px',display:'flex',justifyContent:'space-between',alignItems:'center'}}>
            <span style={{fontSize:10,letterSpacing:1.4,color:P.label,textTransform:'uppercase',fontWeight:700}}>Schemas</span>
            <span style={{color:P.muted,cursor:'pointer',display:'inline-flex'}}><HIcon.Plus/></span>
          </div>
          <div style={{flex:1,overflowY:'auto',paddingBottom:16}}>
            {SCHEMAS.map((schema,si)=>(
              <div key={si}>
                <div onClick={()=>setExpandedSchemas(prev=>{const n=new Set(prev);n.has(schema.name)?n.delete(schema.name):n.add(schema.name);return n;})}
                  style={{display:'flex',alignItems:'center',gap:8,padding:'6px 10px 6px 12px',cursor:'pointer',fontSize:12.5,color:P.text}}>
                  <span style={{color:P.muted,display:'inline-flex',width:10}}><HIcon.Chevron open={expandedSchemas.has(schema.name)}/></span>
                  <span style={{color:P.muted,display:'inline-flex'}}><HIcon.Schema/></span>
                  <span style={{flex:1,fontWeight:500}}>{schema.name}</span>
                </div>
                {expandedSchemas.has(schema.name)&&(
                  <div>
                    <div style={{fontSize:9.5,letterSpacing:1.4,color:P.label,padding:'8px 14px 3px 30px',textTransform:'uppercase',fontWeight:700}}>
                      Tables · {filteredTables.length}
                    </div>
                    {filteredTables.map((tbl,ti)=>{
                      const active=tbl===activeTable;
                      return (
                        <div key={ti} onClick={()=>setActiveTable(tbl)}
                          style={{display:'flex',alignItems:'center',gap:8,padding:'5px 10px 5px 26px',cursor:'pointer',fontSize:12.5,color:active?P.accent:P.text,background:active?P.sideActive:'transparent',borderLeft:active?`2px solid ${P.accent}`:'2px solid transparent',fontWeight:active?600:400}}>
                          <span style={{color:active?P.accent:P.muted,display:'inline-flex'}}><HIcon.Table/></span>
                          <span style={{flex:1,overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap'}}>{tbl}</span>
                          {tbl==='users'&&<span style={{fontSize:10,color:P.muted,background:P.countBg,padding:'1px 6px',borderRadius:8}}>4</span>}
                        </div>
                      );
                    })}
                    <div style={{fontSize:9.5,letterSpacing:1.4,color:P.label,padding:'12px 14px 3px 30px',textTransform:'uppercase',fontWeight:700}}>Views · {ALL_VIEWS.length}</div>
                    {ALL_VIEWS.map((v,vi)=>(
                      <div key={vi} style={{display:'flex',alignItems:'center',gap:8,padding:'5px 10px 5px 26px',cursor:'pointer',fontSize:12.5,color:P.textSec}}>
                        <span style={{color:P.muted,display:'inline-flex'}}><HIcon.View/></span>
                        <span style={{overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap'}}>{v}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* MAIN */}
        <div style={{flex:1,display:'flex',flexDirection:'column',background:P.canvasBg,minWidth:0}}>
          {/* Breadcrumb + Tabs */}
          <div style={{height:44,display:'flex',alignItems:'center',justifyContent:'space-between',padding:'0 20px',borderBottom:`1px solid ${P.border}`,background:P.toolbarBg}}>
            <div style={{display:'flex',alignItems:'center',gap:10,fontSize:13}}>
              <span style={{color:P.muted,display:'inline-flex'}}><HIcon.Database/></span>
              <span style={{color:P.muted}}>sara</span>
              <span style={{color:P.muted,opacity:.4}}>/</span>
              <span style={{color:P.textSec}}>public</span>
              <span style={{color:P.muted,opacity:.4}}>/</span>
              <span style={{color:P.text,fontWeight:700}}>{activeTable}</span>
              <span style={{fontSize:10,color:P.muted,background:P.countBg,padding:'2px 8px',borderRadius:4,marginLeft:2}}>{rows.length} rows</span>
            </div>
            <div style={{display:'inline-flex',gap:2,padding:3,background:P.tabsBg,border:`1px solid ${P.border}`,borderRadius:7,fontSize:12}}>
              {['Data','Query','Schema','Relations'].map((tab,i)=>(
                <span key={i} onClick={()=>setActiveTab(tab)}
                  style={{padding:'5px 14px',borderRadius:5,background:activeTab===tab?P.tabActiveBg:'transparent',color:activeTab===tab?P.accent:P.muted,fontWeight:activeTab===tab?600:400,cursor:'pointer',boxShadow:activeTab===tab?'0 1px 3px rgba(0,0,0,0.08)':'none',border:activeTab===tab?`1px solid ${P.border}`:'1px solid transparent',transition:'all .12s'}}>
                  {tab}
                </span>
              ))}
            </div>
          </div>

          {activeTab==='Data'&&(<>
            {/* Toolbar */}
            <div style={{height:48,display:'flex',alignItems:'center',justifyContent:'space-between',padding:'0 20px',borderBottom:`1px solid ${P.border}`,background:P.toolbarBg}}>
              <div style={{display:'flex',gap:4,alignItems:'center'}}>
                <button style={{display:'inline-flex',alignItems:'center',gap:6,padding:'5px 10px',background:'transparent',color:P.btnFg,border:'1px solid transparent',borderRadius:5,fontFamily:P.ui,fontSize:12,cursor:'pointer'}}>
                  <HIcon.Copy/> <span>Copy</span>
                </button>
                <button style={{display:'inline-flex',alignItems:'center',gap:6,padding:'5px 10px',background:'transparent',color:P.btnFg,border:'1px solid transparent',borderRadius:5,fontFamily:P.ui,fontSize:12,cursor:'pointer'}}>
                  <HIcon.Pencil/> <span>Edit</span>
                </button>
                <button style={{display:'inline-flex',alignItems:'center',gap:6,padding:'5px 10px',background:'transparent',color:P.dangerFg,border:'1px solid transparent',borderRadius:5,fontFamily:P.ui,fontSize:12,cursor:'pointer'}}>
                  <HIcon.Trash/> <span>Delete</span>
                </button>
                {selCount>0&&<>
                  <span style={{width:1,height:18,background:P.border,margin:'0 4px'}}/>
                  <span style={{fontSize:11,color:P.muted}}>{selCount} row{selCount>1?'s':''} selected</span>
                </>}
                {dirty.size>0&&<>
                  <span style={{width:1,height:18,background:P.border,margin:'0 4px'}}/>
                  <button onClick={()=>setDirty(new Set())} style={{display:'inline-flex',alignItems:'center',gap:6,padding:'5px 10px',background:'transparent',color:P.accent,border:`1px solid ${P.accent}`,borderRadius:5,fontFamily:P.ui,fontSize:12,cursor:'pointer',fontWeight:600}}>
                    Save changes
                  </button>
                </>}
              </div>
              <div style={{display:'flex',alignItems:'center',gap:10}}>
                <div style={{display:'inline-flex',alignItems:'center',fontSize:11,color:P.muted}}>
                  <span style={{display:'inline-flex',alignItems:'center',justifyContent:'center',width:22,height:22,cursor:'pointer'}}><HIcon.ArrowLeft/></span>
                  <span style={{padding:'0 10px',fontFamily:P.mono}}>1 / 1</span>
                  <span style={{display:'inline-flex',alignItems:'center',justifyContent:'center',width:22,height:22,cursor:'pointer'}}><HIcon.ArrowRight/></span>
                </div>
                <button style={{display:'inline-flex',alignItems:'center',gap:6,padding:'7px 14px',background:P.primaryBg,color:P.primaryFg,border:'none',borderRadius:6,fontFamily:P.ui,fontSize:12,fontWeight:600,cursor:'pointer',boxShadow:'0 2px 6px rgba(13,122,98,0.25)'}}>
                  <HIcon.Plus/> Insert Row
                </button>
              </div>
            </div>

            {/* Table + Detail */}
            <div style={{flex:1,display:'flex',minHeight:0}}>
              <div style={{flex:1,overflow:'auto'}}>
                <table style={{width:'100%',borderCollapse:'separate',borderSpacing:0,fontFamily:P.mono,fontSize:12}}>
                  <thead>
                    <tr>
                      <th style={{...s.thBase,width:36,textAlign:'center'}}>
                        <span onClick={toggleAll} style={{display:'inline-flex',alignItems:'center',justifyContent:'center',width:14,height:14,border:`1.5px solid ${selected.size===displayRows.length&&displayRows.length?P.checkActiveBg:P.checkBorder}`,background:selected.size===displayRows.length&&displayRows.length?P.checkActiveBg:'transparent',borderRadius:3,cursor:'pointer'}}>
                          {selected.size===displayRows.length&&displayRows.length>0&&<svg width="9" height="9" viewBox="0 0 16 16" fill="none"><path d="M3 8.5L6.5 12 13 4" stroke={P.checkFg} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"/></svg>}
                        </span>
                      </th>
                      <th style={{...s.thBase,width:40,textAlign:'right',paddingRight:14,color:P.label}}>#</th>
                      {COLS.map((col,i)=>(
                        <th key={i} style={s.thBase}>
                          <div style={{display:'flex',alignItems:'center',gap:5,cursor:'pointer',userSelect:'none'}} onClick={()=>toggleSort(col.name)}>
                            {col.pk&&<span style={{color:P.types.pk,display:'inline-flex'}}><HIcon.Key/></span>}
                            <span style={{color:sortCol===col.name?P.accent:P.text,fontWeight:600}}>{col.name}</span>
                            <span style={{fontSize:9,fontWeight:600,color:P.types[col.type]||P.muted,background:P.typeChipBg,padding:'1px 5px',borderRadius:3,letterSpacing:.3}}>{col.type}</span>
                            <span style={{marginLeft:'auto',color:sortCol===col.name?P.accent:P.muted,display:'inline-flex'}}>
                              {sortCol===col.name&&sortDir!=='none'?<HIcon.SortAsc/>:<HIcon.SortNone/>}
                            </span>
                          </div>
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {displayRows.map((row,ri)=>(
                      <tr key={ri}
                        onMouseEnter={()=>{setHoveredRow(ri);setDetailIdx(ri);}}
                        onMouseLeave={()=>setHoveredRow(null)}
                        style={{background:selected.has(ri)?P.rowSelected:hoveredRow===ri?P.rowHover:ri%2===1?P.rowStripe:'transparent',transition:'background .08s'}}>
                        <td style={{...s.tdBase,textAlign:'center',width:36}}>
                          <span onClick={()=>toggleRow(ri)}
                            style={{display:'inline-flex',alignItems:'center',justifyContent:'center',width:14,height:14,border:`1.5px solid ${selected.has(ri)?P.checkActiveBg:P.checkBorder}`,background:selected.has(ri)?P.checkActiveBg:'transparent',borderRadius:3,cursor:'pointer'}}>
                            {selected.has(ri)&&<svg width="9" height="9" viewBox="0 0 16 16" fill="none"><path d="M3 8.5L6.5 12 13 4" stroke={P.checkFg} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"/></svg>}
                          </span>
                        </td>
                        <td style={{...s.tdBase,textAlign:'right',paddingRight:14,color:P.label,fontSize:11,width:40}}>
                          {dirty.has(rows.indexOf(row))&&<span style={{display:'inline-block',width:6,height:6,borderRadius:'50%',background:P.dirtyDot,marginRight:5,verticalAlign:'middle'}}/>}
                          {ri+1}
                        </td>
                        {COLS.map((col,ci)=>{
                          const val=row[col.name];
                          const isEditing=editCell&&editCell.ri===ri&&editCell.name===col.name;
                          return (
                            <td key={ci} style={s.tdBase} onDoubleClick={()=>startEdit(ri,col.name,val)}>
                              <DataCell val={val} col={col} editing={isEditing} editVal={editVal}
                                onChange={setEditVal} onCommit={commitEdit} onCancel={()=>setEditCell(null)}
                                onTipEnter={e=>showTip(String(val),e)} onTipLeave={hideTip}/>
                            </td>
                          );
                        })}
                      </tr>
                    ))}
                    <tr>
                      <td colSpan={COLS.length+2} style={{padding:'12px 20px',color:P.muted,fontSize:12,fontStyle:'italic',fontFamily:P.ui,cursor:'pointer',borderBottom:`1px solid ${P.rowDivider}`}}>
                        <span style={{display:'inline-flex',alignItems:'center',gap:6}}><HIcon.Plus/> Add new row…</span>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>

              {/* Detail panel */}
              <div style={{width:300,borderLeft:`1px solid ${P.border}`,background:P.panelBg,display:'flex',flexDirection:'column'}}>
                <div style={{padding:'12px 16px 10px',borderBottom:`1px solid ${P.border}`,display:'flex',justifyContent:'space-between',alignItems:'flex-start'}}>
                  <div>
                    <div style={{fontSize:10,color:P.label,letterSpacing:1.2,textTransform:'uppercase',fontWeight:700}}>Row Detail</div>
                    <div style={{fontSize:13,fontWeight:600,marginTop:3}}>
                      id = <span style={{color:P.types.int,fontFamily:P.mono}}>{detailRow?.id}</span>
                    </div>
                  </div>
                  <div style={{display:'flex',gap:4}}>
                    {[<HIcon.Copy/>,<HIcon.Pencil/>].map((ic,i)=>(
                      <span key={i} style={{display:'inline-flex',alignItems:'center',justifyContent:'center',width:26,height:26,color:P.muted,border:`1px solid ${P.border}`,borderRadius:5,cursor:'pointer'}}>{ic}</span>
                    ))}
                  </div>
                </div>
                <div style={{flex:1,overflowY:'auto',padding:'6px 16px 16px'}}>
                  {detailRow&&COLS.map((col,ci)=>{
                    const val=detailRow[col.name];
                    return (
                      <div key={ci} style={{padding:'8px 0',borderBottom:`1px solid ${P.rowDivider}`,display:'grid',gridTemplateColumns:'88px 1fr',gap:10,alignItems:'start'}}>
                        <div style={{display:'flex',alignItems:'center',gap:4,paddingTop:2}}>
                          {col.pk&&<span style={{color:P.types.pk,display:'inline-flex'}}><HIcon.Key/></span>}
                          <span style={{fontSize:11,color:P.muted,fontFamily:P.mono}}>{col.name}</span>
                        </div>
                        <div style={{fontFamily:P.mono,fontSize:11.5,overflow:'hidden',textOverflow:'ellipsis',whiteSpace:col.type==='uuid'||col.type==='hash'?'nowrap':'normal'}}>
                          <DataCell val={val} col={col}/>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          </>)}

          {activeTab==='Query'&&<QueryTab table={activeTable}/>}
          {activeTab==='Schema'&&<SchemaTab/>}
          {activeTab==='Relations'&&(
            <div style={{flex:1,display:'flex',alignItems:'center',justifyContent:'center',color:P.muted,fontSize:13,background:P.canvasBg}}>
              No foreign key relationships defined for <strong style={{color:P.text,margin:'0 5px'}}>{activeTable}</strong>
            </div>
          )}

          {/* STATUS BAR */}
          <div style={{height:30,display:'flex',justifyContent:'space-between',alignItems:'center',padding:'0 18px',borderTop:`1px solid ${P.border}`,background:P.statusBg,color:P.muted,fontSize:11,fontFamily:P.mono}}>
            <div style={{display:'flex',alignItems:'center',gap:16}}>
              <span style={{display:'flex',alignItems:'center'}}><HIcon.Dot color={P.accent}/>SQL · public.{activeTable}</span>
              {dirty.size>0&&<span style={{display:'flex',alignItems:'center',color:P.dirtyDot}}><HIcon.Dot color={P.dirtyDot}/>{dirty.size} unsaved</span>}
            </div>
            <div style={{display:'flex',alignItems:'center',gap:16}}>
              {selCount>0&&<span>{selCount} selected</span>}
              <span>{rows.length} rows</span>
              <span>UTF-8</span>
              <span>8ms</span>
              <span>v0.3.0</span>
              <span style={{display:'flex',alignItems:'center'}}><HIcon.Dot color={P.successDot}/>local</span>
            </div>
          </div>
        </div>
      </div>

      {tooltip&&<TipBox text={tooltip.text} x={tooltip.x} y={tooltip.y}/>}
    </div>
  );
}

window.HarnessPaper = HarnessPaper;
