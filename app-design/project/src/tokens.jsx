// tokens.jsx — design tokens for Verity 查重工作台
// Loaded as plain script — exposes `vTokens` and helpers on window.

const vTokens = {
  font: '"Inter", -apple-system, BlinkMacSystemFont, "PingFang SC", "Microsoft YaHei UI", "Segoe UI", system-ui, sans-serif',
  mono: '"JetBrains Mono", "SF Mono", "Cascadia Code", Menlo, Consolas, monospace',

  // Neutrals — warm, paper-like
  ink: '#16151A',
  ink2: '#3D3B45',
  ink3: '#65626E',
  ink4: '#9B97A3',
  ink5: '#C9C5CF',
  line: '#E8E4DC',
  line2: '#EFECE5',
  paper: '#FBFAF7',
  paper2: '#F4F2EC',
  paper3: '#EDEAE2',
  white: '#FFFFFF',

  // Brand — indigo violet, confident enterprise
  brand: '#5B5BD6',
  brandSoft: '#EEEFFC',
  brandInk: '#3F3FB3',

  // Semantic
  ok: '#1F9D55',
  okSoft: '#E3F3EA',
  warn: '#D17C00',
  warnSoft: '#FBEFD8',
  danger: '#C42B2B',
  dangerSoft: '#FBE6E6',
  info: '#1B7BCB',
  infoSoft: '#E3F0FB',

  // Highlight scale for duplicate severity
  hi1: '#FBD38D',  // low match — amber pale
  hi2: '#F6AD55',  // medium match — amber
  hi3: '#F08A6A',  // high match — coral
  hi4: '#E15858',  // critical
  hi1Soft: '#FEF3DC',
  hi2Soft: '#FCE5C7',
  hi3Soft: '#FBD9CC',
  hi4Soft: '#F9D2D2',

  // Doc-type chip colors
  docDocx: { fg: '#1B5BB7', bg: '#E3EDFB', label: 'DOCX' },
  docPdf:  { fg: '#B73838', bg: '#FBE3E3', label: 'PDF'  },
  docTxt:  { fg: '#5E5651', bg: '#ECE7E0', label: 'TXT'  },
  docMd:   { fg: '#3D3B45', bg: '#E8E5DE', label: 'MD'   },
  docPpt:  { fg: '#B86A1F', bg: '#FBE9D4', label: 'PPT'  },
  docXls:  { fg: '#1F7A3D', bg: '#DDF1E2', label: 'XLSX' },
  docCode: { fg: '#5C3DB7', bg: '#ECE6FB', label: '.PY'  },

  shadow: {
    sm: '0 1px 2px rgba(20,18,14,0.06), 0 1px 1px rgba(20,18,14,0.04)',
    md: '0 4px 12px rgba(20,18,14,0.06), 0 1px 3px rgba(20,18,14,0.04)',
    lg: '0 10px 30px rgba(20,18,14,0.08), 0 2px 6px rgba(20,18,14,0.04)',
    pop: '0 18px 56px rgba(20,18,14,0.12), 0 2px 8px rgba(20,18,14,0.06)',
  },

  radius: { sm: 4, md: 6, lg: 8, xl: 12, pill: 999 },
};

