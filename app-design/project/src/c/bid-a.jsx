// bid-a.jsx — Sidebar (bid-flavored) + Home / Tasks / Scan screens

// ─────────────────────────────────────────────────────────────
// Bid-flavored sidebar — task-centric nav, no AI rewrite
// ─────────────────────────────────────────────────────────────
function BidSidebar({ active = 'home', dark = false, accent = C.brand, layout = 'comfort' }) {
  const compact = layout === 'compact';
  const bg = dark ? '#15151B' : C.paper2;
  const border = dark ? 'rgba(255,255,255,0.06)' : C.line;
  const inkMute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const inkStrong = dark ? '#fff' : C.ink;

  const nav = (k, label, icon, badge) => (
    <CNavRow key={k} active={active === k} dark={dark} accent={accent}
      label={compact ? '' : label} icon={icon} compact={compact} />
  );

  return (
    <div style={{
      width: compact ? 60 : 216, flexShrink: 0,
      background: bg, borderRight: `1px solid ${border}`,
      display: 'flex', flexDirection: 'column', color: inkStrong,
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
              Verum · 标书查重
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
            <span>新建查重任务</span>
            <span style={{ marginLeft: 'auto', fontSize: 10.5, opacity: 0.75, fontFamily: C.mono }}>⌘N</span>
          </>}
        </button>
      </div>

      <div style={{ padding: '0 8px', display: 'flex', flexDirection: 'column', gap: 1 }}>
        {nav('home',    '首页',     'home')}
        {nav('tasks',   '我的任务', 'folder')}
        {nav('history', '历史记录', 'history')}
      </div>
      <CSidebarLabel collapsed={compact} dark={dark}>工具</CSidebarLabel>
      <div style={{ padding: '0 8px', display: 'flex', flexDirection: 'column', gap: 1 }}>
        {nav('library', '查重源', 'book')}
      </div>

      <div style={{ flex: 1 }} />

      {/* Quota — quiet */}
      {!compact && (
        <div style={{
          margin: '8px 12px 0', padding: '10px 12px', borderRadius: 8,
          background: dark ? 'rgba(255,255,255,0.04)' : C.white,
          border: `1px solid ${border}`,
        }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: inkStrong }}>本月剩余</div>
          <div style={{
            fontSize: 18, fontWeight: 700, color: inkStrong,
            letterSpacing: '-0.014em', marginTop: 3, fontFamily: C.font,
          }}>
            5<span style={{ fontSize: 11, color: inkMute, fontWeight: 500, marginLeft: 4 }}>/ 8 次</span>
          </div>
          <div style={{ height: 3, background: dark ? 'rgba(255,255,255,0.08)' : C.paper3, borderRadius: 2, marginTop: 8, overflow: 'hidden' }}>
            <div style={{ height: '100%', width: '62%', background: accent }} />
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
            <CAvatar name="周" size={24} color="#7B8FE5" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11.5, fontWeight: 600, color: inkStrong, lineHeight: 1.2 }}>
                周明远
              </div>
              <div style={{ fontSize: 10, color: inkMute, marginTop: 1 }}>
                采购管理 · 评审专员
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// helper from shell — copy here to avoid coupling
function CNavRow({ active, label, icon, dark, accent, compact }) {
  const activeBg = dark ? 'rgba(255,255,255,0.06)' : C.white;
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 9,
      height: 30, padding: compact ? 0 : '0 10px', borderRadius: 7,
      justifyContent: compact ? 'center' : 'flex-start',
      background: active ? activeBg : 'transparent',
      boxShadow: active && !dark ? `0 1px 0 ${C.line}` : 'none',
      color: active ? (dark ? '#fff' : C.ink) : (dark ? 'rgba(255,255,255,0.7)' : C.ink2),
      cursor: 'pointer', position: 'relative',
    }}>
      <CIcon name={icon} size={14} style={{ color: active ? accent : (dark ? 'rgba(255,255,255,0.55)' : C.ink3) }} />
      {label && <span style={{ flex: 1, fontSize: 12.5, fontWeight: active ? 600 : 500 }}>{label}</span>}
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


// ═══════════════════════════════════════════════════════════════
// SCREEN 1 · Home — 2-5 file slots + task settings
// ═══════════════════════════════════════════════════════════════
function BidScrHome({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="新建查重任务"
        sub="选择 2 至 5 份标书进行交叉比对"
        actions={<CButton kind="ghost" size="md" icon="info" dark={dark}>如何识别围标?</CButton>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '28px 48px 40px' }}>
        <div style={{ maxWidth: 1080, margin: '0 auto' }}>
          {/* Headline */}
          <div style={{ marginBottom: 24 }}>
            <div style={{ fontSize: 11, color: accent, fontWeight: 700, letterSpacing: '0.08em', textTransform: 'uppercase' }}>
              第 047 号评审 · 2026 年 5 月
            </div>
            <div style={{
              fontSize: 26, fontWeight: 700, color: ink, marginTop: 8,
              letterSpacing: '-0.018em', lineHeight: 1.25, fontFamily: C.serif,
            }}>
              把候选的几份标书,一起摆在桌上看。
            </div>
            <div style={{ fontSize: 13, color: mute, marginTop: 8, lineHeight: 1.6 }}>
              在 2 至 5 份待审标书之间,识别条款级的雷同片段、共用模板与围标嫌疑。
              所有比对在你的电脑上本地完成,不上传任何文件。
            </div>
          </div>

          {/* Task name */}
          <div style={{
            background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
            padding: '14px 18px', display: 'flex', alignItems: 'center', gap: 14, marginBottom: 16,
          }}>
            <div style={{ fontSize: 10.5, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>
              任务名称
            </div>
            <div style={{ flex: 1, fontSize: 14, fontWeight: 600, color: ink, letterSpacing: '-0.005em' }}>
              市政信息化平台采购 · 5 家供应商围标核查
            </div>
            <CPill bg={dark ? 'rgba(255,255,255,0.06)' : C.paper2} fg={mute} size={11}>
              智慧城市建设
            </CPill>
            <CIcon name="sliders" size={14} style={{ color: mute, cursor: 'pointer' }} />
          </div>

          {/* 5 slots row */}
          <div style={{ fontSize: 12, fontWeight: 600, color: ink, marginBottom: 10, display: 'flex', alignItems: 'center', gap: 8 }}>
            <span>候选标书</span>
            <CPill bg={C.brandSoft} fg={accent} size={10}>4 / 5</CPill>
            <span style={{ fontSize: 11, color: mute, fontWeight: 500 }}>至少 2 份,最多 5 份</span>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(5, 1fr)', gap: 12 }}>
            {BID_SLOTS.map((s, i) => (
              <BidSlot key={i} s={s} idx={i} dark={dark} cardBg={cardBg} border={border} ink={ink} mute={mute} accent={accent} />
            ))}
          </div>

          {/* Settings + start */}
          <div style={{ display: 'grid', gridTemplateColumns: '1.4fr 1fr', gap: 16, marginTop: 22 }}>
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 14 }}>检测设置</div>
              <SettingsRow label="比对范围" sub="对所选 4 份标书两两比对,共 6 对" dark={dark} ink={ink} mute={mute}>
                <CSegControl options={['完整文档', '仅技术标', '仅商务标']} value={0} accent={accent} dark={dark} />
              </SettingsRow>
              <SettingsRow label="最低相似度阈值" sub="低于此值的片段不进入报告" dark={dark} ink={ink} mute={mute}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <div style={{ width: 140, height: 3, background: dark ? 'rgba(255,255,255,0.08)' : C.paper3, borderRadius: 2, position: 'relative' }}>
                    <div style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: '35%', background: accent, borderRadius: 2 }} />
                    <div style={{
                      position: 'absolute', left: '35%', top: '50%', transform: 'translate(-50%,-50%)',
                      width: 12, height: 12, borderRadius: '50%', background: '#fff',
                      boxShadow: `0 0 0 1.5px ${accent}, 0 1px 4px rgba(0,0,0,0.15)`,
                    }} />
                  </div>
                  <span style={{ fontSize: 12, color: ink, fontFamily: C.mono, fontWeight: 600, minWidth: 28 }}>35%</span>
                </div>
              </SettingsRow>
              <SettingsRow label="忽略通用模板段落" sub="标准条款、表头、附件目录" dark={dark} ink={ink} mute={mute}>
                <CToggle on accent={accent} />
              </SettingsRow>
              <SettingsRow label="围标嫌疑提示" sub="3 份及以上共同高相似片段触发" dark={dark} ink={ink} mute={mute} last>
                <CToggle on accent={accent} />
              </SettingsRow>
            </div>
            <div style={{
              background: dark ? 'rgba(79,88,168,0.10)' : `${accent}08`,
              border: `1px solid ${accent}33`, borderRadius: 12, padding: 18,
              display: 'flex', flexDirection: 'column',
            }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: accent, letterSpacing: '0.08em', textTransform: 'uppercase' }}>
                预计耗时
              </div>
              <div style={{
                fontSize: 32, fontWeight: 700, color: ink,
                letterSpacing: '-0.022em', marginTop: 5, fontFamily: C.font,
              }}>
                3<span style={{ fontSize: 14, color: mute, fontWeight: 500, marginLeft: 4 }}>分</span>
                <span style={{ marginLeft: 8 }}>42</span><span style={{ fontSize: 14, color: mute, fontWeight: 500, marginLeft: 4 }}>秒</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 14, fontSize: 11.5, color: mute }}>
                <span>4 份文档 · 共 286 页</span>
                <span style={{ fontFamily: C.mono }}>6 对比对</span>
              </div>
              <div style={{ flex: 1 }} />
              <CButton kind="primary" size="lg" accent={accent} icon="sparkle" style={{ marginTop: 14, width: '100%', justifyContent: 'center' }}>
                开始查重
              </CButton>
              <div style={{ fontSize: 10.5, color: mute, textAlign: 'center', marginTop: 10, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 5 }}>
                <CIcon name="lock" size={11} />本地完成 · 不上传任何文件
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

