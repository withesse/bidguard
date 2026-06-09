// shell.jsx — App shell shared across all screens (sidebar + topbar)

const T = vTokens; // alias

// ─────────────────────────────────────────────────────────────
// Sidebar
// ─────────────────────────────────────────────────────────────
function VSidebar({ active = 'tasks', collapsed = false, dark = false, accent = T.brand }) {
  const bg = dark ? '#15151B' : T.paper2;
  const border = dark ? 'rgba(255,255,255,0.06)' : T.line;
  const navItem = (k, label, icon, badge) => (
    <NavRow key={k} active={active === k} dark={dark} accent={accent}
      label={collapsed ? '' : label} icon={icon} badge={collapsed ? null : badge} />
  );
  return (
    <div style={{
      width: collapsed ? 56 : 220, flexShrink: 0,
      background: bg,
      borderRight: `1px solid ${border}`,
      display: 'flex', flexDirection: 'column',
      color: dark ? 'rgba(255,255,255,0.85)' : T.ink,
    }}>
      {/* Workspace switcher */}
      <div style={{ padding: collapsed ? '12px 8px' : '12px 12px 8px' }}>
        <div style={{
          display: 'flex', alignItems: 'center', gap: 9, padding: '7px 8px',
          background: dark ? 'rgba(255,255,255,0.04)' : T.white,
          border: `1px solid ${dark ? 'rgba(255,255,255,0.06)' : T.line}`,
          borderRadius: 8, cursor: 'pointer',
        }}>
          <div style={{
            width: 24, height: 24, borderRadius: 6,
            background: `linear-gradient(135deg, ${accent}, ${shade(accent, -25)})`,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            color: '#fff', fontSize: 11, fontWeight: 700, letterSpacing: '-0.01em',
          }}>合</div>
          {!collapsed && <>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 12, fontWeight: 600, color: dark ? '#fff' : T.ink, lineHeight: 1.2 }}>
                合规与法务
              </div>
              <div style={{ fontSize: 10.5, color: dark ? 'rgba(255,255,255,0.5)' : T.ink3, marginTop: 1 }}>
                Workspace · 24人
              </div>
            </div>
            <VIcon name="chevD" size={12} style={{ color: dark ? 'rgba(255,255,255,0.4)' : T.ink4 }} />
          </>}
        </div>
      </div>

      {/* Primary action */}
      {!collapsed && (
        <div style={{ padding: '0 12px 8px' }}>
          <button style={{
            width: '100%', height: 30, padding: '0 10px',
            background: accent, color: '#fff', border: 'none',
            borderRadius: 7, fontSize: 12, fontWeight: 600,
            display: 'flex', alignItems: 'center', gap: 7, cursor: 'pointer',
            boxShadow: `0 1px 0 ${shade(accent, -20)} inset, 0 1px 2px rgba(0,0,0,0.08)`,
          }}>
            <VIcon name="plus" size={13} />
            <span>新建查重任务</span>
            <span style={{ marginLeft: 'auto', fontSize: 10.5, opacity: 0.8, letterSpacing: '0.02em' }}>⌘N</span>
          </button>
        </div>
      )}

      {/* Nav */}
      <div style={{ padding: '4px 8px', display: 'flex', flexDirection: 'column', gap: 1 }}>
        {navItem('inbox',   '收件箱',     'upload',  '3')}
        {navItem('tasks',   '我的任务',   'list',    null)}
        {navItem('history', '历史记录',   'history', null)}
      </div>

      <SidebarLabel collapsed={collapsed} dark={dark}>资源库</SidebarLabel>
      <div style={{ padding: '0 8px', display: 'flex', flexDirection: 'column', gap: 1 }}>
        {navItem('library',  '文档库',     'docs',    '1.2k')}
        {navItem('sources',  '查重源',     'archive', null)}
        {navItem('snippets', '片段聚合',   'tag',     null)}
      </div>

      <SidebarLabel collapsed={collapsed} dark={dark}>团队</SidebarLabel>
      <div style={{ padding: '0 8px', display: 'flex', flexDirection: 'column', gap: 1 }}>
        {navItem('comments', '评论',       'comment', '12')}
        {navItem('members',  '成员',       'users',   null)}
      </div>

      <div style={{ flex: 1 }} />

      {/* Bottom: settings + user */}
      <div style={{ padding: '8px 8px 10px', borderTop: `1px solid ${border}` }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
          {navItem('settings', '设置', 'cog')}
        </div>
        {!collapsed && (
          <div style={{
            marginTop: 6, display: 'flex', alignItems: 'center', gap: 9,
            padding: '6px 8px', borderRadius: 7,
          }}>
            <Avatar name="周" color={'#7B5BE8'} size={24} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11.5, fontWeight: 600, color: dark ? '#fff' : T.ink, lineHeight: 1.2 }}>
                周明远
              </div>
              <div style={{ fontSize: 10, color: dark ? 'rgba(255,255,255,0.5)' : T.ink3, marginTop: 1 }}>
                法务合规部 · Admin
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function SidebarLabel({ children, collapsed, dark }) {
  if (collapsed) return <div style={{ height: 8 }} />;
  return (
    <div style={{
      padding: '12px 16px 4px',
      fontSize: 10, fontWeight: 600, letterSpacing: '0.06em',
      textTransform: 'uppercase',
      color: dark ? 'rgba(255,255,255,0.4)' : T.ink4,
    }}>{children}</div>
  );
}

function NavRow({ active, label, icon, badge, dark, accent }) {
  const activeBg = dark ? 'rgba(255,255,255,0.07)' : T.white;
  const activeShadow = dark ? 'none' : `0 1px 0 ${T.line}`;
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 9,
      height: 28, padding: '0 9px', borderRadius: 6,
      background: active ? activeBg : 'transparent',
      boxShadow: active ? activeShadow : 'none',
      color: active
        ? (dark ? '#fff' : T.ink)
        : (dark ? 'rgba(255,255,255,0.72)' : T.ink2),
      cursor: 'pointer', position: 'relative',
    }}>
      {active && (
        <div style={{
          position: 'absolute', left: -8, top: 6, bottom: 6, width: 2.5,
          borderRadius: 2, background: accent,
        }} />
      )}
      <VIcon name={icon} size={14} style={{ color: active ? accent : (dark ? 'rgba(255,255,255,0.55)' : T.ink3) }} />
      {label && <span style={{ flex: 1, fontSize: 12, fontWeight: active ? 600 : 500 }}>{label}</span>}
      {badge && (
        <span style={{
          fontSize: 10, fontWeight: 600,
          padding: '1px 6px', borderRadius: 999,
          background: dark ? 'rgba(255,255,255,0.08)' : T.paper3,
          color: dark ? 'rgba(255,255,255,0.7)' : T.ink3,
        }}>{badge}</span>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Topbar (above each screen)
// ─────────────────────────────────────────────────────────────
function VTopbar({ crumbs = [], actions, dark = false }) {
  return (
    <div style={{
      height: 44, flexShrink: 0, display: 'flex', alignItems: 'center',
      gap: 14, padding: '0 16px',
      borderBottom: `1px solid ${dark ? 'rgba(255,255,255,0.06)' : T.line}`,
      background: dark ? '#1A1A1F' : T.white,
    }}>
      {/* breadcrumb */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12.5 }}>
        {crumbs.map((c, i) => (
          <React.Fragment key={i}>
            {i > 0 && <span style={{ color: dark ? 'rgba(255,255,255,0.3)' : T.ink5 }}>/</span>}
            <span style={{
              color: i === crumbs.length - 1
                ? (dark ? '#fff' : T.ink)
                : (dark ? 'rgba(255,255,255,0.55)' : T.ink3),
              fontWeight: i === crumbs.length - 1 ? 600 : 500,
            }}>{c}</span>
          </React.Fragment>
        ))}
      </div>
      <div style={{ flex: 1 }} />
      {/* search */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6, padding: '0 9px',
        height: 26, borderRadius: 6,
        background: dark ? 'rgba(255,255,255,0.06)' : T.paper2,
        border: `1px solid ${dark ? 'rgba(255,255,255,0.05)' : T.line}`,
        width: 220, color: dark ? 'rgba(255,255,255,0.55)' : T.ink3,
      }}>
        <VIcon name="search" size={12} />
        <span style={{ fontSize: 11.5, flex: 1 }}>搜索文档、片段、任务…</span>
        <span style={{ fontSize: 10, letterSpacing: '0.02em' }}>⌘K</span>
      </div>
      {/* icon actions */}
      <ToolbarIcon name="bell" dark={dark} badge />
      <ToolbarIcon name="comment" dark={dark} />
      {actions}
    </div>
  );
}

