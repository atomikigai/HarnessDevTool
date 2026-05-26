// harness-agents.jsx — v2
// Layout: persistent session sidebar (left) + terminal main area (right)
// Depends on: harness-icons.jsx

const { useState, useRef, useEffect } = React;

// ─── THEME ────────────────────────────────────────────────────────────────────
const AP = {
  mono: "'JetBrains Mono','Fira Code',ui-monospace,Menlo,monospace",
  ui:   "'Inter',ui-sans-serif,system-ui,sans-serif",
  bg:      '#fdfcf8', panelBg:'#f5f3ed', canvasBg:'#fafaf5',
  border:  '#e2ded6', divider:'rgba(0,0,0,0.055)',
  text:    '#1c1a16', textSec:'#5c5850', muted:'#9e9890', label:'#b0a898',
  accent:  '#0d7a62', accentBg:'rgba(13,122,98,0.09)', accentRing:'rgba(13,122,98,0.18)',
  inputBg: '#ede9df', inputBorder:'#d8d4cc',
  active:  '#16a34a', activeBg:'rgba(22,163,74,0.10)', activeBorder:'rgba(22,163,74,0.25)',
  idle:    '#d08030', idleBg:'rgba(208,128,48,0.10)',   idleBorder:'rgba(208,128,48,0.25)',
  stopped: '#b0a898', stoppedBg:'rgba(176,168,152,0.08)', stoppedBorder:'rgba(176,168,152,0.18)',
  danger:  '#c53030',
  termBg:  '#0f1117', termText:'#e2e8f0', termPrompt:'#0d7a62', termMuted:'#4a5568', termBorder:'#1e2533',
};

const MODELS = {
  'claude-code':       { label:'claude-code', color:'#0d7a62', bg:'rgba(13,122,98,0.09)'   },
  'claude-3.5-sonnet': { label:'claude-3.5',  color:'#6b35b8', bg:'rgba(107,53,184,0.09)'  },
  'codex-cli':         { label:'codex-cli',   color:'#0f52c8', bg:'rgba(15,82,200,0.09)'   },
};

const STATUS_CFG = {
  active:  { color:AP.active,  bg:AP.activeBg,  border:AP.activeBorder,  label:'Active'  },
  idle:    { color:AP.idle,    bg:AP.idleBg,    border:AP.idleBorder,    label:'Idle'    },
  stopped: { color:AP.stopped, bg:AP.stoppedBg, border:AP.stoppedBorder, label:'Stopped' },
};

// ─── DATA ─────────────────────────────────────────────────────────────────────
const INIT_SESSIONS = [
  {
    id:'2aa3fcdb', name:'Fix auth middleware', model:'claude-code', status:'active',
    cwd:'/home/sara/projects/harness-backend', lastMsg:'All 8 tests pass.',
    uptime:'14m', tokens:12480,
    subAgents:[
      { id:'sa1', role:'Code Analyst',  status:'done',   action:'Parsed oauth.rs — found cache expiry bug on line 42', started:'12m ago' },
      { id:'sa2', role:'Patch Writer',  status:'done',   action:'Rewrote refresh_token() with expiry check + error variant', started:'9m ago' },
      { id:'sa3', role:'Test Runner',   status:'active', action:'Running cargo test auth — 8/8 passing', started:'3m ago' },
      { id:'sa4', role:'Doc Writer',    status:'idle',   action:'Waiting for test confirmation before updating docs', started:'1m ago' },
    ],
    tasks:[{done:true,text:'Read auth.rs'},{done:true,text:'Identify token expiry bug'},{done:false,text:'Write unit tests'},{done:false,text:'Update docs'}],
    log:[
      {t:'user', text:"can you fix the oauth token refresh logic? it's failing silently"},
      {t:'think', text:'Reading src/auth/oauth.rs and src/middleware/auth.rs…'},
      {t:'tool', tool:'read_file', args:'src/auth/oauth.rs', out:'fn refresh_token(token: &str) -> Result<Token> {\n  // BUG: not checking expiry before returning cached token\n  if let Some(t) = CACHE.get(token) {\n    return Ok(t.clone());\n  }\n  // ...\n}'},
      {t:'reply', text:'Found the bug — the cache lookup returns the token without checking if it\'s expired. Fixing now:'},
      {t:'tool', tool:'write_file', args:'src/auth/oauth.rs', out:'✓ Wrote 142 lines'},
      {t:'tool', tool:'bash', args:'cargo test auth', out:'running 8 tests\ntest auth::test_token_refresh ... ok\ntest auth::test_expired_token ... ok\ntest auth::test_invalid_token ... ok\n...\ntest result: ok. 8 passed; 0 failed'},
      {t:'reply', text:'All 8 tests pass. The fix is clean — no silent failures anymore.'},
      {t:'prompt'},
    ],
  },
  {
    id:'7e98b3ce', name:'DB migration script', model:'claude-code', status:'idle',
    cwd:'/home/sara/projects/harness-backend', lastMsg:'Migration created.',
    uptime:'32m', tokens:7320,
    subAgents:[
      { id:'sb1', role:'Schema Analyst', status:'done',   action:'Mapped existing users_profiles columns', started:'30m ago' },
      { id:'sb2', role:'Migration Writer', status:'done', action:'Created 20250526_004_users_profiles.sql', started:'25m ago' },
      { id:'sb3', role:'Rollback Tester', status:'idle',  action:'Verified DOWN migration — no data loss', started:'18m ago' },
    ],
    tasks:[{done:true,text:'Create migration file'},{done:true,text:'Test rollback'},{done:false,text:'Document schema changes'}],
    log:[
      {t:'user', text:'create a migration to add profile_picture_url and bio to users_profiles'},
      {t:'tool', tool:'bash', args:'ls migrations/', out:'20250526_001_init.sql\n20250526_002_add_oauth.sql\n20250526_003_add_passkeys.sql'},
      {t:'tool', tool:'write_file', args:'migrations/20250526_004_users_profiles.sql', out:'✓ Wrote 28 lines'},
      {t:'reply', text:'Migration created at `migrations/20250526_004_users_profiles.sql`. Adds `profile_picture_url TEXT NULL` and `bio TEXT NULL` with proper rollback.'},
      {t:'prompt'},
    ],
  },
  {
    id:'426db2b7', name:'API rate limiter', model:'claude-3.5-sonnet', status:'stopped',
    cwd:'/home/sara/projects/harness-backend', lastMsg:'Rate limiter implemented.',
    uptime:'1h 8m', tokens:31050,
    subAgents:[
      { id:'sc1', role:'Frontend Agent',  status:'done', action:'Added rate-limit banner in UI — shows 429 feedback', started:'1h ago' },
      { id:'sc2', role:'Backend Agent',   status:'done', action:'Implemented sliding window middleware in rate_limit.rs', started:'58m ago' },
      { id:'sc3', role:'QA Agent',        status:'done', action:'Ran 12 integration tests — all pass, 0 regressions', started:'40m ago' },
    ],
    tasks:[{done:true,text:'Research approach'},{done:true,text:'Implement middleware'},{done:true,text:'Write tests'}],
    log:[
      {t:'reply', text:'Rate limiter implemented using a sliding window algorithm backed by Redis. Middleware at `src/middleware/rate_limit.rs`. Applied to all `/api/*` routes with configurable limits per endpoint.'},
      {t:'prompt'},
    ],
  },
  {
    id:'76983899', name:'Untitled session', model:'codex-cli', status:'stopped',
    cwd:'/home/sara/', lastMsg:'', uptime:'2m', tokens:240,
    subAgents:[],
    tasks:[],
    log:[{t:'prompt'}],
  },
];