const BID_SLOTS = [
  { state: 'filled', name: '智慧城邦科技_技术响应文件.pdf', size: '12.4 MB', pages: 86, type: 'pdf' },
  { state: 'filled', name: '启明信息_投标文件_技术标.docx', size: '8.6 MB',  pages: 72, type: 'docx' },
  { state: 'filled', name: '鸿信科技_市政平台投标书.pdf',    size: '14.2 MB', pages: 92, type: 'pdf' },
  { state: 'filled', name: '蓝信电子_技术标响应.docx',       size: '6.3 MB',  pages: 38, type: 'docx' },
  { state: 'empty',  label: '可选 · 第 5 份' },
];

function BidSlot({ s, idx, dark, cardBg, border, ink, mute, accent }) {
  if (s.state === 'empty') {
    return (
      <div style={{
        background: dark ? 'rgba(255,255,255,0.02)' : C.white,
        border: `1.5px dashed ${dark ? 'rgba(255,255,255,0.15)' : C.ink5}`,
        borderRadius: 12, padding: '20px 14px',
        display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
        gap: 8, minHeight: 142, cursor: 'pointer',
      }}>
        <div style={{
          width: 36, height: 36, borderRadius: 10, background: dark ? 'rgba(255,255,255,0.04)' : C.paper2,
          display: 'flex', alignItems: 'center', justifyContent: 'center', color: mute,
        }}>
          <CIcon name="plus" size={18} />
        </div>
        <div style={{ fontSize: 11.5, color: mute, textAlign: 'center', fontWeight: 500 }}>
          {s.label}<br/>
          <span style={{ fontSize: 10.5 }}>拖入文件或点击</span>
        </div>
      </div>
    );
  }
  // labels: 甲乙丙丁戊
  const tags = ['甲', '乙', '丙', '丁', '戊'];
  const palette = ['#4F58A8', '#0E9A8F', '#C28430', '#B54545', '#7C3AED'];
  return (
    <div style={{
      background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
      padding: '14px 14px', minHeight: 142,
      display: 'flex', flexDirection: 'column', position: 'relative',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
        <div style={{
          width: 20, height: 20, borderRadius: 5,
          background: palette[idx], color: '#fff',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontSize: 11, fontWeight: 700, fontFamily: C.serif,
        }}>{tags[idx]}</div>
        <CDocChip type={s.type} />
        <div style={{ flex: 1 }} />
        <CIcon name="x" size={12} style={{ color: mute, cursor: 'pointer' }} />
      </div>
      <div style={{
        fontSize: 11.5, fontWeight: 600, color: ink, marginTop: 9, lineHeight: 1.4,
        display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden',
      }}>{s.name}</div>
      <div style={{ flex: 1 }} />
      <div style={{ fontSize: 10.5, color: mute, marginTop: 8, display: 'flex', justifyContent: 'space-between' }}>
        <span>{s.pages} 页</span>
        <span style={{ fontFamily: C.mono }}>{s.size}</span>
      </div>
    </div>
  );
}

// SettingsRow reused from screens.jsx
function SettingsRow({ label, sub, children, dark, ink, mute, last }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 16, padding: '12px 0',
      borderBottom: last ? 'none' : `1px solid ${dark ? 'rgba(255,255,255,0.06)' : C.line}`,
    }}>
      <div style={{ flex: 1 }}>
        <div style={{ fontSize: 12.5, fontWeight: 600, color: ink, letterSpacing: '-0.005em' }}>{label}</div>
        {sub && <div style={{ fontSize: 11, color: mute, marginTop: 3 }}>{sub}</div>}
      </div>
      {children}
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 2 · Tasks List — task history with mini matrix preview
// ═══════════════════════════════════════════════════════════════
function BidScrTasks({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="我的任务"
        sub="共 24 个查重任务 · 本月新增 7 个"
        actions={<>
          <CButton kind="ghost" size="md" icon="filter" dark={dark}>筛选</CButton>
          <CButton kind="primary" size="md" accent={accent} icon="plus">新建任务</CButton>
        </>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '24px 48px 40px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 16 }}>
          <CSegControl options={['全部', '进行中', '需复核', '已完成']} value={0} accent={accent} dark={dark} />
          <div style={{ flex: 1 }} />
          <CPill bg={dark ? 'rgba(255,255,255,0.04)' : C.white} fg={mute} style={{ border: `1px solid ${border}`, padding: '4px 10px' }}>
            最近 30 天
          </CPill>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {BID_TASKS.map((t, i) => (
            <BidTaskRow key={i} t={t} dark={dark} cardBg={cardBg} border={border} ink={ink} mute={mute} accent={accent} />
          ))}
        </div>
      </div>
    </div>
  );
}

const BID_TASKS = [
  {
    name: '市政信息化平台采购 · 5 家供应商围标核查',
    sub: '4 份标书 · 6 对比对 · 进行中',
    docs: ['甲', '乙', '丙', '丁'],
    matrix: [
      [1, 0.92, 0.34, 0.42],
      [0.92, 1, 0.31, 0.40],
      [0.34, 0.31, 1, 0.68],
      [0.42, 0.40, 0.68, 1],
    ],
    peak: 92, hint: '甲乙疑似围标',
    status: 'running', progress: 0.62, time: '2 分钟前',
  },
  {
    name: '高校实验室设备采购 · 三厂家技术响应',
    sub: '3 份标书 · 3 对比对 · 已完成',
    docs: ['甲', '乙', '丙'],
    matrix: [
      [1, 0.41, 0.28],
      [0.41, 1, 0.35],
      [0.28, 0.35, 1],
    ],
    peak: 41, hint: '相似度正常',
    status: 'done', progress: 1, time: '昨日 16:18',
  },
  {
    name: '智慧园区集成项目 · 4 家集成商',
    sub: '4 份标书 · 6 对比对 · 需复核',
    docs: ['甲', '乙', '丙', '丁'],
    matrix: [
      [1, 0.58, 0.74, 0.41],
      [0.58, 1, 0.62, 0.38],
      [0.74, 0.62, 1, 0.46],
      [0.41, 0.38, 0.46, 1],
    ],
    peak: 74, hint: '甲丙高相似',
    status: 'review', progress: 1, time: '5 月 22 日',
  },
  {
    name: '政府云一期项目 · 双供应商技术对比',
    sub: '2 份标书 · 1 对比对 · 已完成',
    docs: ['甲', '乙'],
    matrix: [[1, 0.18], [0.18, 1]],
    peak: 18, hint: '差异充分',
    status: 'done', progress: 1, time: '5 月 18 日',
  },
  {
    name: '数据中心建设 · 3 家集成商投标',
    sub: '3 份标书 · 3 对比对 · 已完成',
    docs: ['甲', '乙', '丙'],
    matrix: [
      [1, 0.86, 0.81],
      [0.86, 1, 0.79],
      [0.81, 0.79, 1],
    ],
    peak: 86, hint: '三方共用模板',
    status: 'done', progress: 1, time: '5 月 14 日',
  },
];

function BidTaskRow({ t, dark, cardBg, border, ink, mute, accent }) {
  const sev = t.peak >= 80 ? 'high' : t.peak >= 60 ? 'mid' : 'low';
  const sevColor = sev === 'high' ? C.hi3 : sev === 'mid' ? C.hi2 : C.ok;
  const sevLabel = sev === 'high' ? '高相似' : sev === 'mid' ? '中相似' : '低相似';
  const statusMeta = {
    running: { label: '进行中', fg: accent, bg: C.brandSoft },
    review:  { label: '需复核', fg: C.warn, bg: C.warnSoft },
    done:    { label: '已完成', fg: C.ok,   bg: C.okSoft },
  }[t.status];

  return (
    <div style={{
      background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
      padding: '16px 20px', display: 'grid',
      gridTemplateColumns: '160px 1fr 140px 110px 110px 24px',
      gap: 18, alignItems: 'center', cursor: 'pointer',
    }}>
      {/* mini matrix */}
      <MiniMatrix m={t.matrix} dark={dark} />
      <div>
        <div style={{ fontSize: 13.5, fontWeight: 700, color: ink, letterSpacing: '-0.005em', lineHeight: 1.35 }}>
          {t.name}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 9, marginTop: 6 }}>
          <span style={{ fontSize: 11, color: mute }}>{t.sub}</span>
          <span style={{ fontSize: 11, color: mute }}>·</span>
          <span style={{ fontSize: 11, color: mute }}>{t.time}</span>
          {t.status === 'running' && (
            <div style={{ marginLeft: 4, width: 100, height: 3, background: dark ? 'rgba(255,255,255,0.05)' : C.paper2, borderRadius: 2, overflow: 'hidden' }}>
              <div style={{ height: '100%', width: `${t.progress * 100}%`, background: accent }} />
            </div>
          )}
        </div>
      </div>
      <CPill bg={sev === 'high' ? C.dangerSoft : sev === 'mid' ? C.warnSoft : C.okSoft} fg={sevColor} size={11}>
        <CIcon name="info" size={10} />{t.hint}
      </CPill>
      <div style={{ textAlign: 'right' }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: sevColor, letterSpacing: '-0.012em', fontFamily: C.font, lineHeight: 1 }}>
          {t.peak}<span style={{ fontSize: 11, color: mute, fontWeight: 500 }}>%</span>
        </div>
        <div style={{ fontSize: 10.5, color: mute, marginTop: 3 }}>{sevLabel} · 峰值</div>
      </div>
      <CPill bg={statusMeta.bg} fg={statusMeta.fg} size={11}>
        <span style={{ width: 5, height: 5, borderRadius: '50%', background: statusMeta.fg }} />
        {statusMeta.label}
      </CPill>
      <CIcon name="chevR" size={13} style={{ color: mute }} />
    </div>
  );
}