function ToolbarIcon({ name, dark, badge }) {
  return (
    <div style={{
      width: 26, height: 26, borderRadius: 6,
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      color: dark ? 'rgba(255,255,255,0.7)' : T.ink2,
      position: 'relative', cursor: 'pointer',
    }}>
      <VIcon name={name} size={14} />
      {badge && (
        <span style={{
          position: 'absolute', top: 4, right: 4, width: 6, height: 6,
          borderRadius: '50%', background: T.danger,
          border: `1.5px solid ${dark ? '#1A1A1F' : T.white}`,
        }} />
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Reusable atoms
// ─────────────────────────────────────────────────────────────
function Avatar({ name = 'A', color, size = 22 }) {
  return (
    <div style={{
      width: size, height: size, borderRadius: '50%',
      background: color || '#5B5BD6',
      color: '#fff', fontSize: size * 0.42, fontWeight: 700,
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      flexShrink: 0, letterSpacing: '-0.01em',
    }}>{name}</div>
  );
}

function Pill({ children, fg = T.ink2, bg = T.paper2, size = 11, weight = 500, style }) {
  return (
    <span style={{
      display: 'inline-flex', alignItems: 'center', gap: 4,
      padding: '2px 7px', borderRadius: 999,
      fontSize: size, fontWeight: weight,
      color: fg, background: bg, lineHeight: 1.4,
      ...style,
    }}>{children}</span>
  );
}

function DocChip({ type = 'docx', style }) {
  const t = T['doc' + type.charAt(0).toUpperCase() + type.slice(1).toLowerCase()] || T.docTxt;
  return (
    <span style={{
      display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
      padding: '1.5px 5px', borderRadius: 3,
      fontSize: 9.5, fontWeight: 700, letterSpacing: '0.04em',
      color: t.fg, background: t.bg,
      fontFamily: T.mono,
      ...style,
    }}>{t.label}</span>
  );
}

function Button({ kind = 'primary', size = 'md', icon, children, accent = T.brand, style, dark }) {
  const sizes = {
    sm: { h: 24, px: 9, fs: 11.5, gap: 5 },
    md: { h: 30, px: 12, fs: 12, gap: 6 },
    lg: { h: 36, px: 14, fs: 13, gap: 7 },
  }[size];
  const kinds = {
    primary: {
      bg: accent, color: '#fff', border: 'transparent',
      shadow: `0 1px 0 ${shade(accent, -20)} inset, 0 1px 2px rgba(0,0,0,0.08)`,
    },
    secondary: {
      bg: dark ? 'rgba(255,255,255,0.06)' : T.white,
      color: dark ? '#fff' : T.ink,
      border: dark ? 'rgba(255,255,255,0.10)' : T.line,
      shadow: dark ? 'none' : `0 1px 2px rgba(20,18,14,0.04)`,
    },
    ghost: {
      bg: 'transparent',
      color: dark ? 'rgba(255,255,255,0.8)' : T.ink2,
      border: 'transparent', shadow: 'none',
    },
    danger: {
      bg: T.danger, color: '#fff', border: 'transparent',
      shadow: `0 1px 0 ${shade(T.danger, -20)} inset`,
    },
  }[kind];
  return (
    <button style={{
      height: sizes.h, padding: `0 ${sizes.px}px`,
      background: kinds.bg, color: kinds.color,
      border: kinds.border === 'transparent' ? 'none' : `1px solid ${kinds.border}`,
      borderRadius: 6, fontSize: sizes.fs, fontWeight: 600,
      display: 'inline-flex', alignItems: 'center', gap: sizes.gap,
      cursor: 'pointer', boxShadow: kinds.shadow, fontFamily: T.font,
      ...style,
    }}>
      {icon && <VIcon name={icon} size={sizes.fs + 1} />}
      {children}
    </button>
  );
}

// Generic helper — shade a hex color by amount (-100..+100)
function shade(hex, amt) {
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
  VSidebar, VTopbar, Avatar, Pill, DocChip, Button, shade,
});