// ─── SMALL HELPERS ────────────────────────────────────────────────────────────
const timeAgo = s => ({active:'just now',idle:'4 min ago',stopped:'1h ago'}[s.status]||'');

function StatusDot({ status, pulse }) {
  const c = STATUS_CFG[status]?.color || AP.stopped;
  return (
    <span style={{ position:'relative', display:'inline-flex', alignItems:'center', justifyContent:'center', width:10, height:10, flexShrink:0 }}>
      <span style={{ width:8, height:8, borderRadius:'50%', background:c, display:'block' }} />
      {pulse && status==='active' && (
        <span style={{ position:'absolute', inset:-2, borderRadius:'50%', border:`1.5px solid ${c}`, opacity:.5, animation:'none' }} />
      )}
    </span>
  );
}

function ModelChip({ model, small }) {
  const m = MODELS[model] || MODELS['claude-code'];
  return (
    <span style={{ display:'inline-flex', alignItems:'center', padding: small ? '1px 6px' : '2px 8px', borderRadius:4, fontSize: small ? 10 : 11, fontWeight:600, background:m.bg, color:m.color, fontFamily:AP.mono, whiteSpace:'nowrap' }}>
      {m.label}
    </span>
  );
}

function TasksProgress({ tasks, inline }) {
  if (!tasks.length) return null;
  const done = tasks.filter(t=>t.done).length;
  const pct  = Math.round(done/tasks.length*100);
  return (
    <div style={{ display:'flex', alignItems:'center', gap:6 }}>
      <div style={{ flex:1, height:2.5, borderRadius:2, background:AP.inputBorder, overflow:'hidden' }}>
        <div style={{ width:`${pct}%`, height:'100%', borderRadius:2, background: pct===100 ? AP.active : AP.accent, transition:'width .3s' }} />
      </div>
      <span style={{ fontSize:10, color:AP.muted, fontFamily:AP.mono, whiteSpace:'nowrap' }}>{done}/{tasks.length}</span>
    </div>
  );
}