function MiniMatrix({ m, dark }) {
  const n = m.length;
  const cellColor = (v) => {
    if (v >= 0.9) return C.hi4;
    if (v >= 0.7) return C.hi3;
    if (v >= 0.5) return C.hi2;
    if (v >= 0.3) return C.hi1;
    return dark ? 'rgba(255,255,255,0.08)' : C.paper3;
  };
  return (
    <div style={{
      display: 'grid', gridTemplateColumns: `repeat(${n}, 1fr)`, gap: 2,
      width: 90, height: 90, padding: 4, borderRadius: 6,
      background: dark ? 'rgba(255,255,255,0.025)' : C.paper2,
    }}>
      {m.map((row, r) => row.map((v, c) => (
        <div key={`${r}-${c}`} style={{
          aspectRatio: '1/1', borderRadius: 2,
          background: r === c ? (dark ? 'rgba(255,255,255,0.10)' : C.ink5) : cellColor(v),
        }} />
      )))}
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 3 · Scan — multi-doc parallel pipeline with matrix building
// ═══════════════════════════════════════════════════════════════
function BidScrScan({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  // Building matrix — some cells done, some pending
  const buildingMatrix = [
    [null, 0.92, 0.34, 0.42],
    [0.92, null, 0.31, null],
    [0.34, 0.31, null, null],
    [0.42, null, null, null],
  ];

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="正在比对"
        sub="市政信息化平台采购 · 5 家供应商围标核查"
        actions={<><CButton kind="ghost" size="md" dark={dark}>暂停</CButton>
                   <CButton kind="secondary" size="md" dark={dark}>取消</CButton></>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '24px 48px 40px' }}>
        <div style={{ maxWidth: 1080, margin: '0 auto', display: 'grid', gridTemplateColumns: '1.3fr 1fr', gap: 16 }}>
          {/* Left: per-doc progress */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 20 }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 16 }}>各份标书读取进度</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
              {[
                { tag: '甲', name: '智慧城邦科技_技术响应文件.pdf', pct: 100, color: '#4F58A8' },
                { tag: '乙', name: '启明信息_投标文件_技术标.docx', pct: 100, color: '#0E9A8F' },
                { tag: '丙', name: '鸿信科技_市政平台投标书.pdf',    pct: 84,  color: '#C28430' },
                { tag: '丁', name: '蓝信电子_技术标响应.docx',       pct: 38,  color: '#B54545' },
              ].map((d, i) => (
                <div key={i}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 9, marginBottom: 6 }}>
                    <div style={{
                      width: 18, height: 18, borderRadius: 5,
                      background: d.color, color: '#fff',
                      display: 'flex', alignItems: 'center', justifyContent: 'center',
                      fontSize: 10, fontWeight: 700, fontFamily: C.serif,
                    }}>{d.tag}</div>
                    <span style={{ fontSize: 12, color: ink, fontWeight: 500, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{d.name}</span>
                    <span style={{ fontSize: 11.5, color: d.pct === 100 ? C.ok : ink, fontWeight: 700, fontFamily: C.mono, minWidth: 36, textAlign: 'right' }}>
                      {d.pct === 100 ? '✓' : `${d.pct}%`}
                    </span>
                  </div>
                  <div style={{ height: 4, background: dark ? 'rgba(255,255,255,0.05)' : C.paper2, borderRadius: 2, overflow: 'hidden' }}>
                    <div style={{ height: '100%', width: `${d.pct}%`, background: d.pct === 100 ? C.ok : accent, transition: 'width 0.4s' }} />
                  </div>
                </div>
              ))}
            </div>

            <div style={{ marginTop: 22, paddingTop: 18, borderTop: `1px solid ${border}` }}>
              <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 12 }}>处理阶段</div>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
                {[
                  { label: '解析文档', status: 'done' },
                  { label: '段落语义化', status: 'done' },
                  { label: '两两交叉比对', status: 'running', pct: 62 },
                  { label: '聚合 & 围标识别', status: 'pending' },
                ].map((s, i) => <ScanStep key={i} s={s} dark={dark} accent={accent} ink={ink} mute={mute} border={border} />)}
              </div>
            </div>
          </div>

          {/* Right: matrix building animation */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 20 }}>
            <div style={{ display: 'flex', alignItems: 'baseline' }}>
              <span style={{ fontSize: 13, fontWeight: 700, color: ink }}>相似度矩阵 · 正在生成</span>
              <span style={{ fontSize: 11, color: mute, marginLeft: 8 }}>已完成 4 / 6 对</span>
            </div>
            <div style={{ marginTop: 16 }}>
              <BuildingMatrix m={buildingMatrix} dark={dark} ink={ink} mute={mute} />
            </div>
            {/* Live findings */}
            <div style={{ marginTop: 16, padding: '12px 14px', borderRadius: 10, background: dark ? 'rgba(255,255,255,0.025)' : C.paper2, border: `1px solid ${border}` }}>
              <div style={{ fontSize: 10.5, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 8 }}>
                已发现 · 14 处提示
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 7 }}>
                {[
                  { pair: '甲 × 乙', text: '§3 技术方案 高度同源', pct: 92, sev: 'high' },
                  { pair: '甲 × 乙', text: '§5 服务承诺 措辞完全一致', pct: 88, sev: 'high' },
                  { pair: '丙 × 丁', text: '§7 实施计划 模板雷同', pct: 68, sev: 'mid' },
                ].map((f, i) => (
                  <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 9, fontSize: 11.5 }}>
                    <span style={{ color: mute, fontFamily: C.serif, fontWeight: 700, width: 60, flexShrink: 0 }}>{f.pair}</span>
                    <span style={{ color: ink, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{f.text}</span>
                    <span style={{ fontSize: 12, fontWeight: 700, color: f.sev === 'high' ? C.hi3 : C.hi2, fontFamily: C.mono }}>{f.pct}%</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>

        {/* Bottom progress card */}
        <div style={{
          maxWidth: 1080, margin: '18px auto 0',
          background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
          padding: '16px 22px', display: 'flex', alignItems: 'center', gap: 22,
        }}>
          <div>
            <div style={{ fontSize: 10.5, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', fontWeight: 700 }}>
              总体进度
            </div>
            <div style={{ fontSize: 22, fontWeight: 700, color: ink, marginTop: 3, fontFamily: C.font, letterSpacing: '-0.014em' }}>
              62<span style={{ fontSize: 12, color: mute, fontWeight: 500 }}>%</span>
            </div>
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ height: 4, background: dark ? 'rgba(255,255,255,0.08)' : C.paper2, borderRadius: 2, overflow: 'hidden' }}>
              <div style={{ height: '100%', width: '62%', background: accent, transition: 'width 0.4s' }} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 8, fontSize: 11, color: mute }}>
              <span>已完成 4 / 6 对比对</span>
              <span>预计剩余 1 分 24 秒</span>
            </div>
          </div>
          <CPill bg={C.brandSoft} fg={accent} size={11}>
            <CIcon name="lock" size={10} />本地比对
          </CPill>
        </div>
      </div>
    </div>
  );
}

