// harness-ssh.jsx — SSH File Manager + Terminal
// Paper theme. Depends on harness-icons.jsx

const { useState, useRef, useEffect, useCallback } = React;

// ─── THEME ────────────────────────────────────────────────────────────────────
const SP = {
  mono: "'JetBrains Mono','Fira Code',ui-monospace,Menlo,monospace",
  ui:   "'Inter',ui-sans-serif,system-ui,sans-serif",
  bg:      '#fdfcf8', panelBg:'#f5f3ed', canvasBg:'#ffffff',
  border:  '#e2ded6', divider:'rgba(0,0,0,0.055)',
  text:    '#1c1a16', textSec:'#5c5850', muted:'#9e9890', label:'#b0a898',
  accent:  '#0d7a62', accentBg:'rgba(13,122,98,0.09)', accentRing:'rgba(13,122,98,0.18)',
  inputBg: '#ede9df', inputBorder:'#d8d4cc',
  rowHover:'rgba(13,122,98,0.05)', rowSelected:'rgba(13,122,98,0.09)',
  danger:  '#c53030', dangerBg:'rgba(197,48,48,0.08)',
  warn:    '#d08030', warnBg:'rgba(208,128,48,0.09)',
  success: '#16a34a', successBg:'rgba(22,163,74,0.09)',
  termBg:  '#0f1117', termText:'#e2e8f0', termPrompt:'#0d7a62', termMuted:'#4a5568', termBorder:'#1e2533',
  folderColor: '#d08030',
  fileColor:   '#5c5850',
};

// ─── MOCK DATA ────────────────────────────────────────────────────────────────
const SAVED_CONNECTIONS = [
  { id:'c1', name:'Production', host:'ciat.org', port:22, user:'sara', lastUsed:'2 days ago', favorite:true  },
  { id:'c2', name:'Dev Server', host:'dev.ciat.org', port:22, user:'sara', lastUsed:'1 week ago', favorite:true  },
  { id:'c3', name:'Staging',    host:'staging.ciat.org', port:2222, user:'deploy', lastUsed:'3 weeks ago', favorite:false },
  { id:'c4', name:'Vagrant',    host:'192.168.33.10',    port:22,   user:'vagrant', lastUsed:'1 month ago', favorite:false },
];

const LOCAL_FS = {
  path: '/home/sara/projects',
  items: [
    { name:'harness-backend',  type:'dir',  size:'-',      perms:'drwxr-xr-x', modified:'2026-05-25 09:14', fav:true  },
    { name:'harness-frontend', type:'dir',  size:'-',      perms:'drwxr-xr-x', modified:'2026-05-24 16:30', fav:false },
    { name:'notes.md',         type:'file', size:'4.2 KB', perms:'-rw-r--r--', modified:'2026-05-22 11:00', fav:false },
    { name:'deploy.sh',        type:'file', size:'1.8 KB', perms:'-rwxr-xr-x', modified:'2026-05-20 08:45', fav:false },
    { name:'.env.local',       type:'file', size:'512 B',  perms:'-rw-------',  modified:'2026-05-18 14:22', fav:false },
    { name:'docker-compose.yml',type:'file',size:'2.1 KB', perms:'-rw-r--r--', modified:'2026-05-15 10:10', fav:false },
  ],
};

const REMOTE_FS = {
  path: '/home/sara',
  items: [
    { name:'harness-backend',  type:'dir',  size:'-',       perms:'drwxr-xr-x', modified:'2026-05-25 09:14', fav:true  },
    { name:'logs',             type:'dir',  size:'-',       perms:'drwxr-x---', modified:'2026-05-26 07:00', fav:false },
    { name:'backups',          type:'dir',  size:'-',       perms:'drwx------', modified:'2026-05-20 03:00', fav:false },
    { name:'.bashrc',          type:'file', size:'3.1 KB',  perms:'-rw-r--r--', modified:'2026-05-10 12:00', fav:false },
    { name:'.ssh',             type:'dir',  size:'-',       perms:'drwx------', modified:'2026-04-01 09:00', fav:false },
    { name:'server.log',       type:'file', size:'142.6 KB',perms:'-rw-r--r--', modified:'2026-05-26 08:55', fav:false },
    { name:'cron.log',         type:'file', size:'22.3 KB', perms:'-rw-r--r--', modified:'2026-05-26 06:00', fav:false },
  ],
};

const INIT_TRANSFERS = [
  { id:'t1', name:'deploy.sh',         direction:'up',   size:'1.8 KB',  status:'done',    progress:100, time:'09:14:32' },
  { id:'t2', name:'harness-backend/',  direction:'down', size:'14.2 MB', status:'done',    progress:100, time:'09:12:10' },
  { id:'t3', name:'server.log',        direction:'down', size:'142.6 KB',status:'error',   progress:68,  time:'08:55:01' },
];