// ─── SESSION SIDEBAR ITEM ────────────────────────────────────────────────────
function SessionItem({ session, selected, onClick }) {
  const cfg = STATUS_CFG[session.status] || STATUS_CFG.stopped;
  return (
    <div onClick={onClick}
      style={{
        padding:'12px 14px', cursor:'pointer',
        borderLeft: selected ? `2px solid ${AP.accent}` : '2px solid transparent',
        background: selected ? AP.accentBg : 'transparent',
        transition:'background .1s',
      }}
      onMouseEnter={e=>{ if(!selected) e.currentTarget.style.background='rgba(0,0,0,0.025)'; }}
      onMouseLeave={e=>{ if(!selected) e.currentTarget.style.background='transparent'; }}
    >
      {/* Row 1: dot + name + time */}
      <div style={{ display:'flex', alignItems:'center', gap:8, marginBottom:6 }}>
        <StatusDot status={session.status} pulse />
        <span style={{ flex:1, fontWeight:selected?700:500, fontSize:13, color:selected?AP.accent:AP.text, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap', lineHeight:1 }}>
          {session.name}
        </span>
        <span style={{ fontSize:10, color:AP.muted, whiteSpace:'nowrap', flexShrink:0 }}>{timeAgo(session)}</span>
      </div>
      {/* Row 2: model + uptime */}
      <div style={{ display:'flex', alignItems:'center', gap:6, marginBottom:session.tasks.length?7:0 }}>
        <ModelChip model={session.model} small />
        <span style={{ fontSize:10, color:AP.muted, fontFamily:AP.mono }}>{session.uptime} · {(session.tokens/1000).toFixed(1)}k tok</span>
      </div>
      {/* Row 3: progress bar */}
      {session.tasks.length > 0 && <TasksProgress tasks={session.tasks} />}
    </div>
  );
}

// ─── SESSION SIDEBAR ──────────────────────────────────────────────────────────
function SessionSidebar({ sessions, selectedId, onSelect, onNew, collapsed, onToggle }) {
  const activeCount = sessions.filter(s=>s.status==='active').length;
  const idleCount   = sessions.filter(s=>s.status==='idle').length;

  return (
    <div style={{
      width: collapsed ? 48 : 272,
      minWidth: collapsed ? 48 : 272,
      background: AP.panelBg,
      borderRight: `1px solid ${AP.border}`,
      display: 'flex', flexDirection: 'column',
      flexShrink: 0, overflow: 'hidden',
      transition: 'width .2s cubic-bezier(.4,0,.2,1), min-width .2s cubic-bezier(.4,0,.2,1)',
    }}>

      {/* ── COLLAPSED VIEW ── */}
      {collapsed && (
        <div style={{ display:'flex', flexDirection:'column', alignItems:'center', paddingTop:10, gap:6 }}>
          {/* Expand toggle */}
          <button onClick={onToggle} title="Expand sessions"
            style={{ width:30, height:30, borderRadius:6, border:`1px solid ${AP.border}`, background:AP.bg, cursor:'pointer', display:'flex', alignItems:'center', justifyContent:'center', color:AP.muted, marginBottom:4 }}>
            <HIcon.ArrowRight size={10}/>
          </button>
          {/* New session */}
          <button onClick={onNew} title="New session"
            style={{ width:30, height:30, borderRadius:6, border:'none', background:AP.accent, cursor:'pointer', display:'flex', alignItems:'center', justifyContent:'center', color:'#fff', boxShadow:'0 1px 4px rgba(13,122,98,0.25)' }}>
            <HIcon.Plus size={12}/>
          </button>
          <div style={{ width:24, height:1, background:AP.border, margin:'4px 0' }}/>
          {/* Session dots */}
          {sessions.map(s=>(
            <button key={s.id} onClick={()=>onSelect(s.id)} title={s.name}
              style={{
                width:30, height:30, borderRadius:6, border:`1.5px solid ${selectedId===s.id ? AP.accent : 'transparent'}`,
                background: selectedId===s.id ? AP.accentBg : 'transparent',
                cursor:'pointer', display:'flex', alignItems:'center', justifyContent:'center',
              }}>
              <StatusDot status={s.status} pulse/>
            </button>
          ))}
        </div>
      )}

      {/* ── EXPANDED VIEW ── */}
      {!collapsed && (<>
        {/* Header */}
        <div style={{ padding:'14px 14px 10px', borderBottom:`1px solid ${AP.border}`, flexShrink:0 }}>
          <div style={{ display:'flex', alignItems:'center', justifyContent:'space-between', marginBottom:6 }}>
            <span style={{ fontSize:13, fontWeight:700, color:AP.text }}>Sessions</span>
            <div style={{ display:'flex', gap:6 }}>
              <button onClick={onNew} style={{ display:'inline-flex', alignItems:'center', gap:5, padding:'5px 10px', background:AP.accent, color:'#fff', border:'none', borderRadius:5, fontSize:11, fontWeight:600, cursor:'pointer', fontFamily:AP.ui }}>
                <HIcon.Plus size={11}/> New
              </button>
              {/* Collapse toggle */}
              <button onClick={onToggle} title="Collapse sidebar"
                style={{ width:28, height:28, borderRadius:5, border:`1px solid ${AP.border}`, background:'transparent', cursor:'pointer', display:'flex', alignItems:'center', justifyContent:'center', color:AP.muted }}>
                <HIcon.ArrowLeft size={10}/>
              </button>
            </div>
          </div>
          <div style={{ display:'flex', gap:6 }}>
            {activeCount>0 && <span style={{ fontSize:10, fontWeight:600, color:AP.active, background:AP.activeBg, border:`1px solid ${AP.activeBorder}`, padding:'2px 7px', borderRadius:999 }}>● {activeCount} active</span>}
            {idleCount>0   && <span style={{ fontSize:10, fontWeight:600, color:AP.idle,   background:AP.idleBg,   border:`1px solid ${AP.idleBorder}`,   padding:'2px 7px', borderRadius:999 }}>● {idleCount} idle</span>}
          </div>
        </div>

        {/* Session list */}
        <div style={{ flex:1, overflowY:'auto' }}>
          {sessions.map(s => (
            <div key={s.id}>
              <SessionItem session={s} selected={selectedId===s.id} onClick={()=>onSelect(s.id)} />
              <div style={{ height:1, background:AP.divider, margin:'0 14px' }} />
            </div>
          ))}
        </div>

        {/* Footer */}
        <div style={{ padding:'10px 14px', borderTop:`1px solid ${AP.border}`, fontSize:11, color:AP.accent, cursor:'pointer', display:'flex', alignItems:'center', gap:5, flexShrink:0 }}>
          <span>⚙</span> Agents registry →
        </div>
      </>)}
    </div>
  );
}

// ─── TERMINAL LINE ────────────────────────────────────────────────────────────
function TermLine({ entry, inputVal, onInput, onSubmit }) {
  if (entry.t==='prompt') return (
    <div style={{ display:'flex', alignItems:'center', gap:8, marginTop:4 }}>
      <span style={{ color:AP.termPrompt, fontWeight:700, userSelect:'none', flexShrink:0 }}>❯</span>
      <input value={inputVal} onChange={e=>onInput(e.target.value)}
        onKeyDown={e=>{ if(e.key==='Enter'&&inputVal.trim()) onSubmit(); }}
        placeholder="Message or command…"
        style={{ flex:1, background:'transparent', border:'none', outline:'none', color:AP.termText, fontFamily:AP.mono, fontSize:13, caretColor:AP.termPrompt, minWidth:0 }} />
    </div>
  );
  if (entry.t==='user') return (
    <div style={{ display:'flex', gap:10, padding:'6px 0' }}>
      <span style={{ color:AP.termPrompt, fontWeight:700, userSelect:'none', flexShrink:0 }}>❯</span>
      <span style={{ color:'#f8fafc', fontWeight:500 }}>{entry.text}</span>
    </div>
  );
  if (entry.t==='think') return (
    <div style={{ display:'flex', gap:10, padding:'2px 0', opacity:.6 }}>
      <span style={{ color:AP.termMuted, fontFamily:AP.mono, fontSize:12, flexShrink:0 }}>···</span>
      <span style={{ color:AP.termMuted, fontSize:12, fontStyle:'italic' }}>{entry.text}</span>
    </div>
  );
  if (entry.t==='tool') return (
    <div style={{ margin:'5px 0', borderLeft:`2px solid ${AP.termBorder}`, paddingLeft:12 }}>
      <div style={{ display:'flex', gap:8, alignItems:'center', marginBottom:3 }}>
        <span style={{ fontSize:10, fontWeight:700, color:'#7dd3fc', fontFamily:AP.mono, letterSpacing:.3 }}>{entry.tool}</span>
        <span style={{ fontSize:11, color:AP.termMuted, fontFamily:AP.mono, overflow:'hidden', textOverflow:'ellipsis' }}>{entry.args}</span>
      </div>
      <pre style={{ margin:0, color:'#94a3b8', fontSize:11.5, fontFamily:AP.mono, lineHeight:1.65, whiteSpace:'pre-wrap', wordBreak:'break-all' }}>{entry.out}</pre>
    </div>
  );
  if (entry.t==='reply') return (
    <div style={{ paddingLeft:12, borderLeft:`2px solid ${AP.accent}`, margin:'6px 0', padding:'6px 0 6px 12px' }}>
      <span style={{ color:AP.termText, lineHeight:1.75, fontSize:13 }}>{entry.text}</span>
    </div>
  );
  return null;
}

// ─── TERMINAL PANEL ───────────────────────────────────────────────────────────
// ─── FILE HELPERS ────────────────────────────────────────────────────────────
const FILE_ICONS = {
  image:  { label:'IMG', color:'#7b3fbf' },
  code:   { label:'{ }', color:'#0f52c8' },
  pdf:    { label:'PDF', color:'#c53030' },
  text:   { label:'TXT', color:'#0d7a62' },
  other:  { label:'FILE', color:'#9e9890' },
};
const CODE_EXTS = ['rs','ts','tsx','js','jsx','py','go','rb','sh','toml','json','yaml','yml','md','sql','css','html'];
function fileKind(file) {
  if (file.type.startsWith('image/')) return 'image';
  if (file.type === 'application/pdf') return 'pdf';
  const ext = file.name.split('.').pop().toLowerCase();
  if (CODE_EXTS.includes(ext)) return 'code';
  if (file.type.startsWith('text/')) return 'text';
  return 'other';
}
function FileChip({ att, onRemove }) {
  const cfg = FILE_ICONS[att.kind] || FILE_ICONS.other;
  return (
    <div style={{ display:'inline-flex', alignItems:'center', gap:7, padding:'5px 9px 5px 7px', borderRadius:6, background:'rgba(255,255,255,0.07)', border:'1px solid rgba(255,255,255,0.12)', maxWidth:200, flexShrink:0 }}>
      {att.kind==='image' && att.preview
        ? <img src={att.preview} style={{ width:22, height:22, borderRadius:3, objectFit:'cover', flexShrink:0 }} />
        : <span style={{ fontSize:10, fontWeight:700, color:cfg.color, background:'rgba(0,0,0,0.3)', padding:'2px 5px', borderRadius:3, flexShrink:0, fontFamily:AP.mono }}>{cfg.label}</span>
      }
      <div style={{ minWidth:0 }}>
        <div style={{ fontSize:11, color:'#e2e8f0', fontFamily:AP.mono, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap' }}>{att.name}</div>
        <div style={{ fontSize:9.5, color:'#4a5568' }}>{(att.size/1024).toFixed(1)} KB</div>
      </div>
      <button onClick={onRemove} style={{ background:'none', border:'none', color:'#4a5568', cursor:'pointer', fontSize:15, lineHeight:1, padding:'0 2px', flexShrink:0 }}>×</button>
    </div>
  );
}

function TerminalPanel({ session, onUpdateSession }) {
  const [inputVal,    setInputVal]    = useState('');
  const [taskTab,     setTaskTab]     = useState('Tasks');
  const [attachments, setAttachments] = useState([]);
  const [dragging,    setDragging]    = useState(false);
  const logRef  = useRef(null);
  const fileRef = useRef(null);
  const dragCnt = useRef(0);

  useEffect(()=>{
    if(logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  },[session?.log]);

  if (!session) return (
    <div style={{ flex:1, display:'flex', flexDirection:'column', alignItems:'center', justifyContent:'center', gap:16, background:AP.canvasBg, color:AP.muted }}>
      <div style={{ fontSize:32, opacity:.3 }}>⌨</div>
      <div style={{ fontSize:15, fontWeight:700, color:AP.text, opacity:.35 }}>Select a session</div>
      <div style={{ fontSize:12, opacity:.4 }}>or create a new one →</div>
    </div>
  );

  const doneTasks = session.tasks.filter(t=>t.done).length;

  function processFiles(fileList) {
    Array.from(fileList).forEach(file => {
      const kind = fileKind(file);
      const base = { name:file.name, size:file.size, kind, preview:null };
      if (kind==='image') {
        const reader = new FileReader();
        reader.onload = e => setAttachments(prev=>[...prev, {...base, preview:e.target.result}]);
        reader.readAsDataURL(file);
      } else {
        setAttachments(prev=>[...prev, base]);
      }
    });
  }

  function onDragEnter(e) { e.preventDefault(); dragCnt.current++; setDragging(true); }
  function onDragLeave(e) { e.preventDefault(); dragCnt.current--; if(dragCnt.current===0) setDragging(false); }
  function onDragOver(e)  { e.preventDefault(); }
  function onDrop(e)      { e.preventDefault(); dragCnt.current=0; setDragging(false); processFiles(e.dataTransfer.files); }

  function submit() {
    if (!inputVal.trim() && !attachments.length) return;
    const fileDesc = attachments.map(a=>`[${a.kind}: ${a.name}]`).join(' ');
    const fullText = [fileDesc, inputVal.trim()].filter(Boolean).join(' — ');
    onUpdateSession(session.id, {
      log: [
        ...session.log.filter(e=>e.t!=='prompt'),
        {t:'user', text:fullText, attachments:[...attachments]},
        {t:'think', text: attachments.length ? `Reading ${attachments.map(a=>a.name).join(', ')}…` : 'Processing…'},
        {t:'reply', text: attachments.length
          ? `Received ${attachments.length} file${attachments.length>1?'s':''}: ${attachments.map(a=>a.name).join(', ')}. ${inputVal.trim()?`Working on: "${inputVal}".`:''} (Prototype — real file processing would happen in the PTY.)`
          : `Working on: "${inputVal}". (Prototype — real responses from the PTY would appear here.)`
        },
        {t:'prompt'},
      ],
      tokens: session.tokens + fullText.length * 4,
    });
    setInputVal('');
    setAttachments([]);
  }

  return (
    <div style={{ flex:1, display:'flex', flexDirection:'column', minHeight:0 }}>
      {/* Session header bar */}
      <div style={{ height:48, display:'flex', alignItems:'center', gap:14, padding:'0 20px', borderBottom:`1px solid ${AP.border}`, background:AP.bg, flexShrink:0 }}>
        <StatusDot status={session.status} pulse />
        <div style={{ flex:1, minWidth:0 }}>
          <div style={{ display:'flex', alignItems:'center', gap:10 }}>
            <span style={{ fontSize:14, fontWeight:700, color:AP.text, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap' }}>{session.name}</span>
            <ModelChip model={session.model} />
          </div>
        </div>
        <div style={{ display:'flex', alignItems:'center', gap:12, flexShrink:0 }}>
          <span style={{ fontSize:11, color:AP.muted, fontFamily:AP.mono }}>{session.uptime} · {session.tokens.toLocaleString()} tok</span>
          <div style={{ display:'flex', gap:6 }}>
            <button style={{ padding:'5px 12px', borderRadius:5, fontSize:11, fontWeight:600, border:`1px solid ${AP.danger}`, background:'transparent', color:AP.danger, cursor:'pointer', fontFamily:AP.ui }}>Stop</button>
            <button style={{ padding:'5px 12px', borderRadius:5, fontSize:11, fontWeight:600, border:`1px solid ${AP.border}`, background:'transparent', color:AP.textSec, cursor:'pointer', fontFamily:AP.ui }}>Restart</button>
          </div>
        </div>
      </div>

      {/* Terminal + side panel */}
      <div style={{ flex:1, display:'flex', minHeight:0 }}>

        {/* ── TERMINAL ── */}
        <div
          onDragEnter={onDragEnter} onDragLeave={onDragLeave} onDragOver={onDragOver} onDrop={onDrop}
          style={{ flex:1, background:AP.termBg, display:'flex', flexDirection:'column', minWidth:0, minHeight:0, position:'relative' }}>

          {/* Drop overlay */}
          {dragging && (
            <div style={{ position:'absolute', inset:0, zIndex:50, background:'rgba(13,122,98,0.18)', border:`2px dashed ${AP.accent}`, borderRadius:0, display:'flex', flexDirection:'column', alignItems:'center', justifyContent:'center', gap:12, pointerEvents:'none' }}>
              <div style={{ fontSize:32 }}>📂</div>
              <div style={{ fontSize:15, fontWeight:700, color:AP.accent }}>Drop files to attach</div>
              <div style={{ fontSize:12, color:AP.termMuted }}>Images, code, PDFs…</div>
            </div>
          )}

          {/* Term chrome */}
          <div style={{ height:32, display:'flex', alignItems:'center', justifyContent:'space-between', padding:'0 14px', borderBottom:`1px solid ${AP.termBorder}`, background:'rgba(0,0,0,0.3)', flexShrink:0 }}>
            <div style={{ display:'flex', gap:5 }}>
              {['#ed6a5e','#f4bf4f','#61c554'].map((c,i)=>(
                <span key={i} style={{ width:9, height:9, borderRadius:'50%', background:c }} />
              ))}
            </div>
            <span style={{ color:AP.termMuted, fontSize:10.5, fontFamily:AP.mono }}>{session.model} · {session.cwd}</span>
            <span style={{ fontSize:10, color:AP.termMuted, fontFamily:AP.mono }}>{session.tokens.toLocaleString()} tokens</span>
          </div>

          {/* Output */}
          <div ref={logRef} style={{ flex:1, overflow:'auto', padding:'14px 18px', fontFamily:AP.mono, fontSize:13, lineHeight:1.65, display:'flex', flexDirection:'column', gap:2 }}>
            <div style={{ marginBottom:10, paddingBottom:10, borderBottom:`1px solid ${AP.termBorder}`, display:'flex', alignItems:'center', gap:10 }}>
              <span style={{ color:AP.termPrompt, fontWeight:700 }}>Harness</span>
              <span style={{ color:AP.termMuted, fontSize:11 }}>{session.model} · {session.cwd}</span>
            </div>
            {session.log.map((entry,i)=>(
              <TermLine key={i} entry={entry} inputVal={inputVal} onInput={setInputVal} onSubmit={submit} />
            ))}
          </div>

          {/* ── File chips + prompt bar ── */}
          <div style={{ borderTop:`1px solid ${AP.termBorder}`, background:'rgba(0,0,0,0.25)', flexShrink:0 }}>
            {attachments.length > 0 && (
              <div style={{ display:'flex', flexWrap:'wrap', gap:6, padding:'8px 14px 0' }}>
                {attachments.map((att,i)=>(
                  <FileChip key={i} att={att} onRemove={()=>setAttachments(prev=>prev.filter((_,idx)=>idx!==i))} />
                ))}
              </div>
            )}
            <div style={{ display:'flex', alignItems:'center', gap:8, padding:'10px 14px' }}>
              {/* Hidden file input */}
              <input ref={fileRef} type="file" multiple style={{ display:'none' }}
                onChange={e=>{ processFiles(e.target.files); e.target.value=''; }} />
              {/* Attach button */}
              <button onClick={()=>fileRef.current?.click()}
                title="Attach files (or drag & drop)"
                style={{ width:30, height:30, borderRadius:6, border:`1px solid rgba(255,255,255,0.1)`, background:'rgba(255,255,255,0.06)', cursor:'pointer', display:'flex', alignItems:'center', justifyContent:'center', color:'#64748b', flexShrink:0, fontSize:15 }}>
                📎
              </button>
              {/* Prompt */}
              <div style={{ display:'flex', alignItems:'center', gap:8, flex:1, background:'rgba(255,255,255,0.04)', border:`1px solid rgba(255,255,255,0.08)`, borderRadius:6, padding:'6px 12px' }}>
                <span style={{ color:AP.termPrompt, fontWeight:700, userSelect:'none' }}>❯</span>
                <input value={inputVal} onChange={e=>setInputVal(e.target.value)}
                  onKeyDown={e=>{ if(e.key==='Enter') submit(); }}
                  placeholder={attachments.length ? `Add a message (or press Enter to send ${attachments.length} file${attachments.length>1?'s':''})…` : 'Message or command…'}
                  style={{ flex:1, background:'transparent', border:'none', outline:'none', color:AP.termText, fontFamily:AP.mono, fontSize:13, caretColor:AP.termPrompt, minWidth:0 }} />
                {(inputVal.trim()||attachments.length)>0 && (
                  <button onClick={submit} style={{ background:AP.accent, border:'none', borderRadius:4, color:'#fff', cursor:'pointer', padding:'3px 10px', fontSize:11, fontWeight:700, fontFamily:AP.ui, flexShrink:0 }}>Send</button>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* ── RIGHT PANEL: Tasks / Agents / Info ── */}
        <div style={{ width:280, borderLeft:`1px solid ${AP.border}`, display:'flex', flexDirection:'column', background:AP.panelBg, flexShrink:0 }}>
          {/* Tab strip */}
          <div style={{ display:'flex', gap:2, padding:8, borderBottom:`1px solid ${AP.border}`, background:AP.bg, flexShrink:0 }}>
            {['Tasks','Agents','Info'].map(tab=>(
              <button key={tab} onClick={()=>setTaskTab(tab)} style={{
                flex:1, padding:'5px 0', border:'none', borderRadius:5,
                background:taskTab===tab ? AP.panelBg : 'transparent',
                color:taskTab===tab ? AP.accent : AP.muted,
                fontWeight:taskTab===tab ? 600 : 400, cursor:'pointer',
                fontSize:11, fontFamily:AP.ui,
                boxShadow:taskTab===tab ? '0 1px 3px rgba(0,0,0,0.07)' : 'none',
              }}>
                {tab}{tab==='Tasks'&&session.tasks.length ? ` ${doneTasks}/${session.tasks.length}` : ''}
              </button>
            ))}
          </div>

          {/* TASKS */}
          {taskTab==='Tasks' && (
            <div style={{ flex:1, overflow:'auto', padding:12, display:'flex', flexDirection:'column', gap:7 }}>
              {session.tasks.length===0
                ? <div style={{ textAlign:'center', color:AP.muted, fontSize:12, paddingTop:24 }}>No tasks</div>
                : session.tasks.map((task,i)=>(
                  <div key={i} style={{ display:'flex', alignItems:'flex-start', gap:10, padding:'9px 11px', borderRadius:6, border:`1px solid ${task.done?AP.accentRing:AP.border}`, background:task.done?AP.accentBg:'transparent' }}>
                    <span style={{ display:'inline-flex', alignItems:'center', justifyContent:'center', width:15, height:15, borderRadius:4, flexShrink:0, marginTop:1, background:task.done?AP.accent:'transparent', border:`1.5px solid ${task.done?AP.accent:AP.inputBorder}` }}>
                      {task.done && <svg width="9" height="9" viewBox="0 0 16 16" fill="none"><path d="M3 8.5L6.5 12 13 4" stroke="#fff" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"/></svg>}
                    </span>
                    <span style={{ fontSize:12, color:task.done?AP.textSec:AP.text, textDecoration:task.done?'line-through':'none', lineHeight:1.5 }}>{task.text}</span>
                  </div>
                ))
              }
            </div>
          )}

          {/* AGENTS — sub-agents spawned by this session's main agent */}
          {taskTab==='Agents' && (
            <div style={{ flex:1, overflow:'auto', padding:12, display:'flex', flexDirection:'column', gap:8 }}>
              {(!session.subAgents || session.subAgents.length === 0) ? (
                <div style={{ textAlign:'center', color:AP.muted, fontSize:12, paddingTop:32, lineHeight:1.8 }}>
                  <div style={{ fontSize:22, marginBottom:8, opacity:.3 }}>⟳</div>
                  No sub-agents spawned yet.<br/>
                  <span style={{ fontSize:11 }}>They appear when the main agent<br/>delegates tasks in parallel.</span>
                </div>
              ) : (<>
                <div style={{ fontSize:10, letterSpacing:1.2, color:AP.label, textTransform:'uppercase', fontWeight:700, marginBottom:2 }}>
                  Sub-agents · {session.subAgents.length} spawned
                </div>
                {session.subAgents.map((ag, i) => {
                  const cfg = STATUS_CFG[ag.status] || STATUS_CFG.stopped;
                  return (
                    <div key={ag.id} style={{ borderRadius:8, border:`1px solid ${ag.status==='active'?cfg.border:AP.border}`, background:ag.status==='active'?cfg.bg:AP.bg, padding:'10px 12px', position:'relative', overflow:'hidden' }}>
                      {/* Left status stripe */}
                      <div style={{ position:'absolute', top:0, left:0, width:3, height:'100%', background:cfg.color, borderRadius:'8px 0 0 8px' }}/>
                      <div style={{ paddingLeft:8 }}>
                        {/* Role + status badge */}
                        <div style={{ display:'flex', alignItems:'center', gap:6, marginBottom:5 }}>
                          <span style={{ width:7, height:7, borderRadius:'50%', background:cfg.color, flexShrink:0, boxShadow: ag.status==='active'?`0 0 0 3px ${cfg.color}22`:'none' }}/>
                          <span style={{ fontSize:12.5, fontWeight:700, color:ag.status==='active'?cfg.color:AP.text, flex:1 }}>{ag.role}</span>
                          <span style={{ fontSize:9.5, fontWeight:700, color:cfg.color, background:cfg.bg, border:`1px solid ${cfg.border}`, padding:'1px 6px', borderRadius:3, whiteSpace:'nowrap' }}>
                            {cfg.label}
                          </span>
                        </div>
                        {/* Current action */}
                        <div style={{ fontSize:11, color:ag.status==='active'?AP.textSec:AP.muted, lineHeight:1.55, marginBottom:5 }}>
                          {ag.status==='active' && <span style={{ color:AP.accent, marginRight:5 }}>▶</span>}
                          {ag.action}
                        </div>
                        {/* Started */}
                        <div style={{ fontSize:10, color:AP.label, fontFamily:AP.mono }}>started {ag.started}</div>
                      </div>
                    </div>
                  );
                })}
              </>)}
            </div>
          )}

          {/* INFO */}
          {taskTab==='Info' && (
            <div style={{ flex:1, overflow:'auto', padding:12 }}>
              {[
                ['Session ID', session.id],
                ['Model',      session.model],
                ['Status',     session.status],
                ['Directory',  session.cwd],
                ['Uptime',     session.uptime],
                ['Tokens',     session.tokens.toLocaleString()],
              ].map(([label,val],i)=>(
                <div key={i} style={{ padding:'8px 0', borderBottom:`1px solid ${AP.divider}`, display:'grid', gridTemplateColumns:'80px 1fr', gap:8 }}>
                  <span style={{ fontSize:11, color:AP.muted, fontFamily:AP.mono }}>{label}</span>
                  <span style={{ fontSize:11, color:AP.text, fontFamily:AP.mono, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap' }}>{val}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── BACKEND HEALTH PANEL (shown when no session selected) ────────────────────
function BackendPanel({ onNew }) {
  const [ms, setMs] = useState(12);
  useEffect(()=>{ const t=setInterval(()=>setMs(8+Math.floor(Math.random()*18)),4000); return()=>clearInterval(t); },[]);
  const ok = ms < 30;
  return (
    <div style={{ flex:1, display:'flex', flexDirection:'column', alignItems:'center', justifyContent:'center', gap:28, background:AP.canvasBg, padding:40 }}>
      {/* Empty state */}
      <div style={{ textAlign:'center', marginBottom:8 }}>
        <div style={{ fontSize:28, marginBottom:8, opacity:.25 }}>⌨</div>
        <div style={{ fontSize:16, fontWeight:700, color:AP.text, opacity:.5 }}>Select a session to open the terminal</div>
        <div style={{ fontSize:12, color:AP.muted, marginTop:4 }}>or create a new one</div>
        <button onClick={onNew} style={{ marginTop:14, display:'inline-flex', alignItems:'center', gap:8, padding:'9px 20px', background:AP.accent, color:'#fff', border:'none', borderRadius:7, fontSize:13, fontWeight:600, cursor:'pointer', fontFamily:'Inter,sans-serif', boxShadow:'0 2px 8px rgba(13,122,98,0.25)' }}>
          <HIcon.Plus size={13}/> New session
        </button>
      </div>

      {/* Backend health card */}
      <div style={{ width:'100%', maxWidth:560, background:AP.bg, border:`1px solid ${AP.border}`, borderRadius:10, overflow:'hidden' }}>
        <div style={{ padding:'14px 18px', borderBottom:`1px solid ${AP.border}`, display:'flex', alignItems:'center', gap:10 }}>
          <span style={{ color:AP.accent, display:'inline-flex' }}><HIcon.Sparkles size={14}/></span>
          <span style={{ fontWeight:700, fontSize:14, color:AP.text }}>Backend</span>
          <span style={{ marginLeft:'auto', display:'inline-flex', alignItems:'center', gap:5, fontSize:11, fontWeight:600, color:ok?AP.active:AP.danger, background:ok?AP.activeBg:'rgba(197,48,48,0.08)', padding:'3px 10px', borderRadius:999, border:`1px solid ${ok?AP.activeBorder:'rgba(197,48,48,0.2)'}` }}>
            <span style={{ width:5, height:5, borderRadius:'50%', background:ok?AP.active:AP.danger }} />
            {ok?'Healthy':'Degraded'}
          </span>
        </div>
        <div style={{ padding:'16px 18px', display:'grid', gridTemplateColumns:'repeat(4,1fr)', gap:16 }}>
          {[
            { label:'Version',  val:'0.0.1',   sub:'backend'        },
            { label:'Uptime',   val:'317s',    sub:'since restart'  },
            { label:'Latency',  val:`${ms}ms`, sub:'/api/health', accent:ok?AP.active:AP.danger },
            { label:'Sessions', val:'4',       sub:'2 active'       },
          ].map(({label,val,sub,accent},i)=>(
            <div key={i}>
              <div style={{ fontSize:9.5, letterSpacing:1.3, color:AP.label, textTransform:'uppercase', fontWeight:700, marginBottom:5 }}>{label}</div>
              <div style={{ fontSize:20, fontWeight:800, color:accent||AP.text, fontFamily:AP.mono, letterSpacing:-1 }}>{val}</div>
              <div style={{ fontSize:10.5, color:AP.muted, marginTop:2 }}>{sub}</div>
            </div>
          ))}
        </div>
        <div style={{ padding:'8px 18px', borderTop:`1px solid ${AP.border}`, fontSize:11, color:AP.muted, fontFamily:AP.mono, display:'flex', justifyContent:'space-between' }}>
          <span>GET /api/health · every 10s</span>
          <span>Updated 5:01:37 PM</span>
        </div>
      </div>
    </div>
  );
}

// ─── NEW SESSION DRAWER ───────────────────────────────────────────────────────
function NewSessionDrawer({ onClose, onCreate }) {
  const [name,  setName]  = useState('');
  const [model, setModel] = useState('claude-code');
  const [cwd,   setCwd]   = useState('/home/sara/projects/harness-backend');
  return (
    <div style={{ position:'absolute', inset:0, zIndex:100, display:'flex', justifyContent:'flex-end' }}>
      <div onClick={onClose} style={{ position:'absolute', inset:0, background:'rgba(28,26,22,0.22)' }} />
      <div style={{ position:'relative', width:400, background:AP.bg, borderLeft:`1px solid ${AP.border}`, display:'flex', flexDirection:'column', boxShadow:'-8px 0 32px rgba(0,0,0,0.10)' }}>
        <div style={{ padding:'18px 20px', borderBottom:`1px solid ${AP.border}` }}>
          <div style={{ fontSize:15, fontWeight:700, color:AP.text }}>New Session</div>
          <div style={{ fontSize:12, color:AP.muted, marginTop:2 }}>Launch a CLI inside a managed PTY</div>
        </div>
        <div style={{ flex:1, overflow:'auto', padding:'18px 20px', display:'flex', flexDirection:'column', gap:18 }}>
          <div>
            <label style={{ display:'block', fontSize:10.5, fontWeight:700, color:AP.label, letterSpacing:1.1, textTransform:'uppercase', marginBottom:6 }}>Session name</label>
            <input value={name} onChange={e=>setName(e.target.value)} placeholder="e.g. Fix auth middleware"
              style={{ width:'100%', padding:'9px 12px', borderRadius:6, border:`1px solid ${AP.inputBorder}`, background:AP.inputBg, color:AP.text, fontSize:13, fontFamily:AP.ui, outline:'none' }} />
          </div>
          <div>
            <label style={{ display:'block', fontSize:10.5, fontWeight:700, color:AP.label, letterSpacing:1.1, textTransform:'uppercase', marginBottom:6 }}>Model / CLI</label>
            <div style={{ display:'flex', flexDirection:'column', gap:7 }}>
              {Object.entries(MODELS).map(([key,m])=>(
                <div key={key} onClick={()=>setModel(key)} style={{ display:'flex', alignItems:'center', gap:12, padding:'10px 13px', borderRadius:7, border:`1.5px solid ${model===key?m.color:AP.border}`, background:model===key?m.bg:'transparent', cursor:'pointer' }}>
                  <span style={{ width:10, height:10, borderRadius:'50%', background:model===key?m.color:AP.inputBorder, border:`2px solid ${model===key?m.color:AP.inputBorder}`, flexShrink:0 }} />
                  <div>
                    <div style={{ fontSize:13, fontWeight:600, color:model===key?m.color:AP.text, fontFamily:AP.mono }}>{m.label}</div>
                    <div style={{ fontSize:11, color:AP.muted, marginTop:1 }}>
                      {key==='claude-code'?'Anthropic · code & tasks':key==='claude-3.5-sonnet'?'Anthropic · reasoning':'OpenAI · code generation'}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
          <div>
            <label style={{ display:'block', fontSize:10.5, fontWeight:700, color:AP.label, letterSpacing:1.1, textTransform:'uppercase', marginBottom:6 }}>Working directory</label>
            <input value={cwd} onChange={e=>setCwd(e.target.value)}
              style={{ width:'100%', padding:'9px 12px', borderRadius:6, border:`1px solid ${AP.inputBorder}`, background:AP.inputBg, color:AP.text, fontSize:12, fontFamily:AP.mono, outline:'none' }} />
          </div>
        </div>
        <div style={{ padding:'14px 20px', borderTop:`1px solid ${AP.border}`, display:'flex', gap:8, justifyContent:'flex-end' }}>
          <button onClick={onClose} style={{ padding:'8px 14px', borderRadius:6, border:`1px solid ${AP.border}`, background:'transparent', color:AP.textSec, cursor:'pointer', fontSize:13, fontFamily:AP.ui }}>Cancel</button>
          <button onClick={()=>{ onCreate({name:name||'Untitled session',model,cwd,status:'active',tasks:[],log:[{t:'prompt'}],uptime:'0m',tokens:0}); onClose(); }}
            style={{ padding:'8px 18px', borderRadius:6, border:'none', background:AP.accent, color:'#fff', cursor:'pointer', fontSize:13, fontFamily:AP.ui, fontWeight:600, boxShadow:'0 2px 8px rgba(13,122,98,0.22)' }}>
            Launch session
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── ROOT ─────────────────────────────────────────────────────────────────────
function AgentsView() {
  const [sessions,   setSessions]   = useState(INIT_SESSIONS);
  const [selectedId, setSelectedId] = useState('2aa3fcdb');
  const [showNew,    setShowNew]    = useState(false);
  const [collapsed,  setCollapsed]  = useState(false);

  const selected     = sessions.find(s=>s.id===selectedId) || null;
  const sidebarWidth = collapsed ? 48 : 272;

  // Expose sessions globally so TerminalPanel's Agents tab can read them
  useEffect(()=>{ window.__harnessSessions = sessions; }, [sessions]);

  function updateSession(id, patch) {
    setSessions(prev => prev.map(s => s.id===id ? {...s,...patch} : s));
  }

  function createSession(data) {
    const id = Math.random().toString(36).slice(2,10);
    setSessions(prev => [{ id, lastMsg:'', ...data }, ...prev]);
    setSelectedId(id);
  }

  return (
    <div style={{ flex:1, display:'flex', flexDirection:'column', minHeight:0, position:'relative' }}>
      {/* Sub-header — follows sidebar width with transition */}
      <div style={{
        height:48, display:'flex', alignItems:'center', justifyContent:'space-between',
        borderBottom:`1px solid ${AP.border}`, background:AP.bg, flexShrink:0,
        paddingLeft: sidebarWidth + 24,
        paddingRight: 24,
        transition:'padding-left .2s cubic-bezier(.4,0,.2,1)',
      }}>
        <div>
          <span style={{ fontSize:15, fontWeight:800, color:AP.text }}>Dashboard</span>
          <span style={{ fontSize:12, color:AP.muted, marginLeft:10 }}>Backend health, active sessions, and shell wiring.</span>
        </div>
        <div style={{ display:'flex', gap:8 }}>
          <button style={{ display:'inline-flex', alignItems:'center', gap:6, padding:'5px 12px', fontSize:12, border:`1px solid ${AP.border}`, borderRadius:5, background:'transparent', color:AP.textSec, cursor:'pointer', fontFamily:AP.ui }}>
            <span style={{ fontSize:9, color:AP.accent }}>◉</span> Protocol v1.0
          </button>
          <button style={{ display:'inline-flex', alignItems:'center', gap:6, padding:'5px 12px', fontSize:12, border:`1px solid ${AP.border}`, borderRadius:5, background:'transparent', color:AP.textSec, cursor:'pointer', fontFamily:AP.ui }}>
            ↺ Refresh
          </button>
        </div>
      </div>

      {/* Body */}
      <div style={{ display:'flex', flex:1, minHeight:0 }}>
        <SessionSidebar
          sessions={sessions}
          selectedId={selectedId}
          onSelect={setSelectedId}
          onNew={()=>setShowNew(true)}
          collapsed={collapsed}
          onToggle={()=>setCollapsed(c=>!c)}
        />
        {selected
          ? <TerminalPanel session={selected} onUpdateSession={updateSession} />
          : <BackendPanel onNew={()=>setShowNew(true)} />
        }
      </div>

      {showNew && <NewSessionDrawer onClose={()=>setShowNew(false)} onCreate={createSession} />}
    </div>
  );
}

window.AgentsView = AgentsView;