function BuildingMatrix({ m, dark, ink, mute }) {
  const tags = ['甲', '乙', '丙', '丁'];
  const cellColor = (v) => {
    if (v >= 0.9) return C.hi4;
    if (v >= 0.7) return C.hi3;
    if (v >= 0.5) return C.hi2;
    if (v >= 0.3) return C.hi1;
    return C.okSoft;
  };
  return (
    <div style={{ display: 'grid', gridTemplateColumns: '32px repeat(4, 1fr)', gap: 4 }}>
      <div />
      {tags.map(t => (
        <div key={t} style={{ textAlign: 'center', fontSize: 11, fontWeight: 700, color: ink, fontFamily: C.serif }}>
          {t}
        </div>
      ))}
      {m.map((row, r) => (
        <React.Fragment key={r}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'flex-end', fontSize: 11, fontWeight: 700, color: ink, fontFamily: C.serif, paddingRight: 4 }}>
            {tags[r]}
          </div>
          {row.map((v, c) => {
            const diag = r === c;
            const computed = v != null;
            return (
              <div key={c} style={{
                aspectRatio: '1.2 / 1', borderRadius: 6,
                background: diag ? (dark ? 'rgba(255,255,255,0.04)' : C.paper2)
                  : computed ? cellColor(v)
                  : (dark ? 'rgba(255,255,255,0.02)' : '#fff'),
                border: !computed && !diag ? `1px dashed ${dark ? 'rgba(255,255,255,0.1)' : C.ink5}` : 'none',
                color: computed && v >= 0.7 ? '#fff' : ink,
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                fontSize: 12, fontWeight: 700, fontFamily: C.mono,
                animation: !computed && !diag ? 'cpulse 1.8s ease-in-out infinite' : 'none',
              }}>
                {diag ? '—' : computed ? `${(v * 100).toFixed(0)}` : '·'}
              </div>
            );
          })}
        </React.Fragment>
      ))}
    </div>
  );
}

