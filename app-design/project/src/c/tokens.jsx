// c-tokens.jsx — Design tokens for 「原本」C-end app
// Cool, calm, minimal. Office-pro vibe. Plenty of whitespace.

const C = {
  font: '"Inter", "Noto Sans SC", -apple-system, BlinkMacSystemFont, "PingFang SC", "Microsoft YaHei UI", "Segoe UI", system-ui, sans-serif',
  serif: '"Noto Serif SC", "Songti SC", "STSong", Georgia, serif',
  mono: '"JetBrains Mono", "SF Mono", "Cascadia Code", Menlo, Consolas, monospace',

  // Calm neutrals — slight warm white, slate ink
  ink:    '#16161B',
  ink2:   '#3A3A44',
  ink3:   '#6B6B76',
  ink4:   '#A0A0AB',
  ink5:   '#CECED5',
  line:   '#ECEAE3',
  line2:  '#F4F2EB',
  paper:  '#FAFAF7',
  paper2: '#F4F2EB',
  paper3: '#EEEBE2',
  white:  '#FFFFFF',

  // Primary — muted indigo (calm, professional)
  brand:     '#4F58A8',
  brandSoft: '#EEEFF9',
  brandInk:  '#363D7A',

  // Semantic — desaturated
  ok:        '#3A8F5F',
  okSoft:    '#E4F0E8',
  warn:      '#C28430',
  warnSoft:  '#F7ECD7',
  danger:    '#B54545',
  dangerSoft:'#F6E2E2',

  // Match severity (calm coral/amber on cream)
  hi1: '#E3C28A',  hi1Soft: '#FAF1DC',  // low
  hi2: '#E0A064',  hi2Soft: '#F8E3CB',  // mid
  hi3: '#D67A5E',  hi3Soft: '#F6D4C8',  // high
  hi4: '#B85546',  hi4Soft: '#F1C7C2',  // critical

  // Doc-type chips — muted
  docDocx: { fg: '#1F4D8F', bg: '#E5EDF8', label: 'DOCX' },
  docPdf:  { fg: '#9A3838', bg: '#F4DDDD', label: 'PDF'  },
  docTxt:  { fg: '#5E5651', bg: '#ECE6DD', label: 'TXT'  },
  docMd:   { fg: '#3A3A44', bg: '#E8E5DC', label: 'MD'   },
  docPpt:  { fg: '#A26425', bg: '#F6E4CC', label: 'PPT'  },
  docXls:  { fg: '#1F6E3D', bg: '#DCEFE0', label: 'XLSX' },

  shadow: {
    xs: '0 1px 0 rgba(20,18,14,0.04)',
    sm: '0 1px 2px rgba(20,18,14,0.05), 0 1px 1px rgba(20,18,14,0.03)',
    md: '0 6px 18px rgba(20,18,14,0.06), 0 1px 3px rgba(20,18,14,0.04)',
    lg: '0 16px 42px rgba(20,18,14,0.08), 0 2px 6px rgba(20,18,14,0.04)',
  },

  radius: { sm: 4, md: 6, lg: 8, xl: 12, xxl: 16, pill: 999 },
};