// ─── HELPERS ──────────────────────────────────────────────────────────────────
function FileIcon({ type, name }) {
  if (type === 'dir') return <span style={{ color:SP.folderColor, fontSize:14 }}>📁</span>;
  const ext = name.split('.').pop().toLowerCase();
  const codeExts = ['js','ts','jsx','tsx','py','rs','go','sh','toml','json','yml','yaml','md','css','html'];
  if (codeExts.includes(ext)) return <span style={{ fontSize:12, color:'#0f52c8', fontWeight:700, fontFamily:SP.mono }}>{ }</span>;
  if (['png','jpg','gif','svg','webp'].includes(ext)) return <span style={{ fontSize:13 }}>🖼</span>;
  return <span style={{ fontSize:13, color:SP.muted }}>📄</span>;
}

function TransferIcon({ direction, status }) {
  if (status === 'error') return <span style={{ color:SP.danger, fontSize:12 }}>✕</span>;
  if (status === 'active') return <span style={{ color:SP.accent, fontSize:12 }}>{ direction==='up'?'↑':'↓' }</span>;
  return <span style={{ color:SP.success, fontSize:12 }}>{ direction==='up'?'↑':'↓' }</span>;
}

// ─── CONNECTION SCREEN ────────────────────────────────────────────────────────
function ConnectionScreen({ onConnect, onAdd }) {
  const [filter, setFilter] = useState('');
  const filtered = SAVED_CONNECTIONS.filter(c =>
    c.name.toLowerCase().includes(filter.toLowerCase()) ||
    c.host.toLowerCase().includes(filter.toLowerCase())
  );
  const favorites = filtered.filter(c => c.favorite);
  const others    = filtered.filter(c => !c.favorite);

  return (
    <div style={{ flex:1, display:'flex', flexDirection:'column', alignItems:'center', justifyContent:'flex-start', background:SP.canvasBg, padding:'40px 0', overflow:'auto' }}>
      <div style={{ width:'100%', maxWidth:640, display:'flex', flexDirection:'column', gap:24, padding:'0 32px' }}>
        {/* Header */}
        <div style={{ display:'flex', alignItems:'center', justifyContent:'space-between' }}>
          <div>
            <h2 style={{ fontSize:22, fontWeight:800, color:SP.text, margin:0 }}>SSH Connections</h2>
            <p style={{ fontSize:13, color:SP.muted, margin:'4px 0 0' }}>Select a connection or configure a new one</p>
          </div>
          <button onClick={onAdd} style={{ display:'inline-flex', alignItems:'center', gap:7, padding:'9px 18px', background:SP.accent, color:'#fff', border:'none', borderRadius:7, fontSize:13, fontWeight:600, cursor:'pointer', fontFamily:SP.ui, boxShadow:'0 2px 8px rgba(13,122,98,0.22)' }}>
            <HIcon.Plus size={13}/> New connection
          </button>
        </div>

        {/* Search */}
        <div style={{ display:'flex', alignItems:'center', gap:8, background:SP.inputBg, border:`1px solid ${SP.inputBorder}`, borderRadius:6, padding:'8px 12px' }}>
          <HIcon.Search size={13}/>
          <input value={filter} onChange={e=>setFilter(e.target.value)} placeholder="Search connections…"
            style={{ flex:1, border:'none', background:'transparent', color:SP.text, fontSize:13, fontFamily:SP.ui, outline:'none' }}/>
        </div>

        {/* Favorites */}
        {favorites.length > 0 && (
          <div>
            <div style={{ fontSize:10, letterSpacing:1.3, color:SP.label, textTransform:'uppercase', fontWeight:700, marginBottom:10 }}>⭐ Favorites</div>
            <div style={{ display:'grid', gridTemplateColumns:'1fr 1fr', gap:10 }}>
              {favorites.map(c => <ConnCard key={c.id} conn={c} onConnect={onConnect} highlight />)}
            </div>
          </div>
        )}

        {/* Others */}
        {others.length > 0 && (
          <div>
            <div style={{ fontSize:10, letterSpacing:1.3, color:SP.label, textTransform:'uppercase', fontWeight:700, marginBottom:10 }}>All connections</div>
            <div style={{ display:'flex', flexDirection:'column', gap:8 }}>
              {others.map(c => <ConnCard key={c.id} conn={c} onConnect={onConnect} />)}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ConnCard({ conn, onConnect, highlight }) {
  return (
    <div onClick={()=>onConnect(conn)}
      style={{ background:SP.bg, border:`1px solid ${SP.border}`, borderRadius:9, padding:'14px 16px', cursor:'pointer', transition:'all .12s' }}
      onMouseEnter={e=>{ e.currentTarget.style.borderColor=SP.accent; e.currentTarget.style.boxShadow=`0 0 0 2px ${SP.accentRing}`; }}
      onMouseLeave={e=>{ e.currentTarget.style.borderColor=SP.border; e.currentTarget.style.boxShadow='none'; }}>
      <div style={{ display:'flex', alignItems:'center', justifyContent:'space-between', marginBottom:6 }}>
        <span style={{ fontSize:14, fontWeight:700, color:SP.text }}>{conn.name}</span>
        {conn.favorite && <span style={{ fontSize:12 }}>⭐</span>}
      </div>
      <div style={{ fontSize:12, color:SP.accent, fontFamily:SP.mono, marginBottom:4 }}>{conn.user}@{conn.host}:{conn.port}</div>
      <div style={{ fontSize:11, color:SP.muted }}>Last used {conn.lastUsed}</div>
    </div>
  );
}

// ─── CONNECTION DRAWER ────────────────────────────────────────────────────────
function ConnectionDrawer({ onClose, onSave }) {
  const [form, setForm] = useState({ name:'', host:'', port:'22', user:'', password:'' });
  const set = (k,v) => setForm(f=>({...f,[k]:v}));
  const fields = [
    { key:'name',     label:'Connection name', placeholder:'e.g. Production server', type:'text' },
    { key:'host',     label:'Host / IP',        placeholder:'ciat.org or 192.168.1.1', type:'text' },
    { key:'port',     label:'Port',             placeholder:'22', type:'number' },
    { key:'user',     label:'Username',          placeholder:'sara', type:'text' },
    { key:'password', label:'Password',          placeholder:'••••••••', type:'password' },
  ];
  return (
    <div style={{ position:'absolute', inset:0, zIndex:200, display:'flex', justifyContent:'flex-end' }}>
      <div onClick={onClose} style={{ position:'absolute', inset:0, background:'rgba(28,26,22,0.22)' }}/>
      <div style={{ position:'relative', width:420, background:SP.bg, borderLeft:`1px solid ${SP.border}`, display:'flex', flexDirection:'column', boxShadow:'-8px 0 32px rgba(0,0,0,0.10)' }}>
        <div style={{ padding:'20px 22px', borderBottom:`1px solid ${SP.border}` }}>
          <div style={{ fontSize:16, fontWeight:700, color:SP.text }}>New SSH Connection</div>
          <div style={{ fontSize:12, color:SP.muted, marginTop:2 }}>Configure connection details</div>
        </div>
        <div style={{ flex:1, overflow:'auto', padding:'20px 22px', display:'flex', flexDirection:'column', gap:16 }}>
          {fields.map(f => (
            <div key={f.key}>
              <label style={{ display:'block', fontSize:10.5, fontWeight:700, color:SP.label, letterSpacing:1.1, textTransform:'uppercase', marginBottom:6 }}>{f.label}</label>
              <input type={f.type} value={form[f.key]} onChange={e=>set(f.key,e.target.value)} placeholder={f.placeholder}
                style={{ width:'100%', padding:'9px 12px', borderRadius:6, border:`1px solid ${SP.inputBorder}`, background:SP.inputBg, color:SP.text, fontSize:13, fontFamily:f.key==='host'||f.key==='user'?SP.mono:SP.ui, outline:'none' }}/>
            </div>
          ))}
          {/* Key auth note */}
          <div style={{ padding:'10px 14px', borderRadius:6, background:SP.accentBg, border:`1px solid ${SP.accentRing}`, fontSize:12, color:SP.textSec }}>
            💡 SSH Key auth coming soon — use <code style={{ fontFamily:SP.mono, color:SP.accent }}>ssh-agent</code> forwarding in the meantime.
          </div>
        </div>
        <div style={{ padding:'16px 22px', borderTop:`1px solid ${SP.border}`, display:'flex', gap:8, justifyContent:'flex-end' }}>
          <button onClick={onClose} style={{ padding:'8px 14px', borderRadius:6, border:`1px solid ${SP.border}`, background:'transparent', color:SP.textSec, cursor:'pointer', fontSize:13, fontFamily:SP.ui }}>Cancel</button>
          <button onClick={()=>{ onSave(form); onClose(); }} style={{ padding:'8px 20px', borderRadius:6, border:'none', background:SP.accent, color:'#fff', cursor:'pointer', fontSize:13, fontFamily:SP.ui, fontWeight:600, boxShadow:'0 2px 8px rgba(13,122,98,0.22)' }}>
            Connect
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── FILE PANE ────────────────────────────────────────────────────────────────
function FilePane({ label, fs, selected, onSelect, onToggleFav, dropTarget, onDragOver, onDrop, onDragLeave }) {
  const [filter, setFilter] = useState('');
  const [sortCol, setSortCol] = useState('name');
  const [sortDir, setSortDir] = useState('asc');

  const visible = fs.items
    .filter(f => f.name.toLowerCase().includes(filter.toLowerCase()))
    .sort((a,b)=>{
      let va=a[sortCol]||'', vb=b[sortCol]||'';
      if(sortCol==='name') { if(a.type!==b.type) return a.type==='dir'?-1:1; }
      return sortDir==='asc'?va.localeCompare(vb):vb.localeCompare(va);
    });

  function toggleSort(col) {
    if(sortCol===col) setSortDir(d=>d==='asc'?'desc':'asc');
    else { setSortCol(col); setSortDir('asc'); }
  }

  const SortIcon = ({ col }) => {
    if(sortCol!==col) return <span style={{ color:SP.muted, opacity:.4, fontSize:9 }}>⇅</span>;
    return <span style={{ color:SP.accent, fontSize:9 }}>{sortDir==='asc'?'↑':'↓'}</span>;
  };

  return (
    <div onDragOver={onDragOver} onDrop={onDrop} onDragLeave={onDragLeave}
      style={{ flex:1, display:'flex', flexDirection:'column', minWidth:0, border:`2px solid ${dropTarget?SP.accent:'transparent'}`, borderRadius: dropTarget?6:0, transition:'border-color .12s', position:'relative' }}>

      {/* Drop overlay */}
      {dropTarget && (
        <div style={{ position:'absolute', inset:0, zIndex:20, background:'rgba(13,122,98,0.08)', display:'flex', alignItems:'center', justifyContent:'center', pointerEvents:'none' }}>
          <div style={{ fontSize:28, color:SP.accent, fontWeight:700, textAlign:'center' }}>
            <div>⬇</div>
            <div style={{ fontSize:13, marginTop:8 }}>Drop to {label==='Local'?'download':'upload'}</div>
          </div>
        </div>
      )}

      {/* Pane header */}
      <div style={{ padding:'10px 14px', borderBottom:`1px solid ${SP.border}`, background:SP.panelBg, display:'flex', alignItems:'center', gap:10 }}>
        <span style={{ fontSize:11, fontWeight:700, color:SP.accent, background:SP.accentBg, padding:'2px 8px', borderRadius:4 }}>{label}</span>
        <span style={{ fontSize:12, color:SP.muted, fontFamily:SP.mono, flex:1, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap' }}>{fs.path}</span>
        {/* Filter */}
        <div style={{ display:'flex', alignItems:'center', gap:6, background:SP.inputBg, border:`1px solid ${SP.inputBorder}`, borderRadius:4, padding:'4px 8px', fontSize:11 }}>
          <HIcon.Search size={10}/>
          <input value={filter} onChange={e=>setFilter(e.target.value)} placeholder="filter…"
            style={{ width:90, border:'none', background:'transparent', color:SP.text, fontSize:11, fontFamily:SP.ui, outline:'none' }}/>
        </div>
      </div>

      {/* Bookmarks bar */}
      <div style={{ display:'flex', gap:6, padding:'6px 14px', borderBottom:`1px solid ${SP.border}`, background:SP.bg, overflowX:'auto' }}>
        {fs.items.filter(f=>f.fav).map((f,i)=>(
          <span key={i} style={{ display:'inline-flex', alignItems:'center', gap:4, padding:'2px 8px', borderRadius:4, background:SP.panelBg, border:`1px solid ${SP.border}`, fontSize:11, color:SP.accent, cursor:'pointer', whiteSpace:'nowrap' }}>
            ⭐ {f.name}
          </span>
        ))}
        <span style={{ display:'inline-flex', alignItems:'center', gap:4, padding:'2px 8px', borderRadius:4, background:'transparent', border:`1px dashed ${SP.border}`, fontSize:11, color:SP.muted, cursor:'pointer' }}>+ Bookmark</span>
      </div>

      {/* Column headers */}
      <div style={{ display:'grid', gridTemplateColumns:'1fr 80px 110px 130px', gap:0, padding:'6px 14px', borderBottom:`1px solid ${SP.border}`, background:SP.panelBg, fontSize:10, fontWeight:700, color:SP.muted, letterSpacing:.4 }}>
        {[['name','Name'],['size','Size'],['perms','Permissions'],['modified','Modified']].map(([col,lbl])=>(
          <div key={col} onClick={()=>toggleSort(col)} style={{ display:'flex', alignItems:'center', gap:4, cursor:'pointer', userSelect:'none' }}>
            {lbl} <SortIcon col={col}/>
          </div>
        ))}
      </div>

      {/* File list */}
      <div style={{ flex:1, overflow:'auto' }}>
        {/* Parent dir */}
        <div style={{ display:'grid', gridTemplateColumns:'1fr 80px 110px 130px', padding:'7px 14px', fontSize:12, color:SP.muted, cursor:'pointer', borderBottom:`1px solid ${SP.divider}` }}
          onMouseEnter={e=>e.currentTarget.style.background=SP.rowHover}
          onMouseLeave={e=>e.currentTarget.style.background='transparent'}>
          <span style={{ display:'flex', alignItems:'center', gap:8 }}>
            <span style={{ color:SP.folderColor, fontSize:13 }}>📁</span> ..
          </span>
          <span>-</span><span>-</span><span>-</span>
        </div>

        {visible.map((f,i)=>{
          const isSel = selected.has(f.name);
          return (
            <div key={i} draggable
              onDragStart={e=>{ e.dataTransfer.setData('text/plain', f.name); e.dataTransfer.setData('source', label); }}
              onClick={e=>{ if(e.metaKey||e.ctrlKey){ const n=new Set(selected); isSel?n.delete(f.name):n.add(f.name); onSelect(n); } else onSelect(new Set([f.name])); }}
              style={{
                display:'grid', gridTemplateColumns:'1fr 80px 110px 130px',
                padding:'7px 14px', fontSize:12,
                background:isSel?SP.rowSelected:'transparent',
                borderBottom:`1px solid ${SP.divider}`,
                cursor:'pointer', userSelect:'none',
              }}
              onMouseEnter={e=>{ if(!isSel) e.currentTarget.style.background=SP.rowHover; }}
              onMouseLeave={e=>{ if(!isSel) e.currentTarget.style.background='transparent'; }}>
              <span style={{ display:'flex', alignItems:'center', gap:8, overflow:'hidden' }}>
                <FileIcon type={f.type} name={f.name}/>
                <span style={{ overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap', color:f.type==='dir'?SP.text:SP.textSec, fontWeight:f.type==='dir'?600:400 }}>{f.name}</span>
                {f.fav && <span style={{ fontSize:10, flexShrink:0 }}>⭐</span>}
              </span>
              <span style={{ color:SP.muted, fontFamily:SP.mono }}>{f.size}</span>
              <span style={{ color:SP.muted, fontFamily:SP.mono, fontSize:11 }}>{f.perms}</span>
              <span style={{ color:SP.muted, fontFamily:SP.mono, fontSize:11 }}>{f.modified.split(' ')[0]}</span>
            </div>
          );
        })}
      </div>
      <div style={{ padding:'6px 14px', borderTop:`1px solid ${SP.border}`, fontSize:10.5, color:SP.muted, fontFamily:SP.mono, background:SP.panelBg }}>
        {visible.length} items{selected.size>0?` · ${selected.size} selected`:''}
      </div>
    </div>
  );
}

// ─── TRANSFER LOG ─────────────────────────────────────────────────────────────
function TransferLog({ transfers }) {
  return (
    <div style={{ flex:1, overflow:'auto', fontFamily:SP.mono, fontSize:11.5 }}>
      {/* Header */}
      <div style={{ display:'grid', gridTemplateColumns:'16px 1fr 80px 80px 80px', gap:0, padding:'6px 14px', background:SP.panelBg, borderBottom:`1px solid ${SP.border}`, fontSize:10, fontWeight:700, color:SP.muted, letterSpacing:.4 }}>
        <span/>
        <span>Name</span><span>Size</span><span>Status</span><span>Time</span>
      </div>
      {transfers.map((t,i)=>{
        const statusCfg = { done:{color:SP.success,label:'Done'}, error:{color:SP.danger,label:'Error'}, active:{color:SP.accent,label:'Active'} }[t.status]||{};
        return (
          <div key={t.id} style={{ display:'grid', gridTemplateColumns:'16px 1fr 80px 80px 80px', padding:'7px 14px', borderBottom:`1px solid ${SP.divider}`, alignItems:'center', background: t.status==='error'?SP.dangerBg:'transparent' }}>
            <TransferIcon direction={t.direction} status={t.status}/>
            <span style={{ color:SP.text, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap' }}>{t.name}</span>
            <span style={{ color:SP.muted }}>{t.size}</span>
            <span style={{ color:statusCfg.color, fontWeight:600 }}>{statusCfg.label}</span>
            <span style={{ color:SP.muted }}>{t.time}</span>
          </div>
        );
      })}
    </div>
  );
}

// ─── ACTIVE TRANSFER BAR ──────────────────────────────────────────────────────
function TransferBar({ transfers }) {
  const active = transfers.find(t=>t.status==='active');
  if (!active) return null;
  return (
    <div style={{ display:'flex', alignItems:'center', gap:10, padding:'6px 16px', background:SP.accentBg, borderBottom:`1px solid ${SP.accentRing}`, fontSize:11, color:SP.accent }}>
      <span style={{ fontWeight:700 }}>↑</span>
      <span style={{ fontFamily:SP.mono }}>{active.name}</span>
      <div style={{ flex:1, height:4, borderRadius:2, background:'rgba(13,122,98,0.15)', overflow:'hidden' }}>
        <div style={{ width:`${active.progress}%`, height:'100%', background:SP.accent, borderRadius:2 }}/>
      </div>
      <span style={{ fontFamily:SP.mono }}>{active.progress}%</span>
      <span style={{ color:SP.muted }}>{active.size}</span>
    </div>
  );
}

// ─── SSH TERMINAL ─────────────────────────────────────────────────────────────
function SSHTerminal({ conn }) {
  const [inputVal, setInputVal] = useState('');
  const [log, setLog] = useState([
    { type:'info',  text:`Connected to ${conn.host} as ${conn.user}` },
    { type:'info',  text:'Welcome to Ubuntu 22.04.3 LTS (GNU/Linux 5.15.0-100-generic x86_64)' },
    { type:'prompt'},
  ]);
  const logRef = useRef(null);
  useEffect(()=>{ if(logRef.current) logRef.current.scrollTop=logRef.current.scrollHeight; },[log]);

  function submit() {
    if(!inputVal.trim()) return;
    const cmd = inputVal.trim();
    const fakeOutput = {
      'ls':    'harness-backend/  logs/  backups/  .bashrc  .ssh/  server.log  cron.log',
      'pwd':   '/home/sara',
      'whoami':'sara',
      'df -h': 'Filesystem      Size  Used Avail Use% Mounted on\n/dev/sda1        50G   22G   26G  46% /',
      'uptime':'08:55:01 up 12 days, 3:42, 2 users, load average: 0.15, 0.12, 0.09',
    }[cmd] || `bash: ${cmd}: command simulated`;
    setLog(prev=>[...prev.filter(e=>e.type!=='prompt'),{type:'cmd',text:cmd},{type:'out',text:fakeOutput},{type:'prompt'}]);
    setInputVal('');
  }

  return (
    <div ref={logRef} style={{ flex:1, overflow:'auto', padding:'12px 16px', fontFamily:SP.mono, fontSize:12.5, lineHeight:1.7, background:SP.termBg }}>
      <div style={{ marginBottom:8, paddingBottom:8, borderBottom:`1px solid ${SP.termBorder}`, color:SP.termMuted }}>
        ssh {conn.user}@{conn.host} -p {conn.port}
      </div>
      {log.map((e,i)=>{
        if(e.type==='info')   return <div key={i} style={{ color:'#94a3b8', marginBottom:2 }}>{e.text}</div>;
        if(e.type==='cmd')    return <div key={i} style={{ display:'flex', gap:8 }}><span style={{ color:SP.termPrompt, fontWeight:700 }}>$</span><span style={{ color:'#f8fafc' }}>{e.text}</span></div>;
        if(e.type==='out')    return <pre key={i} style={{ margin:'0 0 4px 0', color:'#94a3b8', whiteSpace:'pre-wrap' }}>{e.text}</pre>;
        if(e.type==='prompt') return (
          <div key={i} style={{ display:'flex', alignItems:'center', gap:8 }}>
            <span style={{ color:SP.termPrompt, fontWeight:700 }}>$</span>
            <input value={inputVal} onChange={e=>setInputVal(e.target.value)}
              onKeyDown={e=>{ if(e.key==='Enter') submit(); }}
              placeholder="Type a command…"
              style={{ flex:1, background:'transparent', border:'none', outline:'none', color:SP.termText, fontFamily:SP.mono, fontSize:12.5, caretColor:SP.termPrompt }}/>
          </div>
        );
        return null;
      })}
    </div>
  );
}

// ─── CONNECTED VIEW ───────────────────────────────────────────────────────────
function ConnectedView({ conn, onDisconnect }) {
  const [localSel,  setLocalSel]  = useState(new Set());
  const [remoteSel, setRemoteSel] = useState(new Set());
  const [dropTarget, setDropTarget] = useState(null); // 'local' | 'remote'
  const [bottomH,  setBottomH]    = useState(200);
  const [bottomTab, setBottomTab] = useState('log');
  const [bottomOpen, setBottomOpen] = useState(true);
  const [transfers, setTransfers] = useState(INIT_TRANSFERS);
  const [localFs,  setLocalFs]    = useState(LOCAL_FS);
  const [remoteFs, setRemoteFs]   = useState(REMOTE_FS);

  function doTransfer(direction) {
    const sel  = direction==='up' ? localSel : remoteSel;
    if(!sel.size) return;
    const name = [...sel][0];
    const newT = { id:'t'+Date.now(), name, direction: direction==='up'?'up':'down', size:'3.2 KB', status:'active', progress:0, time:new Date().toTimeString().slice(0,8) };
    setTransfers(prev=>[newT,...prev]);
    setBottomOpen(true); setBottomTab('log');
    let p=0;
    const iv = setInterval(()=>{ p+=14; if(p>=100){ clearInterval(iv); setTransfers(prev=>prev.map(t=>t.id===newT.id?{...t,status:'done',progress:100}:t)); } else setTransfers(prev=>prev.map(t=>t.id===newT.id?{...t,progress:p}:t)); },200);
  }

  function handleDrop(targetPane, e) {
    e.preventDefault(); setDropTarget(null);
    const name = e.dataTransfer.getData('text/plain');
    const src  = e.dataTransfer.getData('source');
    if(!name||src===targetPane) return;
    doTransfer(targetPane==='Remote'?'up':'down');
  }

  // Toolbar actions (rename, delete, chmod, mkdir)
  function addFolder(side) {
    const name = prompt('New folder name:');
    if(!name) return;
    const newItem = { name, type:'dir', size:'-', perms:'drwxr-xr-x', modified:new Date().toISOString().slice(0,16).replace('T',' '), fav:false };
    if(side==='local')  setLocalFs(f=>({...f, items:[newItem,...f.items]}));
    else                setRemoteFs(f=>({...f, items:[newItem,...f.items]}));
  }

  const fileToolbar = (side, sel) => (
    <div style={{ display:'flex', gap:4, padding:'6px 10px', borderBottom:`1px solid ${SP.border}`, background:SP.panelBg, flexWrap:'wrap' }}>
      {[
        { label:'⬆ Upload',   action:()=>doTransfer('up'),   disabled:side==='remote'||!localSel.size  },
        { label:'⬇ Download', action:()=>doTransfer('down'),  disabled:side==='local'||!remoteSel.size  },
        { label:'📁 New folder', action:()=>addFolder(side), disabled:false },
        { label:'✏ Rename',   action:()=>{}, disabled:!sel.size },
        { label:'🗑 Delete',   action:()=>{}, disabled:!sel.size, danger:true },
        { label:'🔒 chmod',   action:()=>{}, disabled:!sel.size },
      ].map((btn,i)=>(
        <button key={i} disabled={btn.disabled} onClick={btn.action} style={{ padding:'4px 10px', borderRadius:4, fontSize:11, fontWeight:500, border:`1px solid ${btn.danger?SP.danger:SP.border}`, background:'transparent', color:btn.disabled?SP.muted:btn.danger?SP.danger:SP.textSec, cursor:btn.disabled?'default':'pointer', fontFamily:SP.ui, opacity:btn.disabled?.5:1 }}>
          {btn.label}
        </button>
      ))}
    </div>
  );

  return (
    <div style={{ flex:1, display:'flex', flexDirection:'column', minHeight:0 }}>
      {/* Toolbar */}
      <div style={{ height:44, display:'flex', alignItems:'center', justifyContent:'space-between', padding:'0 20px', borderBottom:`1px solid ${SP.border}`, background:SP.bg, flexShrink:0 }}>
        <div style={{ display:'flex', alignItems:'center', gap:12 }}>
          <span style={{ display:'inline-flex', alignItems:'center', gap:6, fontSize:12, color:SP.success, fontWeight:600 }}>
            <HIcon.Dot color={SP.success}/>{conn.name}
          </span>
          <span style={{ fontSize:12, color:SP.muted, fontFamily:SP.mono }}>{conn.user}@{conn.host}:{conn.port}</span>
        </div>
        <div style={{ display:'flex', gap:8 }}>
          <button onClick={()=>{setBottomOpen(o=>!o);setBottomTab('terminal');}} style={{ padding:'5px 12px', borderRadius:5, fontSize:12, border:`1px solid ${SP.border}`, background:'transparent', color:SP.textSec, cursor:'pointer', fontFamily:SP.ui }}>
            ⌨ Terminal {bottomOpen&&bottomTab==='terminal'?'▼':'▲'}
          </button>
          <button onClick={onDisconnect} style={{ padding:'5px 12px', borderRadius:5, fontSize:12, border:`1px solid ${SP.danger}`, background:SP.dangerBg, color:SP.danger, cursor:'pointer', fontFamily:SP.ui, fontWeight:600 }}>
            Disconnect
          </button>
        </div>
      </div>

      {/* Active transfer indicator */}
      <TransferBar transfers={transfers}/>

      {/* Dual pane */}
      <div style={{ flex:1, display:'flex', minHeight:0, overflow:'hidden' }}>
        {/* Local */}
        <div style={{ flex:1, display:'flex', flexDirection:'column', minWidth:0, borderRight:`2px solid ${SP.border}` }}>
          {fileToolbar('local', localSel)}
          <FilePane label="Local" fs={localFs} selected={localSel} onSelect={setLocalSel}
            dropTarget={dropTarget==='local'}
            onDragOver={e=>{e.preventDefault();setDropTarget('local');}}
            onDrop={e=>handleDrop('Local',e)}
            onDragLeave={()=>setDropTarget(null)}/>
        </div>

        {/* Center: transfer buttons */}
        <div style={{ width:52, display:'flex', flexDirection:'column', alignItems:'center', justifyContent:'center', gap:10, background:SP.panelBg, flexShrink:0, borderRight:`1px solid ${SP.border}` }}>
          <button onClick={()=>doTransfer('up')} disabled={!localSel.size} title="Upload →"
            style={{ width:34, height:34, borderRadius:7, border:`1px solid ${localSel.size?SP.accent:SP.border}`, background:localSel.size?SP.accentBg:'transparent', color:localSel.size?SP.accent:SP.muted, cursor:localSel.size?'pointer':'default', fontSize:14, fontWeight:700, display:'flex', alignItems:'center', justifyContent:'center', transition:'all .12s' }}>→</button>
          <button onClick={()=>doTransfer('down')} disabled={!remoteSel.size} title="← Download"
            style={{ width:34, height:34, borderRadius:7, border:`1px solid ${remoteSel.size?SP.accent:SP.border}`, background:remoteSel.size?SP.accentBg:'transparent', color:remoteSel.size?SP.accent:SP.muted, cursor:remoteSel.size?'pointer':'default', fontSize:14, fontWeight:700, display:'flex', alignItems:'center', justifyContent:'center', transition:'all .12s' }}>←</button>
        </div>

        {/* Remote */}
        <div style={{ flex:1, display:'flex', flexDirection:'column', minWidth:0 }}>
          {fileToolbar('remote', remoteSel)}
          <FilePane label="Remote" fs={remoteFs} selected={remoteSel} onSelect={setRemoteSel}
            dropTarget={dropTarget==='remote'}
            onDragOver={e=>{e.preventDefault();setDropTarget('remote');}}
            onDrop={e=>handleDrop('Remote',e)}
            onDragLeave={()=>setDropTarget(null)}/>
        </div>
      </div>

      {/* Bottom panel: Terminal + Log */}
      {bottomOpen && (
        <div style={{ height:bottomH, borderTop:`2px solid ${SP.border}`, display:'flex', flexDirection:'column', background:bottomTab==='terminal'?SP.termBg:SP.bg, flexShrink:0 }}>
          {/* Tab strip */}
          <div style={{ display:'flex', alignItems:'center', height:34, borderBottom:`1px solid ${bottomTab==='terminal'?SP.termBorder:SP.border}`, background:bottomTab==='terminal'?'rgba(0,0,0,0.4)':SP.panelBg, flexShrink:0, padding:'0 14px', gap:4 }}>
            {[{id:'terminal',label:'⌨ Terminal'},{id:'log',label:`📋 Transfer log (${transfers.length})`}].map(t=>(
              <button key={t.id} onClick={()=>setBottomTab(t.id)} style={{ padding:'4px 12px', borderRadius:4, border:'none', background:bottomTab===t.id?(bottomTab==='terminal'?'rgba(255,255,255,0.08)':SP.bg):'transparent', color:bottomTab===t.id?(bottomTab==='terminal'?'#e2e8f0':SP.accent):SP.muted, fontWeight:bottomTab===t.id?600:400, cursor:'pointer', fontSize:11.5, fontFamily:SP.ui }}>
                {t.label}
              </button>
            ))}
            <button onClick={()=>setBottomOpen(false)} style={{ marginLeft:'auto', background:'none', border:'none', color:SP.muted, cursor:'pointer', fontSize:16 }}>×</button>
          </div>
          {/* Content */}
          {bottomTab==='terminal' ? <SSHTerminal conn={conn}/> : <TransferLog transfers={transfers}/>}
        </div>
      )}
    </div>
  );
}

// ─── ROOT ─────────────────────────────────────────────────────────────────────
function SSHView() {
  const [conn,    setConn]    = useState(null);
  const [showAdd, setShowAdd] = useState(false);

  return (
    <div style={{ flex:1, display:'flex', flexDirection:'column', minHeight:0, position:'relative' }}>
      {/* Sub-header */}
      <div style={{ height:48, display:'flex', alignItems:'center', justifyContent:'space-between', padding:'0 24px', borderBottom:`1px solid ${SP.border}`, background:SP.bg, flexShrink:0 }}>
        <div>
          <span style={{ fontSize:15, fontWeight:800, color:SP.text }}>SSH</span>
          <span style={{ fontSize:12, color:SP.muted, marginLeft:10 }}>File manager &amp; secure shell</span>
        </div>
        <div style={{ display:'flex', gap:8 }}>
          <button onClick={()=>setShowAdd(true)} style={{ display:'inline-flex', alignItems:'center', gap:6, padding:'5px 12px', fontSize:12, border:`1px solid ${SP.border}`, borderRadius:5, background:'transparent', color:SP.textSec, cursor:'pointer', fontFamily:SP.ui }}>
            ⚙ Manage connections
          </button>
          {conn && <button onClick={()=>setConn(null)} style={{ padding:'5px 12px', borderRadius:5, fontSize:12, border:`1px solid ${SP.border}`, background:'transparent', color:SP.muted, cursor:'pointer', fontFamily:SP.ui }}>← All connections</button>}
        </div>
      </div>

      {conn
        ? <ConnectedView conn={conn} onDisconnect={()=>setConn(null)}/>
        : <ConnectionScreen onConnect={c=>setConn(c)} onAdd={()=>setShowAdd(true)}/>
      }
      {showAdd && <ConnectionDrawer onClose={()=>setShowAdd(false)} onSave={f=>{setConn({...f,name:f.name||f.host,id:'new',favorite:false,lastUsed:'just now'});}}/>}
    </div>
  );
}

window.SSHView = SSHView;
