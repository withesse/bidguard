// c-frames.jsx — macOS + Windows chrome for 「原本」C-end app

const C_FRAME_FONT = C.font;

function CMacFrame({ width = 1280, height = 820, title = '原本', children, dark = false, accent = C.brand }) {
  const titlebarBg = dark ? '#1F1F26' : '#EFECE5';
  const border = dark ? 'rgba(255,255,255,0.08)' : 'rgba(20,18,14,0.10)';
  const titleColor = dark ? 'rgba(255,255,255,0.7)' : C.ink2;
  return (
    <div style={{
      width, height, borderRadius: 12, overflow: 'hidden',
      background: dark ? '#15151B' : C.paper, fontFamily: C_FRAME_FONT,
      boxShadow: dark
        ? '0 0 0 0.5px rgba(255,255,255,0.10), 0 24px 60px rgba(0,0,0,0.45)'
        : '0 0 0 0.5px rgba(20,18,14,0.16), 0 24px 60px rgba(20,18,14,0.16)',
      display: 'flex', flexDirection: 'column',
    }}>
      <div style={{
        height: 36, flexShrink: 0, background: titlebarBg,
        borderBottom: `0.5px solid ${border}`,
        display: 'flex', alignItems: 'center', padding: '0 12px',
        position: 'relative',
      }}>
        <div style={{ display: 'flex', gap: 8 }}>
          <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#FF5F57', border: '0.5px solid rgba(0,0,0,0.08)' }} />
          <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#FEBC2E', border: '0.5px solid rgba(0,0,0,0.08)' }} />
          <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#28C840', border: '0.5px solid rgba(0,0,0,0.08)' }} />
        </div>
        <div style={{
          position: 'absolute', inset: 0, display: 'flex',
          alignItems: 'center', justifyContent: 'center', pointerEvents: 'none',
        }}>
          <div style={{
            display: 'flex', alignItems: 'center', gap: 7,
            fontSize: 12.5, fontWeight: 600, color: titleColor, letterSpacing: '-0.005em',
          }}>
            <CLogo size={13} color={accent} />
            <span>{title}</span>
          </div>
        </div>
      </div>
      <div style={{ flex: 1, minHeight: 0, display: 'flex', overflow: 'hidden' }}>
        {children}
      </div>
    </div>
  );
}

function CWinFrame({ width = 1280, height = 820, title = '原本', children, dark = false, accent = C.brand }) {
  const titlebarBg = dark ? '#22222A' : '#F4F2EB';
  const border = dark ? 'rgba(255,255,255,0.08)' : 'rgba(20,18,14,0.08)';
  const titleColor = dark ? 'rgba(255,255,255,0.85)' : C.ink;
  return (
    <div style={{
      width, height, borderRadius: 8, overflow: 'hidden',
      background: dark ? '#1B1B22' : C.paper,
      fontFamily: '"Segoe UI Variable Display", "Segoe UI", ' + C_FRAME_FONT,
      boxShadow: dark
        ? '0 0 0 0.5px rgba(255,255,255,0.08), 0 24px 60px rgba(0,0,0,0.45)'
        : '0 0 0 0.5px rgba(20,18,14,0.12), 0 24px 60px rgba(20,18,14,0.14)',
      display: 'flex', flexDirection: 'column',
    }}>
      <div style={{
        height: 36, flexShrink: 0, background: titlebarBg,
        borderBottom: `1px solid ${border}`, display: 'flex', alignItems: 'stretch',
      }}>
        <div style={{
          display: 'flex', alignItems: 'center', gap: 8, padding: '0 12px', flex: 1,
          fontSize: 12, fontWeight: 600, color: titleColor, letterSpacing: '-0.005em',
        }}>
          <CLogo size={14} color={accent} />
          <span>{title}</span>
        </div>
        <div style={{ display: 'flex' }}>
          {[
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" strokeWidth="1" fill="none"/></svg>,
            <svg width="10" height="10" viewBox="0 0 10 10"><rect x="1" y="1" width="8" height="8" stroke="currentColor" strokeWidth="1" fill="none"/></svg>,
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1" fill="none"/></svg>,
          ].map((s, i) => (
            <div key={i} style={{
              width: 46, display: 'flex', alignItems: 'center', justifyContent: 'center',
              color: dark ? 'rgba(255,255,255,0.7)' : C.ink2,
            }}>{s}</div>
          ))}
        </div>
      </div>
      <div style={{ flex: 1, minHeight: 0, display: 'flex', overflow: 'hidden' }}>
        {children}
      </div>
    </div>
  );
}

Object.assign(window, { CMacFrame, CWinFrame });
