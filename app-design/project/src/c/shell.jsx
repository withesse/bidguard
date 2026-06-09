// c-shell.jsx — Sidebar + topbar + atoms for 「原本」C-end app

// ─────────────────────────────────────────────────────────────
// Sidebar — personal, clean. No team, no workspace, no badges.
// ─────────────────────────────────────────────────────────────
function CSidebar({ active = 'home', dark = false, accent = C.brand, layout = 'comfort' }) {
  const compact = layout === 'compact';
  const bg = dark ? '#15151B' : C.paper2;
  const border = dark ? 'rgba(255,255,255,0.06)' : C.line;
  const inkMute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const inkStrong = dark ? '#fff' : C.ink;

  const nav = (k, label, icon) => (
    <CNavRow key={k} active={active === k} dark={dark} accent={accent}
      label={compact ? '' : label} icon={icon} compact={compact} />
  );

  return (
    <div style={{
      width: compact ? 60 : 208, flexShrink: 0,
      background: bg, borderRight: `1px solid ${border}`,
      display: 'flex', flexDirection: 'column',
      color: inkStrong,
    }}>
      {/* Brand */}
      <div style={{
        padding: compact ? '14px 0 10px' : '14px 14px 10px',
        display: 'flex', alignItems: 'center', gap: 9,
        justifyContent: compact ? 'center' : 'flex-start',
      }}>
        <CLogo size={20} color={accent} />
        {!compact && (
          <div>
            <div style={{ fontSize: 13, fontWeight: 700, color: inkStrong, letterSpacing: '-0.01em', lineHeight: 1 }}>
              原本
            </div>
            <div style={{ fontSize: 9.5, color: inkMute, marginTop: 3, letterSpacing: '0.04em' }}>
              Verum · 个人版
            </div>
          </div>
        )}
      </div>

      {/* Primary CTA */}
      <div style={{ padding: compact ? '6px 8px 10px' : '6px 12px 12px' }}>
        <button style={{
          width: '100%', height: 32, padding: compact ? 0 : '0 10px',
          background: accent, color: '#fff', border: 'none', borderRadius: 8,
          fontSize: 12.5, fontWeight: 600, letterSpacing: '-0.005em',
          display: 'flex', alignItems: 'center', gap: 7,
          justifyContent: compact ? 'center' : 'flex-start', cursor: 'pointer',
          boxShadow: `0 1px 0 ${shadeC(accent, -22)} inset, 0 1px 2px rgba(20,18,14,0.08)`,
          fontFamily: C.font,
        }}>
          <CIcon name="plus" size={14} />
          {!compact && <>
            <span>开始查重</span>
            <span style={{ marginLeft: 'auto', fontSize: 10.5, opacity: 0.75, fontFamily: C.mono }}>⌘N</span>
          </>}
        </button>
      </div>

      {/* Nav */}
      <div style={{ padding: compact ? '0 8px' : '0 8px', display: 'flex', flexDirection: 'column', gap: 1 }}>
        {nav('home',     '首页',         'home')}
        {nav('docs',     '我的文档',     'folder')}
        {nav('history',  '检测记录',     'history')}
      </div>
      <CSidebarLabel collapsed={compact} dark={dark}>工具</CSidebarLabel>
      <div style={{ padding: '0 8px', display: 'flex', flexDirection: 'column', gap: 1 }}>
        {nav('rewrite',  'AI 改写',      'ai')}
        {nav('library',  '查重源',       'book')}
      </div>

      <div style={{ flex: 1 }} />

      {/* Plan hint — quiet, not pushy */}
      {!compact && (
        <div style={{
          margin: '8px 12px 0', padding: '10px 12px', borderRadius: 8,
          background: dark ? 'rgba(255,255,255,0.04)' : C.white,
          border: `1px solid ${border}`,
        }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ fontSize: 11, fontWeight: 600, color: inkStrong }}>本月剩余</span>
          </div>
          <div style={{
            fontSize: 18, fontWeight: 700, color: inkStrong,
            letterSpacing: '-0.014em', marginTop: 3, fontFamily: C.font,
          }}>
            8<span style={{ fontSize: 11, color: inkMute, fontWeight: 500, marginLeft: 4 }}>/ 10 次</span>
          </div>
          <div style={{ height: 3, background: dark ? 'rgba(255,255,255,0.08)' : C.paper3, borderRadius: 2, marginTop: 8, overflow: 'hidden' }}>
            <div style={{ height: '100%', width: '80%', background: accent }} />
          </div>
        </div>
      )}

      {/* Bottom: settings + user */}
      <div style={{ padding: '8px 8px 10px' }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 1, marginBottom: 4 }}>
          {nav('settings', '设置', 'cog')}
        </div>
        {!compact && (
          <div style={{
            marginTop: 4, display: 'flex', alignItems: 'center', gap: 9,
            padding: '6px 8px', borderRadius: 7,
          }}>
            <CAvatar name="林" size={24} color="#7B8FE5" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11.5, fontWeight: 600, color: inkStrong, lineHeight: 1.2 }}>
                林一帆
              </div>
              <div style={{ fontSize: 10, color: inkMute, marginTop: 1 }}>
                lin.yifan@…
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function CSidebarLabel({ children, collapsed, dark }) {
  if (collapsed) return <div style={{ height: 10 }} />;
  return (
    <div style={{
      padding: '14px 16px 4px',
      fontSize: 9.5, fontWeight: 600, letterSpacing: '0.07em',
      textTransform: 'uppercase',
      color: dark ? 'rgba(255,255,255,0.38)' : C.ink4,
    }}>{children}</div>
  );
}

