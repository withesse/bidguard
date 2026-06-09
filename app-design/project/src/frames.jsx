// frames.jsx — macOS & Windows 11 window chrome for Verity
// Tight chrome optimized for desktop productivity apps, not the OS shell.

const VFRAME_FONT = vTokens.font;

// ─────────────────────────────────────────────────────────────
// macOS chrome: rounded 12px, unified titlebar, traffic lights
// ─────────────────────────────────────────────────────────────
function MacFrame({ width = 1280, height = 800, title = 'Verity', children, dark = false, accent }) {
  const bg = dark ? '#1A1A1F' : vTokens.paper;
  const titleColor = dark ? 'rgba(255,255,255,0.7)' : vTokens.ink2;
  const titlebarBg = dark ? '#222227' : '#EFECE5';
  const border = dark ? 'rgba(255,255,255,0.08)' : 'rgba(20,18,14,0.10)';
  return (
    <div style={{
      width, height, borderRadius: 12, overflow: 'hidden',
      background: bg, fontFamily: VFRAME_FONT,
      boxShadow: dark
        ? '0 0 0 0.5px rgba(255,255,255,0.10), 0 24px 60px rgba(0,0,0,0.45)'
        : '0 0 0 0.5px rgba(20,18,14,0.18), 0 24px 60px rgba(20,18,14,0.18)',
      display: 'flex', flexDirection: 'column', position: 'relative',
    }}>
      {/* Title bar */}
      <div style={{
        height: 36, flexShrink: 0,
        background: titlebarBg,
        borderBottom: `0.5px solid ${border}`,
        display: 'flex', alignItems: 'center', padding: '0 12px', gap: 10,
        position: 'relative',
      }}>
        {/* traffic lights */}
        <div style={{ display: 'flex', gap: 8 }}>
          <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#FF5F57', border: '0.5px solid rgba(0,0,0,0.1)' }} />
          <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#FEBC2E', border: '0.5px solid rgba(0,0,0,0.1)' }} />
          <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#28C840', border: '0.5px solid rgba(0,0,0,0.1)' }} />
        </div>
        {/* Centered title */}
        <div style={{
          position: 'absolute', left: 0, right: 0, top: 0, bottom: 0,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          pointerEvents: 'none',
        }}>
          <div style={{
            display: 'flex', alignItems: 'center', gap: 7,
            fontSize: 12.5, fontWeight: 600, color: titleColor, letterSpacing: '-0.005em',
          }}>
            <VLogo size={13} color={accent || vTokens.brand} />
            <span>{title}</span>
          </div>
        </div>
      </div>
      {/* Content */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', overflow: 'hidden' }}>
        {children}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Windows 11 chrome: sharper square corners, right-side controls,
// title-bar with Mica-like tint and app menu strip.
// ─────────────────────────────────────────────────────────────
function WinFrame({ width = 1280, height = 800, title = 'Verity', children, dark = false, accent }) {
  const bg = dark ? '#1F1F23' : vTokens.paper;
  const titleColor = dark ? 'rgba(255,255,255,0.85)' : '#1A1A1A';
  const titlebarBg = dark ? '#26262C' : '#F4F2EC';
  const border = dark ? 'rgba(255,255,255,0.08)' : 'rgba(20,18,14,0.08)';
  return (
    <div style={{
      width, height, borderRadius: 8, overflow: 'hidden',
      background: bg, fontFamily: '"Segoe UI Variable Display", "Segoe UI", ' + VFRAME_FONT,
      boxShadow: dark
        ? '0 0 0 0.5px rgba(255,255,255,0.08), 0 24px 60px rgba(0,0,0,0.45)'
        : '0 0 0 0.5px rgba(20,18,14,0.14), 0 24px 60px rgba(20,18,14,0.16)',
      display: 'flex', flexDirection: 'column',
    }}>
      {/* Title bar */}
      <div style={{
        height: 36, flexShrink: 0, background: titlebarBg,
        borderBottom: `1px solid ${border}`,
        display: 'flex', alignItems: 'stretch',
      }}>
        {/* Left: icon + title */}
        <div style={{
          display: 'flex', alignItems: 'center', gap: 8, padding: '0 12px', flex: 1,
          fontSize: 12, fontWeight: 600, color: titleColor, letterSpacing: '-0.005em',
        }}>
          <VLogo size={14} color={accent || vTokens.brand} />
          <span>{title}</span>
        </div>
        {/* Right: window controls */}
        <div style={{ display: 'flex' }}>
          <WinCtl><svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" strokeWidth="1" fill="none"/></svg></WinCtl>
          <WinCtl><svg width="10" height="10" viewBox="0 0 10 10"><rect x="1" y="1" width="8" height="8" stroke="currentColor" strokeWidth="1" fill="none"/></svg></WinCtl>
          <WinCtl close><svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1" fill="none"/></svg></WinCtl>
        </div>
      </div>
      {/* Content */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', overflow: 'hidden' }}>
        {children}
      </div>
    </div>
  );
}

function WinCtl({ children, close }) {
  return (
    <div style={{
      width: 46, display: 'flex', alignItems: 'center', justifyContent: 'center',
      color: close ? '#1A1A1A' : '#3D3B45',
    }}>{children}</div>
  );
}

// ─────────────────────────────────────────────────────────────
// App logo — stacked layered "V" mark
// ─────────────────────────────────────────────────────────────
function VLogo({ size = 16, color = vTokens.brand }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <path d="M2.5 3h3l2.5 6 2.5-6h3L9.5 13h-3z" fill={color} />
      <path d="M5.5 3l2.5 6L10.5 3" stroke="rgba(255,255,255,0.65)" strokeWidth="0.8" fill="none" />
    </svg>
  );
}

Object.assign(window, { MacFrame, WinFrame, VLogo });