// Tiny icon set — stroke=currentColor, line-art only.
const VIcon = ({ name, size = 16, strokeWidth = 1.6, style }) => {
  const paths = {
    upload: <><path d="M8 12V4M8 4L5 7M8 4L11 7" /><path d="M3 11v1.5A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V11" /></>,
    folder: <><path d="M2 5.5A1.5 1.5 0 0 1 3.5 4h2.4l1.2 1.5h5.4A1.5 1.5 0 0 1 14 7v4.5A1.5 1.5 0 0 1 12.5 13h-9A1.5 1.5 0 0 1 2 11.5z" /></>,
    doc: <><path d="M4 2.5A1.5 1.5 0 0 1 5.5 1h4L13 4.5v8A1.5 1.5 0 0 1 11.5 14h-6A1.5 1.5 0 0 1 4 12.5z" /><path d="M9.5 1v3.5H13" /></>,
    docs: <><path d="M3.5 3.5A1.5 1.5 0 0 1 5 2h3.5L11 4.5v6A1.5 1.5 0 0 1 9.5 12h-4.5A1.5 1.5 0 0 1 3.5 10.5z" /><path d="M6 13.5A1.5 1.5 0 0 0 7.5 15h4.5a1.5 1.5 0 0 0 1.5-1.5V8" /></>,
    search: <><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5l3 3" /></>,
    cog: <><circle cx="8" cy="8" r="2" /><path d="M8 1.5v2M8 12.5v2M14.5 8h-2M3.5 8h-2M12.6 3.4l-1.4 1.4M4.8 11.2L3.4 12.6M12.6 12.6l-1.4-1.4M4.8 4.8L3.4 3.4" /></>,
    play: <><path d="M5 3.5v9l7-4.5z" fill="currentColor" stroke="none" /></>,
    pause: <><rect x="5" y="3.5" width="2.2" height="9" fill="currentColor" stroke="none" /><rect x="9.2" y="3.5" width="2.2" height="9" fill="currentColor" stroke="none" /></>,
    check: <><path d="M3 8.5l3 3 7-7" /></>,
    plus: <><path d="M8 3.5v9M3.5 8h9" /></>,
    x: <><path d="M4 4l8 8M12 4l-8 8" /></>,
    chevR: <><path d="M6 3l4 5-4 5" /></>,
    chevD: <><path d="M3 6l5 4 5-4" /></>,
    chevL: <><path d="M10 3L6 8l4 5" /></>,
    chevU: <><path d="M3 10l5-4 5 4" /></>,
    bell: <><path d="M4 11V7a4 4 0 0 1 8 0v4M2.5 11h11M6.5 13.5a1.5 1.5 0 0 0 3 0" /></>,
    user: <><circle cx="8" cy="5.5" r="2.5" /><path d="M3 13.5c.5-2.5 2.5-4 5-4s4.5 1.5 5 4" /></>,
    users: <><circle cx="6" cy="5.5" r="2.2" /><path d="M2 13c.4-2.2 2-3.5 4-3.5s3.6 1.3 4 3.5" /><circle cx="11" cy="6" r="1.7" /><path d="M10 9.7c.7-.2 1.3-.2 2.3.1 1.3.5 1.7 1.6 1.7 3.2" /></>,
    history: <><path d="M3 8a5 5 0 1 0 1.4-3.5L3 6M3 3v3h3M8 5.5V8l2 1.5" /></>,
    grid: <><rect x="2.5" y="2.5" width="4.5" height="4.5" /><rect x="9" y="2.5" width="4.5" height="4.5" /><rect x="2.5" y="9" width="4.5" height="4.5" /><rect x="9" y="9" width="4.5" height="4.5" /></>,
    list: <><path d="M3 4h10M3 8h10M3 12h10" /></>,
    sun: <><circle cx="8" cy="8" r="3" /><path d="M8 1.5v1.5M8 13v1.5M14.5 8H13M3 8H1.5M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1M12.6 12.6l-1.1-1.1M4.5 4.5L3.4 3.4" /></>,
    moon: <><path d="M13 9.5a5.5 5.5 0 0 1-6.5-7 5.5 5.5 0 1 0 6.5 7z" /></>,
    download: <><path d="M8 2v8M8 10l-3-3M8 10l3-3" /><path d="M3 12h10" /></>,
    share: <><circle cx="4" cy="8" r="1.8" /><circle cx="12" cy="4" r="1.8" /><circle cx="12" cy="12" r="1.8" /><path d="M5.5 7l5-2.5M5.5 9l5 2.5" /></>,
    comment: <><path d="M2.5 4A1.5 1.5 0 0 1 4 2.5h8A1.5 1.5 0 0 1 13.5 4v5A1.5 1.5 0 0 1 12 10.5H6.5L4 13v-2.5h-.5A1.5 1.5 0 0 1 2 9z" /></>,
    filter: <><path d="M2 3h12l-4.5 5.5V13l-3-1.5V8.5z" /></>,
    sort: <><path d="M4 3v10M4 13l-2-2M4 13l2-2M12 13V3M12 3l-2 2M12 3l2 2" /></>,
    link: <><path d="M9 7l3-3a2.5 2.5 0 0 1 3.5 3.5l-3 3M7 9l-3 3a2.5 2.5 0 0 0 3.5 3.5l3-3" transform="translate(-1 -1.5) scale(.85)" /></>,
    sparkle: <><path d="M8 2l1.3 3.7L13 7l-3.7 1.3L8 12 6.7 8.3 3 7l3.7-1.3z" /></>,
    quote: <><path d="M4 9c0-2 1-3 2.5-3.5L6 4C3.5 4.5 2 6.5 2 9.5V12h4V9zM10 9c0-2 1-3 2.5-3.5L12 4c-2.5.5-4 2.5-4 5.5V12h4V9z" fill="currentColor" stroke="none" /></>,
    branch: <><circle cx="4" cy="3" r="1.5" /><circle cx="4" cy="13" r="1.5" /><circle cx="12" cy="6" r="1.5" /><path d="M4 4.5v7M4 7c0-1.5 1-2.5 3-2.5h3.5" /></>,
    ai: <><path d="M5 2l.8 2.2L8 5l-2.2.8L5 8l-.8-2.2L2 5l2.2-.8zM11 7l.6 1.7L13 9l-1.7.6L11 11l-.6-1.7L9 9l1.7-.6z" fill="currentColor" stroke="none" /></>,
    warn: <><path d="M8 1.5L15 13.5H1z" /><path d="M8 6v3.5M8 11.5v.5" strokeLinecap="round"/></>,
    eye: <><path d="M1 8s2.5-4.5 7-4.5S15 8 15 8s-2.5 4.5-7 4.5S1 8 1 8z" /><circle cx="8" cy="8" r="2" /></>,
    refresh: <><path d="M13 8a5 5 0 1 1-1.5-3.5M13 2.5V5h-2.5" /></>,
    code: <><path d="M5 5L2 8l3 3M11 5l3 3-3 3M9 4l-2 8" /></>,
    archive: <><rect x="2" y="3" width="12" height="3" /><path d="M3 6v7h10V6M6.5 9h3" /></>,
    panel: <><rect x="2" y="3" width="12" height="10" rx="1.5" /><path d="M6.5 3v10" /></>,
    paperclip: <><path d="M11 5l-5 5a2 2 0 0 0 2.8 2.8L14 7.5a3.5 3.5 0 0 0-5-5l-6 6a5 5 0 0 0 7 7l4-4" /></>,
    lock: <><rect x="3" y="7" width="10" height="7" rx="1" /><path d="M5 7V5a3 3 0 0 1 6 0v2" /></>,
    tag: <><path d="M2 8.5V3h5.5L14 9.5 9.5 14z" /><circle cx="5.5" cy="5.5" r=".8" fill="currentColor"/></>,
    flag: <><path d="M3.5 14V2M3.5 3h9l-2 3 2 3h-9" /></>,
  };
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth={strokeWidth}
      strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0, ...style }}>
      {paths[name] || null}
    </svg>
  );
};

Object.assign(window, { vTokens, VIcon });