function CNavRow({ active, label, icon, dark, accent, compact }) {
  const activeBg = dark ? 'rgba(255,255,255,0.06)' : C.white;
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 9,
      height: 30, padding: compact ? 0 : '0 10px', borderRadius: 7,
      justifyContent: compact ? 'center' : 'flex-start',
      background: active ? activeBg : 'transparent',
      boxShadow: active && !dark ? `0 1px 0 ${C.line}` : 'none',
      color: active
        ? (dark ? '#fff' : C.ink)
        : (dark ? 'rgba(255,255,255,0.7)' : C.ink2),
      cursor: 'pointer', position: 'relative',
    }}>
      <CIcon name={icon} size={14} style={{ color: active ? accent : (dark ? 'rgba(255,255,255,0.55)' : C.ink3) }} />
      {label && <span style={{ flex: 1, fontSize: 12.5, fontWeight: active ? 600 : 500 }}>{label}</span>}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Topbar — minimal, lots of breathing room
// ─────────────────────────────────────────────────────────────
function CTopbar({ title, sub, actions, dark = false, accent = C.brand }) {
  return (
    <div style={{
      height: 60, flexShrink: 0, padding: '0 24px',
      display: 'flex', alignItems: 'center', gap: 14,
      borderBottom: `1px solid ${dark ? 'rgba(255,255,255,0.06)' : C.line}`,
      background: dark ? '#1A1A20' : C.paper,
    }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: 14, fontWeight: 700, color: dark ? '#fff' : C.ink,
          letterSpacing: '-0.01em', lineHeight: 1.2,
        }}>{title}</div>
        {sub && (
          <div style={{ fontSize: 11.5, color: dark ? 'rgba(255,255,255,0.55)' : C.ink3, marginTop: 2 }}>
            {sub}
          </div>
        )}
      </div>
      {/* search */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 7, padding: '0 11px',
        height: 30, borderRadius: 7,
        background: dark ? 'rgba(255,255,255,0.05)' : C.white,
        border: `1px solid ${dark ? 'rgba(255,255,255,0.05)' : C.line}`,
        width: 240, color: dark ? 'rgba(255,255,255,0.5)' : C.ink3,
      }}>
        <CIcon name="search" size={12} />
        <span style={{ fontSize: 11.5, flex: 1 }}>搜索文档或片段</span>
        <span style={{ fontSize: 10, fontFamily: C.mono }}>⌘K</span>
      </div>
      {actions}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Atoms
// ─────────────────────────────────────────────────────────────
function CAvatar({ name, color = C.brand, size = 24 }) {
  return (
    <div style={{
      width: size, height: size, borderRadius: '50%', background: color,
      color: '#fff', fontSize: size * 0.44, fontWeight: 700,
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      flexShrink: 0, letterSpacing: '-0.01em', fontFamily: C.font,
    }}>{name}</div>
  );
}

function CPill({ children, fg = C.ink2, bg = C.paper2, size = 11, weight = 500, style }) {
  return (
    <span style={{
      display: 'inline-flex', alignItems: 'center', gap: 4,
      padding: '2px 8px', borderRadius: 999,
      fontSize: size, fontWeight: weight,
      color: fg, background: bg, lineHeight: 1.4,
      ...style,
    }}>{children}</span>
  );
}

