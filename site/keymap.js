// the keyboard map demo: the product's own behaviour (modifier chips, key states, the owner list)
(() => {
  const SYM = { ctrl: '⌃', opt: '⌥', shift: '⇧', cmd: '⌘' };
  const ORDER = ['ctrl', 'opt', 'shift', 'cmd'];
  const ROWS = [
    { id: 'fn', keys: [['esc', 'w15'], 'F1','F2','F3','F4','F5','F6','F7','F8','F9','F10','F11','F12'] },
    { keys: ['`','1','2','3','4','5','6','7','8','9','0','-','=',['⌫','w15']] },
    { keys: [['⇥','w15'],'Q','W','E','R','T','Y','U','I','O','P','[',']','\\'] },
    { keys: [['caps','w2'],'A','S','D','F','G','H','J','K','L',';',"'",['↩','w15']] },
    { keys: [['⇧','w25','shift'],'Z','X','C','V','B','N','M',',','.','/',['⇧','w2','shift']] },
    { keys: [['fn','w1'],['⌃','w1','ctrl'],['⌥','w1','opt'],['⌘','w15','cmd'],['Space','wsp'],['⌘','w15','cmd'],['⌥','w1','opt'],'←','↑','↓','→'] },
  ];
  // what this demo Mac uses: combo -> [status, [app, action, scope]...]
  const A = (app, act, w) => [app, act, w || 'In the app'];
  const SW = 'System-wide';
  const D = {
    'cmd': {
      C: ['shared', A('Every app','Copy')], V: ['shared', A('Every app','Paste')], X: ['shared', A('Every app','Cut')], Z: ['shared', A('Every app','Undo')],
      A: ['shared', A('Every app','Select All')], S: ['shared', A('Most apps','Save')], W: ['shared', A('Every app','Close Window')], Q: ['shared', A('Every app','Quit')],
      N: ['shared', A('Finder','New Window'), A('Notes','New Note'), A('Mail','New Message')], T: ['shared', A('Safari','New Tab'), A('Terminal','New Tab'), A('Finder','New Tab')],
      F: ['shared', A('Every app','Find')], P: ['shared', A('Most apps','Print')], O: ['shared', A('Most apps','Open')], H: ['shared', A('Every app','Hide')],
      M: ['shared', A('Every app','Minimize')], ',': ['shared', A('Every app','Settings')], L: ['app', A('Safari','Open Location')], R: ['shared', A('Safari','Reload Page'), A('Mail','Reply')],
      K: ['app', A('Terminal','Clear to Start')], Y: ['app', A('Safari','Show History')], G: ['shared', A('Most apps','Find Next')], E: ['app', A('Finder','Eject')],
      B: ['app', A('Notes','Bold')], I: ['app', A('Finder','Get Info')], U: ['app', A('Notes','Underline')], D: ['app', A('Safari','Add Bookmark')], J: ['app', A('Finder','Show View Options')],
      '1': ['app', A('Finder','as Icons')], '2': ['app', A('Finder','as List')], '3': ['app', A('Finder','as Columns')], '0': ['app', A('Safari','Actual Size')],
      '=': ['shared', A('Safari','Zoom In'), A('Notes','Bigger')], '-': ['shared', A('Safari','Zoom Out'), A('Notes','Smaller')], '[': ['shared', A('Safari','Back'), A('Finder','Back')], ']': ['shared', A('Safari','Forward'), A('Finder','Forward')],
      Space: ['blocks', A('Spotlight','Show Spotlight search', SW)], '⇥': ['blocks', A('macOS','Switch apps', SW)], '`': ['blocks', A('macOS','Next window of the app', SW)],
      '/': ['app', A('Safari','Show Status Bar')], '↑': ['shared', A('Finder','Enclosing Folder')], '↓': ['app', A('Finder','Open')], '⌫': ['app', A('Finder','Move to Trash')],
    },
    'shift+cmd': {
      '3': ['blocks', A('Screenshot','Save picture of screen', SW)], '4': ['blocks', A('Screenshot','Save picture of selected area', SW)], '5': ['blocks', A('Screenshot','Screenshot and recording options', SW), A('Bear','Heading 5')],
      Z: ['shared', A('Every app','Redo')], N: ['shared', A('Finder','New Folder'), A('Safari','New Private Window')], T: ['shared', A('Safari','Reopen Last Tab'), A('Finder','Show Tab Bar')],
      G: ['shared', A('Finder','Go to Folder'), A('Safari','Find Previous')], D: ['shared', A('Finder','Desktop'), A('Safari','Add to Reading List')], A: ['blocks', A('skhd','Open Finder', SW), A('Finder','Applications'), A('Safari','AutoFill Form')],
      '\\': ['app', A('Safari','Show Tab Overview')], H: ['shared', A('Finder','Home'), A('Safari','Home')], L: ['app', A('Safari','Show Sidebar')], R: ['app', A('Safari','Show Reader')],
      '/': ['shared', A('Every app','Help')], K: ['app', A('Terminal','New Remote Connection')], O: ['app', A('Finder','Documents')], U: ['app', A('Finder','Utilities')],
    },
    'opt+cmd': {
      Space: ['blocks', A('Finder','Show Finder search window', SW)], esc: ['blocks', A('macOS','Force Quit', SW)], D: ['blocks', A('Dock','Turn hiding on or off', SW)],
      H: ['blocks', A('AeroSpace','Focus left', SW), A('Every app','Hide Others')], J: ['blocks', A('AeroSpace','Focus down', SW)], K: ['blocks', A('AeroSpace','Focus up', SW)], L: ['blocks', A('AeroSpace','Focus right', SW), A('Safari','Show Downloads')],
      W: ['shared', A('Safari','Close Other Tabs'), A('Finder','Close All')], T: ['app', A('Safari','New Tab at End')], I: ['app', A('Finder','Show Inspector')], M: ['shared', A('Every app','Minimize All')],
      V: ['app', A('Finder','Move Item Here')], S: ['app', A('Finder','Show Sidebar')], P: ['app', A('Finder','Show Path Bar')],
    },
    'ctrl': {
      Space: ['clash', A('skhd','Open Terminal', SW), A('macOS','Select previous input source', SW)], '⇥': ['shared', A('Safari','Show Next Tab'), A('Terminal','Next Tab')],
      '←': ['blocks', A('Mission Control','Move left a space', SW)], '→': ['blocks', A('Mission Control','Move right a space', SW)], '↑': ['blocks', A('Mission Control','Show Mission Control', SW)], '↓': ['blocks', A('Mission Control','Application windows', SW)],
      A: ['app', A('Terminal','Line start')], E: ['app', A('Terminal','Line end')], K: ['app', A('Terminal','Delete to end')],
    },
    'ctrl+cmd': {
      F: ['shared', A('Every app','Enter Full Screen')], Q: ['blocks', A('macOS','Lock Screen', SW)], Space: ['blocks', A('macOS','Emoji & Symbols', SW)], D: ['blocks', A('macOS','Look Up', SW)],
      '1': ['app', A('Safari','Show Bookmarks Sidebar')], '2': ['app', A('Safari','Show Reading List')],
    },
    'opt': { Space: ['blocks', A('Wispr Flow','Hold to dictate', SW)], '⇥': ['app', A('Safari','Highlight next item')] },
    'ctrl+opt': { Space: ['clash', A('AeroSpace','Toggle floating', SW), A('Wispr Flow','Hands-free dictation', SW)], '←': ['blocks', A('Rectangle','Left half', SW)], '→': ['blocks', A('Rectangle','Right half', SW)], '↩': ['blocks', A('Rectangle','Maximize', SW)] },
    'opt+shift+cmd': { V: ['shared', A('Most apps','Paste and Match Style')], esc: ['blocks', A('macOS','Force quit front app', SW)] },
  };
  const LABEL = { free: 'free', app: 'used inside one app', shared: 'used by several apps', blocks: 'a system-wide tool takes it', clash: 'clash' };
  const mods = new Set(['ctrl']); let sel = 'Space';
  const kb = document.getElementById('kb'), list = document.getElementById('kmap-list');
  const title = document.getElementById('kmap-title'), sub = document.getElementById('kmap-sub');
  const chips = [...document.querySelectorAll('.kmap__mods button')];
  const keyOf = () => ORDER.filter(m => mods.has(m)).join('+');
  const symOf = () => ORDER.filter(m => mods.has(m)).map(m => SYM[m]).join('');
  for (const r of ROWS) {
    const row = document.createElement('div'); row.className = 'kb__row'; if (r.id) row.dataset.row = r.id;
    let n = 0;
    for (const k of r.keys) {
      const [label, w, mod] = Array.isArray(k) ? k : [k];
      const b = document.createElement('button'); b.type = 'button'; b.textContent = label;
      if (w) b.classList.add(w);
      n += w === 'wsp' ? 12 : w === 'w25' ? 5 : w === 'w2' ? 4 : w === 'w15' ? 3 : 2;
      if (mod) { b.dataset.modKey = mod; b.setAttribute('aria-label', mod === 'cmd' ? 'command' : mod === 'opt' ? 'option' : mod === 'ctrl' ? 'control' : 'shift'); }
      else if (label === 'fn' || label === 'caps') { b.tabIndex = -1; b.disabled = true; }
      else b.dataset.key = label;
      row.append(b);
    }
    row.style.setProperty('--n', n); kb.append(row);
  }
  function entry(k) { const t = D[keyOf()]; return t && t[k]; }
  function render() {
    const sym = symOf();
    chips.forEach(c => c.setAttribute('aria-pressed', mods.has(c.dataset.mod)));
    kb.querySelectorAll('[data-mod-key]').forEach(b => b.setAttribute('aria-pressed', mods.has(b.dataset.modKey)));
    let used = 0;
    kb.querySelectorAll('[data-key]').forEach(b => {
      const k = b.dataset.key, e = mods.size ? entry(k) : null, s = e ? e[0] : 'free';
      if (e) used++;
      b.dataset.s = s; b.classList.toggle('is-sel', sel === k);
      b.setAttribute('aria-label', `${sym}${k}: ${LABEL[s]}`);
    });
    if (sel && mods.size) {
      const e = entry(sel); title.textContent = `${sym} ${sel}`;
      sub.textContent = e ? LABEL[e[0]][0].toUpperCase() + LABEL[e[0]].slice(1) : 'Free with these modifier keys';
      list.innerHTML = '';
      for (const [app, act, w] of (e ? e.slice(1) : [])) {
        const li = document.createElement('li');
        li.innerHTML = `<b></b><span></span>`; li.querySelector('b').textContent = act; li.querySelector('span').textContent = `${app} · ${w}`;
        list.append(li);
      }
    } else {
      title.textContent = mods.size ? `${sym} + key: ${used} keys in use` : 'Pick a modifier key';
      sub.textContent = mods.size ? 'Click a key to see its apps' : 'Hold or click ⌃ ⌥ ⇧ ⌘';
      list.innerHTML = '';
      const t = mods.size ? D[keyOf()] || {} : {};
      const rank = { clash: 0, blocks: 1, shared: 2, app: 3 };
      Object.entries(t).sort((a, b) => rank[a[1][0]] - rank[b[1][0]]).slice(0, 8).forEach(([k, e]) => {
        const li = document.createElement('li');
        li.innerHTML = `<b></b><span></span>`; li.querySelector('b').textContent = `${sym}${k}  ${e[1][1]}`; li.querySelector('span').textContent = `${e[1][0]}${e.length > 2 ? ` and ${e.length - 2} more` : ''} · ${LABEL[e[0]]}`;
        list.append(li);
      });
    }
  }
  const toggle = m => { mods.has(m) ? mods.delete(m) : mods.add(m); sel = null; render(); };
  chips.forEach(c => c.addEventListener('click', () => toggle(c.dataset.mod)));
  kb.addEventListener('click', e => {
    const b = e.target.closest('button'); if (!b) return;
    if (b.dataset.modKey) return toggle(b.dataset.modKey);
    if (b.dataset.key) { sel = sel === b.dataset.key ? null : b.dataset.key; render(); }
  });
  // physical modifier keys light up the map while held (the browser keeps ⌘ for itself on some keys)
  const KM = { Control: 'ctrl', Alt: 'opt', Shift: 'shift', Meta: 'cmd' };
  let held = null;
  const map = document.getElementById('kmap');
  const onScreen = () => { const r = map.getBoundingClientRect(); return r.top < innerHeight && r.bottom > 0; };
  addEventListener('keydown', e => {
    const m = KM[e.key]; if (!m || !onScreen() || e.target.closest('input, textarea')) return;
    if (!held) { held = new Set(mods); mods.clear(); }
    mods.add(m); sel = null; render();
  });
  addEventListener('keyup', e => { if (KM[e.key] && held && ![...Object.keys(KM)].some(k => e.getModifierState && e.getModifierState(k))) { mods.clear(); held.forEach(m => mods.add(m)); held = null; render(); } });
  addEventListener('blur', () => { if (held) { mods.clear(); held.forEach(m => mods.add(m)); held = null; render(); } });
  render();
})();