// Compact line-art icon set
const CIcon = ({ name, size = 16, strokeWidth = 1.6, style }) => {
  const paths = {
    upload:  <><path d="M8 11V3M8 3L5.5 5.5M8 3L10.5 5.5" /><path d="M3 11v1.5A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V11" /></>,
    file:    <><path d="M4 2.5A1.5 1.5 0 0 1 5.5 1h4L13 4.5v8A1.5 1.5 0 0 1 11.5 14h-6A1.5 1.5 0 0 1 4 12.5z" /><path d="M9.5 1v3.5H13" /></>,
    folder:  <><path d="M2 5.5A1.5 1.5 0 0 1 3.5 4h2.4l1.2 1.5h5.4A1.5 1.5 0 0 1 14 7v4.5A1.5 1.5 0 0 1 12.5 13h-9A1.5 1.5 0 0 1 2 11.5z" /></>,
    search:  <><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5l3 3" /></>,
    cog:     <><circle cx="8" cy="8" r="2" /><path d="M8 1.5v2M8 12.5v2M14.5 8h-2M3.5 8h-2M12.6 3.4l-1.4 1.4M4.8 11.2L3.4 12.6M12.6 12.6l-1.4-1.4M4.8 4.8L3.4 3.4" /></>,
    home:    <><path d="M2.5 7L8 2.5 13.5 7v6A.5.5 0 0 1 13 13.5h-3v-4h-4v4h-3A.5.5 0 0 1 2.5 13z" /></>,
    history: <><path d="M3 8a5 5 0 1 0 1.4-3.5L3 6M3 3v3h3M8 5.5V8l2 1.5" /></>,
    check:   <><path d="M3 8.5l3 3 7-7" /></>,
    plus:    <><path d="M8 3.5v9M3.5 8h9" /></>,
    x:       <><path d="M4 4l8 8M12 4l-8 8" /></>,
    chevR:   <><path d="M6 3l4 5-4 5" /></>,
    chevD:   <><path d="M3 6l5 4 5-4" /></>,
    chevL:   <><path d="M10 3L6 8l4 5" /></>,
    sun:     <><circle cx="8" cy="8" r="3" /><path d="M8 1.5v1.5M8 13v1.5M14.5 8H13M3 8H1.5M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1M12.6 12.6l-1.1-1.1M4.5 4.5L3.4 3.4" /></>,
    moon:    <><path d="M13 9.5a5.5 5.5 0 0 1-6.5-7 5.5 5.5 0 1 0 6.5 7z" /></>,
    download:<><path d="M8 2v8M8 10l-3-3M8 10l3-3M3 12h10" /></>,
    share:   <><path d="M4 8v4a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V8M8 9V2M5.5 4.5L8 2l2.5 2.5" /></>,
    ai:      <><path d="M5 2l.8 2.2L8 5l-2.2.8L5 8l-.8-2.2L2 5l2.2-.8zM11 7l.6 1.7L13 9l-1.7.6L11 11l-.6-1.7L9 9l1.7-.6z" fill="currentColor" stroke="none" /></>,
    eye:     <><path d="M1 8s2.5-4.5 7-4.5S15 8 15 8s-2.5 4.5-7 4.5S1 8 1 8z" /><circle cx="8" cy="8" r="2" /></>,
    sliders: <><path d="M3 4h6M11 4h2M3 8h2M7 8h6M3 12h8M13 12h0" /><circle cx="10" cy="4" r="1.2" /><circle cx="6"  cy="8" r="1.2" /><circle cx="12" cy="12" r="1.2" /></>,
    lock:    <><rect x="3" y="7" width="10" height="7" rx="1" /><path d="M5 7V5a3 3 0 0 1 6 0v2" /></>,
    filter:  <><path d="M2 3h12l-4.5 5.5V13l-3-1.5V8.5z" /></>,
    sort:    <><path d="M4 3v10M4 13l-2-2M4 13l2-2M12 13V3M12 3l-2 2M12 3l2 2" /></>,
    book:    <><path d="M2.5 3v10A1.5 1.5 0 0 0 4 14.5h9.5V3.5A1.5 1.5 0 0 0 12 2H4a1.5 1.5 0 0 0-1.5 1.5z" /><path d="M2.5 12.5A1.5 1.5 0 0 1 4 11h9.5" /></>,
    info:    <><circle cx="8" cy="8" r="6.5" /><path d="M8 7v4M8 5v.5" /></>,
    arrowR:  <><path d="M3 8h10M9 4l4 4-4 4" /></>,
    refresh: <><path d="M13.5 8.5A5.5 5.5 0 1 1 12 4.7M13 1.5V5h-3.5" /></>,
    user:    <><circle cx="8" cy="5.5" r="2.5" /><path d="M3 13.5c.5-2.5 2.5-4 5-4s4.5 1.5 5 4" /></>,
    paperclip: <><path d="M11 5l-5 5a2 2 0 0 0 2.8 2.8L14 7.5a3.5 3.5 0 0 0-5-5l-6 6a5 5 0 0 0 7 7l4-4" /></>,
    quote:   <><path d="M4 9c0-2 1-3 2.5-3.5L6 4C3.5 4.5 2 6.5 2 9.5V12h4V9zM10 9c0-2 1-3 2.5-3.5L12 4c-2.5.5-4 2.5-4 5.5V12h4V9z" fill="currentColor" stroke="none" /></>,
    diff:    <><path d="M5 2v8M5 10a2 2 0 0 0 2 2h2M11 14V6M11 6a2 2 0 0 0-2-2H7" /><circle cx="5" cy="2" r="1" fill="currentColor"/><circle cx="11" cy="14" r="1" fill="currentColor"/></>,
    sparkle: <><path d="M8 2l1.3 3.7L13 7l-3.7 1.3L8 12 6.7 8.3 3 7l3.7-1.3z" /></>,
  };
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth={strokeWidth}
      strokeLinecap="round" strokeLinejoin="round"
      style={{ flexShrink: 0, ...style }}>
      {paths[name] || null}
    </svg>
  );
};

// Logo: ascending serif "原" mark in a soft mark
function CLogo({ size = 16, color = C.brand }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
      <rect x="1" y="1" width="14" height="14" rx="3.5" fill={color} />
      <text x="8" y="12.4" textAnchor="middle"
        fontFamily="'Noto Serif SC', Georgia, serif" fontWeight="700"
        fontSize="10.5" fill="white" letterSpacing="-0.03em">原</text>
    </svg>
  );
}

Object.assign(window, { C, CIcon, CLogo });