function ScanStep({ s, dark, accent, ink, mute, border }) {
  const done = s.status === 'done';
  const running = s.status === 'running';
  return (
    <div style={{
      padding: 12, borderRadius: 8, background: dark ? 'rgba(255,255,255,0.02)' : '#fff',
      border: `1px solid ${border}`,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <div style={{
          width: 16, height: 16, borderRadius: '50%', flexShrink: 0,
          background: done ? C.ok : running ? `${accent}22` : 'transparent',
          border: done ? 'none' : `1.5px solid ${running ? accent : (dark ? 'rgba(255,255,255,0.2)' : C.ink5)}`,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
        }}>
          {done && <CIcon name="check" size={10} style={{ color: '#fff' }} strokeWidth={2.5} />}
          {running && <span style={{ width: 7, height: 7, borderRadius: '50%', background: accent, animation: 'cpulse 1.4s ease-in-out infinite' }} />}
        </div>
        <span style={{ fontSize: 11.5, fontWeight: 600, color: done ? mute : ink, flex: 1 }}>{s.label}</span>
        {running && <span style={{ fontSize: 11, color: accent, fontFamily: C.mono, fontWeight: 700 }}>{s.pct}%</span>}
      </div>
    </div>
  );
}

Object.assign(window, {
  BidSidebar, BidScrHome, BidScrTasks, BidScrScan,
  SettingsRow, MiniMatrix,
});