function CDocChip({ type = 'docx', style }) {
  const t = C['doc' + type.charAt(0).toUpperCase() + type.slice(1).toLowerCase()] || C.docTxt;
  return (
    <span style={{
      display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
      padding: '1.5px 5px', borderRadius: 3,
      fontSize: 9.5, fontWeight: 700, letterSpacing: '0.04em',
      color: t.fg, background: t.bg, fontFamily: C.mono, ...style,
    }}>{t.label}</span>
  );
}

function CButton({ kind = 'primary', size = 'md', icon, iconRight, children, accent = C.brand, style, dark }) {
  const sizes = {
    sm: { h: 26, px: 10, fs: 11.5, gap: 5 },
    md: { h: 32, px: 13, fs: 12.5, gap: 6 },
    lg: { h: 40, px: 18, fs: 13.5, gap: 8 },
  }[size];
  const kinds = {
    primary: {
      bg: accent, color: '#fff', border: 'transparent',
      shadow: `0 1px 0 ${shadeC(accent, -22)} inset, 0 1px 2px rgba(20,18,14,0.08)`,
    },
    secondary: {
      bg: dark ? 'rgba(255,255,255,0.05)' : C.white,
      color: dark ? '#fff' : C.ink,
      border: dark ? 'rgba(255,255,255,0.10)' : C.line,
      shadow: dark ? 'none' : `0 1px 2px rgba(20,18,14,0.04)`,
    },
    ghost: {
      bg: 'transparent', color: dark ? 'rgba(255,255,255,0.8)' : C.ink2,
      border: 'transparent', shadow: 'none',
    },
  }[kind];
  return (
    <button style={{
      height: sizes.h, padding: `0 ${sizes.px}px`,
      background: kinds.bg, color: kinds.color,
      border: kinds.border === 'transparent' ? 'none' : `1px solid ${kinds.border}`,
      borderRadius: 7, fontSize: sizes.fs, fontWeight: 600,
      display: 'inline-flex', alignItems: 'center', gap: sizes.gap,
      cursor: 'pointer', boxShadow: kinds.shadow, fontFamily: C.font,
      letterSpacing: '-0.005em',
      ...style,
    }}>
      {icon && <CIcon name={icon} size={sizes.fs + 1} />}
      {children}
      {iconRight && <CIcon name={iconRight} size={sizes.fs + 1} />}
    </button>
  );
}

function CToggle({ on, accent = C.brand, onChange }) {
  return (
    <div onClick={onChange} style={{
      width: 28, height: 16, borderRadius: 999,
      background: on ? accent : '#C9C5CF',
      position: 'relative', cursor: 'pointer', transition: 'background 0.15s',
      flexShrink: 0,
    }}>
      <div style={{
        position: 'absolute', top: 2, left: on ? 14 : 2,
        width: 12, height: 12, borderRadius: '50%',
        background: '#fff', boxShadow: '0 1px 2px rgba(0,0,0,0.2)',
        transition: 'left 0.15s',
      }} />
    </div>
  );
}

function CSegControl({ options, value, accent, dark, onChange }) {
  return (
    <div style={{
      display: 'flex', padding: 2,
      background: dark ? 'rgba(255,255,255,0.05)' : C.paper2,
      border: `1px solid ${dark ? 'rgba(255,255,255,0.08)' : C.line}`,
      borderRadius: 7,
    }}>
      {options.map((o, i) => (
        <div key={i} onClick={() => onChange && onChange(i)} style={{
          flex: 1, textAlign: 'center', padding: '5px 12px',
          fontSize: 11.5, fontWeight: 600,
          background: i === value ? (dark ? '#2A2A33' : '#fff') : 'transparent',
          color: i === value ? (dark ? '#fff' : C.ink) : (dark ? 'rgba(255,255,255,0.55)' : C.ink3),
          borderRadius: 5, cursor: 'pointer',
          boxShadow: i === value && !dark ? '0 1px 2px rgba(0,0,0,0.06)' : 'none',
        }}>{o}</div>
      ))}
    </div>
  );
}

function shadeC(hex, amt) {
  const h = hex.replace('#', '');
  const num = parseInt(h, 16);
  let r = (num >> 16) + amt * 2;
  let g = ((num >> 8) & 0xff) + amt * 2;
  let b = (num & 0xff) + amt * 2;
  r = Math.max(0, Math.min(255, r));
  g = Math.max(0, Math.min(255, g));
  b = Math.max(0, Math.min(255, b));
  return '#' + ((r << 16) | (g << 8) | b).toString(16).padStart(6, '0');
}

Object.assign(window, {
  CSidebar, CTopbar, CAvatar, CPill, CDocChip, CButton, CToggle, CSegControl, shadeC,
});
