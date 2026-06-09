// screens-a.jsx — Import, Tasks/History, Progress, Overview Report

// ═════════════════════════════════════════════════════════════
// SCREEN 1 · Import (拖拽上传)
// ═════════════════════════════════════════════════════════════
function ScrImport({ accent = T.brand, dark = false }) {
  const fileBg = dark ? 'rgba(255,255,255,0.04)' : T.white;
  const fileBorder = dark ? 'rgba(255,255,255,0.08)' : T.line;
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: dark ? '#1A1A1F' : T.paper }}>
      <VTopbar dark={dark}
        crumbs={['工作区', '收件箱', '新建查重任务']}
        actions={<Button kind="secondary" size="md" icon="cog" dark={dark}>任务配置</Button>}
      />
      <div style={{ flex: 1, minHeight: 0, padding: 20, display: 'grid', gridTemplateColumns: '1fr 320px', gap: 16 }}>
        {/* LEFT: dropzone + list */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14, minHeight: 0 }}>
          <div>
            <div style={{ fontSize: 18, fontWeight: 700, color: textInk, letterSpacing: '-0.012em' }}>
              添加待查文档
            </div>
            <div style={{ fontSize: 12.5, color: textMute, marginTop: 3 }}>
              支持 Word · PDF · TXT/Markdown · PPT · Excel · 代码文件,&nbsp;最多 200 份 · 单份 100MB
            </div>
          </div>
          {/* Dropzone */}
          <div style={{
            border: `1.5px dashed ${accent}55`, borderRadius: 12,
            background: dark ? 'rgba(91,91,214,0.06)' : `${accent}08`,
            padding: '28px 24px', display: 'flex', alignItems: 'center', gap: 22,
          }}>
            <div style={{
              width: 56, height: 56, borderRadius: 14, flexShrink: 0,
              background: `${accent}22`, color: accent,
              display: 'flex', alignItems: 'center', justifyContent: 'center',
            }}>
              <VIcon name="upload" size={26} strokeWidth={1.8} />
            </div>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 14, fontWeight: 600, color: textInk }}>拖拽文件或文件夹到此处</div>
              <div style={{ fontSize: 12, color: textMute, marginTop: 4 }}>
                也可粘贴文件路径 · 从 SharePoint / 飞书 / Confluence 同步导入
              </div>
            </div>
            <div style={{ display: 'flex', gap: 8 }}>
              <Button kind="secondary" size="md" icon="folder" dark={dark}>选择文件</Button>
              <Button kind="primary" size="md" icon="link" accent={accent}>从云端导入</Button>
            </div>
          </div>
          {/* File list */}
          <div style={{
            flex: 1, minHeight: 0, background: fileBg, borderRadius: 10,
            border: `1px solid ${fileBorder}`, display: 'flex', flexDirection: 'column',
            overflow: 'hidden',
          }}>
            <div style={{
              height: 36, flexShrink: 0, padding: '0 14px',
              display: 'flex', alignItems: 'center', gap: 10,
              borderBottom: `1px solid ${fileBorder}`,
              background: dark ? 'rgba(255,255,255,0.02)' : T.paper2,
            }}>
              <span style={{ fontSize: 12, fontWeight: 600, color: textInk }}>已添加 6 份</span>
              <span style={{ fontSize: 11, color: textMute }}>·&nbsp;共 18.4 MB</span>
              <div style={{ flex: 1 }} />
              <Pill bg={dark ? 'rgba(255,255,255,0.06)' : T.paper3} fg={textMute}>
                <VIcon name="filter" size={10} />过滤
              </Pill>
              <Pill bg={dark ? 'rgba(255,255,255,0.06)' : T.paper3} fg={textMute}>
                <VIcon name="sort" size={10} />排序
              </Pill>
            </div>
            <div style={{ flex: 1, overflow: 'auto' }}>
              {IMPORT_FILES.map((f, i) => (
                <ImportFileRow key={i} f={f} dark={dark} fileBorder={fileBorder} textInk={textInk} textMute={textMute} accent={accent} />
              ))}
            </div>
          </div>
        </div>

        {/* RIGHT: settings */}
        <ImportSettings accent={accent} dark={dark} textInk={textInk} textMute={textMute} />
      </div>
    </div>
  );
}

const IMPORT_FILES = [
  { type: 'docx', name: '2026Q1-供应商保密协议-华东区v3.docx', size: '2.4 MB', pages: 18, status: 'ready', dept: '法务合规部' },
  { type: 'pdf',  name: '员工手册（含修订条款）2026.pdf',       size: '6.8 MB', pages: 84, status: 'ready', dept: '人力资源部' },
  { type: 'docx', name: '产品白皮书-Verity-初稿.docx',           size: '1.9 MB', pages: 22, status: 'parsing', dept: '市场部' },
  { type: 'pdf',  name: '《数据安全合规白皮书》2026版.pdf',        size: '4.1 MB', pages: 56, status: 'ready', dept: '安全部' },
  { type: 'xls',  name: '合规检查清单_2026Q1.xlsx',              size: '180 KB', pages: 6,  status: 'ready', dept: '法务合规部' },
  { type: 'code', name: 'audit_pipeline.py',                    size: '24 KB',  pages: 1,  status: 'warn',  dept: '工程部',
    warn: '检测到含有大量许可证头注释，可在设置中跳过' },
];

function ImportFileRow({ f, dark, fileBorder, textInk, textMute, accent }) {
  const isParsing = f.status === 'parsing';
  const isWarn = f.status === 'warn';
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 12, padding: '10px 14px',
      borderBottom: `1px solid ${fileBorder}`,
    }}>
      <DocChip type={f.type} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <div style={{
            fontSize: 12.5, fontWeight: 600, color: textInk,
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}>{f.name}</div>
          {isParsing && <Pill bg={`${accent}22`} fg={accent} size={10}><Spinner size={9} color={accent} />解析中</Pill>}
          {isWarn && <Pill bg={T.warnSoft} fg={T.warn} size={10}><VIcon name="warn" size={10} />注意</Pill>}
        </div>
        <div style={{ fontSize: 11, color: textMute, marginTop: 2, display: 'flex', gap: 8 }}>
          <span>{f.pages} 页</span><span>·</span>
          <span>{f.size}</span><span>·</span>
          <span>{f.dept}</span>
          {isWarn && <><span>·</span><span style={{ color: T.warn }}>{f.warn}</span></>}
        </div>
      </div>
      <div style={{ display: 'flex', gap: 4, color: textMute }}>
        <VIcon name="eye" size={14} />
        <div style={{ width: 8 }} />
        <VIcon name="x" size={13} />
      </div>
    </div>
  );
}

function Spinner({ size = 12, color = T.brand }) {
  return (
    <span style={{
      width: size, height: size, borderRadius: '50%', display: 'inline-block',
      border: `1.5px solid ${color}33`, borderTopColor: color,
      animation: 'vspin 0.9s linear infinite',
    }} />
  );
}

function ImportSettings({ accent, dark, textInk, textMute }) {
  const bg = dark ? 'rgba(255,255,255,0.04)' : T.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : T.line;
  return (
    <div style={{
      background: bg, borderRadius: 10, border: `1px solid ${border}`,
      padding: 16, display: 'flex', flexDirection: 'column', gap: 14, overflow: 'auto',
    }}>
      <div style={{ fontSize: 13, fontWeight: 700, color: textInk }}>查重设置</div>

      <SettingField label="查重模式" dark={dark} textMute={textMute}>
        <SegControl options={['两两互查', '与库对比', '一对多']} value={0} accent={accent} dark={dark} />
      </SettingField>

      <SettingField label="对比库" dark={dark} textMute={textMute}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <SourceRow checked label="法务合同库" count="3,482 份" dark={dark} accent={accent} />
          <SourceRow checked label="企业内部白皮书库" count="1,128 份" dark={dark} accent={accent} />
          <SourceRow label="行业公开文献" count="48,209 份" dark={dark} accent={accent} />
          <SourceRow label="历史归档（>2 年）" count="6,914 份" dark={dark} accent={accent} />
        </div>
      </SettingField>

      <SettingField label="最低相似度阈值" dark={dark} textMute={textMute}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <div style={{
              flex: 1, height: 4, background: dark ? 'rgba(255,255,255,0.08)' : T.paper3, borderRadius: 2, position: 'relative',
            }}>
              <div style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: '32%', background: accent, borderRadius: 2 }} />
              <div style={{
                position: 'absolute', left: '32%', top: '50%', transform: 'translate(-50%,-50%)',
                width: 12, height: 12, borderRadius: '50%', background: '#fff',
                boxShadow: `0 0 0 1.5px ${accent}, 0 1px 4px rgba(0,0,0,0.15)`,
              }} />
            </div>
            <span style={{ fontSize: 12, fontWeight: 600, color: textInk, fontFamily: T.mono, minWidth: 32 }}>32%</span>
          </div>
        </div>
      </SettingField>

      <SettingField label="片段最小长度" dark={dark} textMute={textMute}>
        <Stepper value="15 字符" dark={dark} />
      </SettingField>

      <SettingField label="高级选项" dark={dark} textMute={textMute}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 7 }}>
          <Check label="忽略引用块 / 脚注" checked dark={dark} accent={accent} />
          <Check label="跳过通用模板段落" checked dark={dark} accent={accent} />
          <Check label="启用 AI 改写建议" checked dark={dark} accent={accent} />
          <Check label="生成 GB/T 7714 引用" dark={dark} accent={accent} />
        </div>
      </SettingField>

      <div style={{ display: 'flex', gap: 8, marginTop: 'auto', paddingTop: 8 }}>
        <Button kind="secondary" dark={dark} style={{ flex: 1 }}>保存为模板</Button>
        <Button kind="primary" accent={accent} style={{ flex: 1.5 }} icon="play">开始查重</Button>
      </div>
    </div>
  );
}

function SettingField({ label, children, textMute }) {
  return (
    <div>
      <div style={{
        fontSize: 10.5, fontWeight: 600, letterSpacing: '0.06em',
        textTransform: 'uppercase', color: textMute, marginBottom: 7,
      }}>{label}</div>
      {children}
    </div>
  );
}

function SegControl({ options, value, accent, dark }) {
  return (
    <div style={{
      display: 'flex', padding: 2,
      background: dark ? 'rgba(255,255,255,0.05)' : T.paper2,
      border: `1px solid ${dark ? 'rgba(255,255,255,0.08)' : T.line}`,
      borderRadius: 7,
    }}>
      {options.map((o, i) => (
        <div key={i} style={{
          flex: 1, textAlign: 'center', padding: '5px 0',
          fontSize: 11.5, fontWeight: 600,
          background: i === value ? (dark ? '#2A2A33' : '#fff') : 'transparent',
          color: i === value ? (dark ? '#fff' : T.ink) : (dark ? 'rgba(255,255,255,0.55)' : T.ink3),
          borderRadius: 5,
          boxShadow: i === value && !dark ? '0 1px 2px rgba(0,0,0,0.06)' : 'none',
        }}>{o}</div>
      ))}
    </div>
  );
}

function SourceRow({ label, count, checked, dark, accent }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 9, padding: '6px 8px',
      borderRadius: 6, background: checked ? (dark ? 'rgba(91,91,214,0.12)' : `${accent}10`) : 'transparent',
    }}>
      <Check checked={checked} dark={dark} accent={accent} bare />
      <span style={{ fontSize: 12, color: dark ? '#fff' : T.ink, fontWeight: 500 }}>{label}</span>
      <div style={{ flex: 1 }} />
      <span style={{ fontSize: 10.5, color: dark ? 'rgba(255,255,255,0.5)' : T.ink4, fontFamily: T.mono }}>{count}</span>
    </div>
  );
}

function Stepper({ value, dark }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center',
      border: `1px solid ${dark ? 'rgba(255,255,255,0.08)' : T.line}`,
      borderRadius: 6, background: dark ? 'rgba(255,255,255,0.04)' : T.white,
      height: 28,
    }}>
      <div style={{ width: 28, textAlign: 'center', color: dark ? 'rgba(255,255,255,0.5)' : T.ink4 }}>−</div>
      <div style={{ flex: 1, textAlign: 'center', fontSize: 12, color: dark ? '#fff' : T.ink, fontFamily: T.mono }}>{value}</div>
      <div style={{ width: 28, textAlign: 'center', color: dark ? 'rgba(255,255,255,0.5)' : T.ink4 }}>+</div>
    </div>
  );
}

function Check({ checked, label, dark, accent, bare }) {
  const box = (
    <div style={{
      width: 14, height: 14, borderRadius: 4, flexShrink: 0,
      border: `1.5px solid ${checked ? accent : (dark ? 'rgba(255,255,255,0.2)' : T.ink5)}`,
      background: checked ? accent : 'transparent',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
    }}>
      {checked && (
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
          <path d="M2 5l2 2 4-4" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      )}
    </div>
  );
  if (bare) return box;
  return (
    <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
      {box}
      <span style={{ fontSize: 11.5, color: dark ? 'rgba(255,255,255,0.85)' : T.ink2 }}>{label}</span>
    </label>
  );
}


// ═════════════════════════════════════════════════════════════
// SCREEN 2 · Tasks / History
// ═════════════════════════════════════════════════════════════
function ScrTasks({ accent = T.brand, dark = false }) {
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;
  const bg = dark ? '#1A1A1F' : T.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : T.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : T.line;
  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg }}>
      <VTopbar dark={dark}
        crumbs={['工作区', '我的任务']}
        actions={<><Button kind="ghost" icon="filter" dark={dark}>筛选</Button>
                  <Button kind="primary" accent={accent} icon="plus">新建任务</Button></>}
      />

      {/* KPI strip */}
      <div style={{ padding: '16px 20px 0', display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12 }}>
        <KPI label="本月任务"  value="48"  delta="+12%" deltaPositive dark={dark} accent={accent} />
        <KPI label="待处理"    value="6"   sub="2 个高优先级" dark={dark} accent={T.warn} />
        <KPI label="平均相似度" value="34.2%" delta="−3.4%" deltaPositive dark={dark} accent={T.info} />
        <KPI label="检测节省工时" value="138h" sub="约 4.2 周" dark={dark} accent={T.ok} />
      </div>

      {/* Filter bar */}
      <div style={{ padding: '16px 20px 0', display: 'flex', alignItems: 'center', gap: 8 }}>
        <SegControl options={['全部', '进行中', '需复核', '已完成', '已归档']} value={0} accent={accent} dark={dark} />
        <div style={{ flex: 1 }} />
        <Pill fg={textMute} bg={dark ? 'rgba(255,255,255,0.04)' : T.white} style={{ border: `1px solid ${border}` }}>
          <VIcon name="user" size={11} />负责人 · 全部
        </Pill>
        <Pill fg={textMute} bg={dark ? 'rgba(255,255,255,0.04)' : T.white} style={{ border: `1px solid ${border}` }}>
          <VIcon name="folder" size={11} />范围 · 法务合同库 +1
        </Pill>
        <Pill fg={textMute} bg={dark ? 'rgba(255,255,255,0.04)' : T.white} style={{ border: `1px solid ${border}` }}>
          <span style={{ fontFamily: T.mono }}>{'>'}30%</span>
        </Pill>
      </div>

      {/* Table */}
      <div style={{ flex: 1, minHeight: 0, padding: '14px 20px 20px' }}>
        <div style={{
          height: '100%', background: cardBg, borderRadius: 10,
          border: `1px solid ${border}`, overflow: 'hidden',
          display: 'flex', flexDirection: 'column',
        }}>
          {/* header row */}
          <div style={{
            height: 32, flexShrink: 0, display: 'grid',
            gridTemplateColumns: '1.8fr 110px 1.2fr 100px 140px 110px 32px',
            alignItems: 'center', gap: 12, padding: '0 16px',
            borderBottom: `1px solid ${border}`,
            background: dark ? 'rgba(255,255,255,0.02)' : T.paper2,
            fontSize: 10.5, fontWeight: 600, letterSpacing: '0.05em', textTransform: 'uppercase',
            color: textMute,
          }}>
            <span>任务名称</span>
            <span>类型</span>
            <span>相似度分布</span>
            <span style={{ textAlign: 'right' }}>峰值</span>
            <span>负责人</span>
            <span>状态</span>
            <span></span>
          </div>
          <div style={{ flex: 1, overflow: 'auto' }}>
            {TASKS.map((t, i) => (
              <TaskRow key={i} t={t} dark={dark} border={border} textInk={textInk} textMute={textMute} accent={accent} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

const TASKS = [
  { name: '2026Q1 供应商保密协议 · 批次审核', type: '合同 · 两两互查', dist: [0.18, 0.42, 0.28, 0.10, 0.02], peak: '64%', owner: '周明远', avatar: '周', color: '#5B5BD6', status: 'running', progress: 0.62, sub: '6 份文档 · 2 分钟前' },
  { name: '员工手册修订版 v2026', type: '合规 · 与库对比', dist: [0.62, 0.20, 0.10, 0.06, 0.02], peak: '38%', owner: '林一帆', avatar: '林', color: '#1F7A3D', status: 'review', progress: 1, sub: '24 处需要复核 · 上午 09:42' },
  { name: 'Verity 产品白皮书 · 初稿', type: '通用 · 一对多', dist: [0.10, 0.25, 0.35, 0.22, 0.08], peak: '82%', owner: '陈思雨', avatar: '陈', color: '#B86A1F', status: 'review', progress: 1, sub: '高相似度警告 · 昨日' },
  { name: 'audit_pipeline 代码审计', type: '代码 · 库内比对', dist: [0.78, 0.12, 0.06, 0.03, 0.01], peak: '28%', owner: '高伟', avatar: '高', color: '#1B5BB7', status: 'done', progress: 1, sub: '已归档 · 5 月 24 日' },
  { name: '数据安全合规白皮书 v2.1', type: '合规 · 与库对比', dist: [0.45, 0.30, 0.15, 0.08, 0.02], peak: '47%', owner: '周明远', avatar: '周', color: '#5B5BD6', status: 'done', progress: 1, sub: '已交付 · 5 月 22 日' },
  { name: '合规检查清单 · 季度复盘', type: 'Excel · 两两互查', dist: [0.88, 0.08, 0.02, 0.01, 0.01], peak: '12%', owner: '林一帆', avatar: '林', color: '#1F7A3D', status: 'done', progress: 1, sub: '已归档 · 5 月 18 日' },
  { name: '《新员工 onboarding 流程》', type: '通用 · 与库对比', dist: [0.25, 0.30, 0.25, 0.15, 0.05], peak: '71%', owner: '陈思雨', avatar: '陈', color: '#B86A1F', status: 'archived', progress: 1, sub: '已归档 · 5 月 14 日' },
];

function TaskRow({ t, dark, border, textInk, textMute, accent }) {
  const statusMeta = {
    running:  { label: '运行中', fg: T.brand, bg: T.brandSoft, dot: T.brand },
    review:   { label: '需复核', fg: T.warn,  bg: T.warnSoft,  dot: T.warn },
    done:     { label: '已完成', fg: T.ok,    bg: T.okSoft,    dot: T.ok },
    archived: { label: '已归档', fg: T.ink3,  bg: T.paper3,    dot: T.ink4 },
  }[t.status];
  return (
    <div style={{
      display: 'grid', gridTemplateColumns: '1.8fr 110px 1.2fr 100px 140px 110px 32px',
      alignItems: 'center', gap: 12, padding: '14px 16px',
      borderBottom: `1px solid ${border}`,
    }}>
      <div>
        <div style={{ fontSize: 12.5, fontWeight: 600, color: textInk, lineHeight: 1.3 }}>
          {t.name}
        </div>
        <div style={{ fontSize: 11, color: textMute, marginTop: 3 }}>{t.sub}</div>
        {t.status === 'running' && (
          <div style={{ marginTop: 5, height: 3, background: dark ? 'rgba(255,255,255,0.08)' : T.paper3, borderRadius: 2, overflow: 'hidden' }}>
            <div style={{ height: '100%', width: `${t.progress * 100}%`, background: accent }} />
          </div>
        )}
      </div>
      <div style={{ fontSize: 11.5, color: textMute }}>{t.type}</div>
      <div>
        <DistBar dist={t.dist} dark={dark} />
      </div>
      <div style={{ textAlign: 'right', fontSize: 14, fontWeight: 700, fontFamily: T.mono, color: severityFg(t.peak) }}>
        {t.peak}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
        <Avatar name={t.avatar} color={t.color} size={20} />
        <span style={{ fontSize: 12, color: textInk }}>{t.owner}</span>
      </div>
      <Pill bg={statusMeta.bg} fg={statusMeta.fg} size={11}>
        <span style={{ width: 6, height: 6, borderRadius: '50%', background: statusMeta.dot }} />
        {statusMeta.label}
      </Pill>
      <VIcon name="chevR" size={12} style={{ color: textMute }} />
    </div>
  );
}

function severityFg(peak) {
  const v = parseInt(peak, 10);
  if (v >= 70) return T.danger;
  if (v >= 50) return T.hi3;
  if (v >= 30) return T.warn;
  return T.ok;
}

function DistBar({ dist, dark }) {
  const colors = [T.okSoft, T.hi1Soft, T.hi2Soft, T.hi3Soft, T.hi4Soft];
  const fills = [T.ok, T.hi1, T.hi2, T.hi3, T.hi4];
  // make a clean stacked bar
  return (
    <div style={{ display: 'flex', height: 7, borderRadius: 4, overflow: 'hidden', background: dark ? 'rgba(255,255,255,0.04)' : T.paper2 }}>
      {dist.map((v, i) => (
        <div key={i} style={{ width: `${v * 100}%`, background: fills[i], opacity: 0.85 }} />
      ))}
    </div>
  );
}

function KPI({ label, value, sub, delta, deltaPositive, dark, accent = T.brand }) {
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;
  return (
    <div style={{
      padding: '14px 16px', borderRadius: 10,
      background: dark ? 'rgba(255,255,255,0.04)' : T.white,
      border: `1px solid ${dark ? 'rgba(255,255,255,0.08)' : T.line}`,
    }}>
      <div style={{ fontSize: 11, color: textMute, fontWeight: 500 }}>{label}</div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, marginTop: 4 }}>
        <div style={{ fontSize: 22, fontWeight: 700, color: textInk, letterSpacing: '-0.018em' }}>{value}</div>
        {delta && (
          <span style={{
            fontSize: 11, fontWeight: 600, color: deltaPositive ? T.ok : T.danger,
            fontFamily: T.mono,
          }}>{delta}</span>
        )}
        {sub && <span style={{ fontSize: 11, color: textMute }}>{sub}</span>}
      </div>
      {/* mini sparkline */}
      <svg width="100%" height="22" viewBox="0 0 200 22" style={{ marginTop: 6, display: 'block' }} preserveAspectRatio="none">
        <path d="M0 14 L20 12 L40 16 L60 8 L80 10 L100 5 L120 9 L140 4 L160 8 L180 3 L200 6"
          stroke={accent} strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M0 14 L20 12 L40 16 L60 8 L80 10 L100 5 L120 9 L140 4 L160 8 L180 3 L200 6 L200 22 L0 22 Z"
          fill={accent} opacity="0.08" />
      </svg>
    </div>
  );
}


// ═════════════════════════════════════════════════════════════
// SCREEN 3 · Progress
// ═════════════════════════════════════════════════════════════
function ScrProgress({ accent = T.brand, dark = false }) {
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;
  const bg = dark ? '#1A1A1F' : T.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : T.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : T.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg }}>
      <VTopbar dark={dark}
        crumbs={['工作区', '我的任务', '2026Q1 供应商保密协议']}
        actions={<><Button kind="secondary" icon="pause" dark={dark}>暂停</Button>
                  <Button kind="primary" accent={accent} icon="eye">实时预览</Button></>}
      />

      <div style={{ flex: 1, minHeight: 0, padding: 20, display: 'grid', gridTemplateColumns: '1fr 360px', gap: 16, overflow: 'auto' }}>
        {/* LEFT main */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          {/* Big progress ring + summary */}
          <div style={{
            background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
            padding: 22, display: 'flex', alignItems: 'center', gap: 24,
          }}>
            <ProgressRing pct={62} accent={accent} dark={dark} />
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 13, color: textMute, fontWeight: 500 }}>正在进行</div>
              <div style={{ fontSize: 20, fontWeight: 700, color: textInk, marginTop: 2, letterSpacing: '-0.012em' }}>
                2026Q1 供应商保密协议 · 批次审核
              </div>
              <div style={{ display: 'flex', gap: 24, marginTop: 14 }}>
                <Stat label="已完成" value="3.7k" sub="片段对比" dark={dark} />
                <Stat label="剩余" value="2.3k" sub="预计 1分46秒" dark={dark} />
                <Stat label="发现重复" value="146" sub="≥30% 相似" dark={dark} fg={T.warn} />
                <Stat label="高危匹配" value="9" sub="≥70%" dark={dark} fg={T.danger} />
              </div>
            </div>
            <div style={{
              padding: '7px 10px', borderRadius: 8,
              background: dark ? 'rgba(255,255,255,0.04)' : T.paper2,
              fontFamily: T.mono, fontSize: 12, color: textMute,
              display: 'flex', alignItems: 'center', gap: 6,
            }}>
              <span style={{ width: 6, height: 6, borderRadius: '50%', background: T.ok, animation: 'vpulse 1.4s ease-in-out infinite' }} />
              <span>v2026.5.18 · cluster-04</span>
            </div>
          </div>

          {/* Per-doc list */}
          <div style={{
            background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
            display: 'flex', flexDirection: 'column', overflow: 'hidden', minHeight: 0,
          }}>
            <div style={{ padding: '12px 16px', borderBottom: `1px solid ${border}`, display: 'flex', alignItems: 'center' }}>
              <span style={{ fontSize: 13, fontWeight: 600, color: textInk }}>逐文档进度</span>
              <div style={{ flex: 1 }} />
              <Pill bg={dark ? 'rgba(255,255,255,0.06)' : T.paper2} fg={textMute}>共 6 份</Pill>
            </div>
            {PROGRESS_DOCS.map((d, i) => (
              <ProgressDocRow key={i} d={d} dark={dark} border={border} textInk={textInk} textMute={textMute} accent={accent} last={i === PROGRESS_DOCS.length - 1} />
            ))}
          </div>
        </div>

        {/* RIGHT log + queue */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16, minHeight: 0 }}>
          {/* Pipeline stages */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 16 }}>
            <div style={{ fontSize: 13, fontWeight: 600, color: textInk, marginBottom: 12 }}>处理阶段</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <PipelineStage label="解析 · 文本抽取" status="done" dark={dark} accent={accent} />
              <PipelineStage label="分词 · 句段切分" status="done" dark={dark} accent={accent} />
              <PipelineStage label="语义向量化"     status="done" dark={dark} accent={accent} />
              <PipelineStage label="相似度计算"     status="running" pct={68} dark={dark} accent={accent} />
              <PipelineStage label="片段聚合"       status="pending" dark={dark} accent={accent} />
              <PipelineStage label="AI 改写建议"    status="pending" dark={dark} accent={accent} />
              <PipelineStage label="报告生成"       status="pending" dark={dark} accent={accent} />
            </div>
          </div>
          {/* Live log */}
          <div style={{
            background: dark ? '#0E0E12' : '#1A1A1F', borderRadius: 12, padding: 14, flex: 1, minHeight: 0,
            display: 'flex', flexDirection: 'column',
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
              <span style={{ width: 8, height: 8, borderRadius: '50%', background: T.ok }} />
              <span style={{ fontSize: 11.5, fontWeight: 600, color: 'rgba(255,255,255,0.85)' }}>实时日志</span>
              <div style={{ flex: 1 }} />
              <span style={{ fontSize: 10, color: 'rgba(255,255,255,0.4)', fontFamily: T.mono }}>tail -f</span>
            </div>
            <div style={{ flex: 1, overflow: 'auto', fontFamily: T.mono, fontSize: 11, lineHeight: 1.65 }}>
              {LOG_LINES.map((l, i) => (
                <div key={i} style={{ display: 'flex', gap: 8 }}>
                  <span style={{ color: 'rgba(255,255,255,0.3)' }}>{l.t}</span>
                  <span style={{ color: l.level === 'WARN' ? T.warn : l.level === 'OK' ? T.ok : 'rgba(255,255,255,0.45)' }}>
                    {l.level}
                  </span>
                  <span style={{ color: 'rgba(255,255,255,0.78)', flex: 1 }}>{l.msg}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

const PROGRESS_DOCS = [
  { type: 'docx', name: '2026Q1-供应商保密协议-华东区v3.docx', pct: 100, status: 'done', sims: 18 },
  { type: 'docx', name: '2026Q1-供应商保密协议-西南区v3.docx', pct: 100, status: 'done', sims: 22 },
  { type: 'docx', name: '2026Q1-供应商保密协议-华北区v3.docx', pct: 100, status: 'done', sims: 24 },
  { type: 'docx', name: '2026Q1-供应商保密协议-华东区v4(草稿).docx', pct: 68, status: 'running', sims: 31 },
  { type: 'docx', name: '2026Q1-供应商保密协议-海外版v1.docx', pct: 0, status: 'pending', sims: null },
  { type: 'docx', name: '附录A-条款变更说明.docx', pct: 0, status: 'pending', sims: null },
];

function ProgressDocRow({ d, dark, border, textInk, textMute, accent, last }) {
  const statusColor = d.status === 'done' ? T.ok : d.status === 'running' ? accent : T.ink5;
  return (
    <div style={{
      display: 'grid', gridTemplateColumns: '20px 1fr 200px 60px',
      alignItems: 'center', gap: 12, padding: '11px 16px',
      borderBottom: last ? 'none' : `1px solid ${border}`,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        {d.status === 'done' ? (
          <div style={{ width: 16, height: 16, borderRadius: '50%', background: T.ok, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <VIcon name="check" size={11} style={{ color: '#fff' }} strokeWidth={2.5} />
          </div>
        ) : d.status === 'running' ? (
          <Spinner size={14} color={accent} />
        ) : (
          <div style={{ width: 14, height: 14, borderRadius: '50%', border: `1.5px solid ${dark ? 'rgba(255,255,255,0.2)' : T.ink5}` }} />
        )}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}>
        <DocChip type={d.type} />
        <span style={{
          fontSize: 12.5, color: textInk, fontWeight: 500,
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
        }}>{d.name}</span>
      </div>
      <div>
        <div style={{ height: 5, background: dark ? 'rgba(255,255,255,0.06)' : T.paper2, borderRadius: 3, overflow: 'hidden' }}>
          <div style={{ height: '100%', width: `${d.pct}%`, background: statusColor }} />
        </div>
      </div>
      <div style={{ textAlign: 'right', fontSize: 11.5, color: textMute, fontFamily: T.mono }}>
        {d.sims != null ? `${d.sims} 处` : '—'}
      </div>
    </div>
  );
}

function PipelineStage({ label, status, pct, dark, accent }) {
  const done = status === 'done';
  const running = status === 'running';
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
      <div style={{
        width: 18, height: 18, borderRadius: '50%', flexShrink: 0,
        background: done ? T.ok : running ? `${accent}22` : 'transparent',
        border: done ? 'none' : `1.5px solid ${running ? accent : (dark ? 'rgba(255,255,255,0.2)' : T.ink5)}`,
        display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}>
        {done && <VIcon name="check" size={11} style={{ color: '#fff' }} strokeWidth={2.5} />}
        {running && <Spinner size={10} color={accent} />}
      </div>
      <span style={{
        flex: 1, fontSize: 12, fontWeight: 500,
        color: done ? (dark ? 'rgba(255,255,255,0.55)' : T.ink3) : (dark ? '#fff' : T.ink),
      }}>{label}</span>
      {running && <span style={{ fontSize: 11, color: accent, fontFamily: T.mono, fontWeight: 600 }}>{pct}%</span>}
    </div>
  );
}

const LOG_LINES = [
  { t: '14:22:11', level: 'INFO', msg: 'parsing 2026Q1-华东v4.docx · 1240 段落' },
  { t: '14:22:14', level: 'INFO', msg: 'embedding batch 17/24 · 64 句段' },
  { t: '14:22:18', level: 'OK',   msg: 'cluster match: §4.2 ←→ 法务库/MSA-template §4.2 (88%)' },
  { t: '14:22:19', level: 'OK',   msg: 'cluster match: §7.1 ←→ 华北v3 §7.1 (74%)' },
  { t: '14:22:21', level: 'WARN', msg: 'long-tail boilerplate detected · skipping §A.3' },
  { t: '14:22:23', level: 'OK',   msg: 'cluster match: §11 ←→ 历史归档/v2024 §11 (62%)' },
  { t: '14:22:25', level: 'INFO', msg: 'AI rewrite candidate generated · §4.2 v0.1' },
  { t: '14:22:27', level: 'INFO', msg: 'similarity index updated · 24,118 spans' },
  { t: '14:22:29', level: 'OK',   msg: 'snapshot saved · cluster-04/checkpoint-68' },
];

function ProgressRing({ pct, accent, dark }) {
  const r = 50;
  const c = 2 * Math.PI * r;
  const dash = c * (pct / 100);
  return (
    <div style={{ position: 'relative', width: 120, height: 120 }}>
      <svg width="120" height="120" viewBox="0 0 120 120" style={{ transform: 'rotate(-90deg)' }}>
        <circle cx="60" cy="60" r={r} fill="none" stroke={dark ? 'rgba(255,255,255,0.08)' : T.paper2} strokeWidth="8" />
        <circle cx="60" cy="60" r={r} fill="none" stroke={accent} strokeWidth="8"
          strokeDasharray={`${dash} ${c}`} strokeLinecap="round" />
      </svg>
      <div style={{
        position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'center',
      }}>
        <div style={{ fontSize: 28, fontWeight: 700, color: dark ? '#fff' : T.ink, letterSpacing: '-0.022em', lineHeight: 1, fontFamily: T.mono }}>
          {pct}<span style={{ fontSize: 14, color: dark ? 'rgba(255,255,255,0.5)' : T.ink3 }}>%</span>
        </div>
        <div style={{ fontSize: 10, color: dark ? 'rgba(255,255,255,0.5)' : T.ink3, marginTop: 4, fontWeight: 600, letterSpacing: '0.04em', textTransform: 'uppercase' }}>
          完成度
        </div>
      </div>
    </div>
  );
}

function Stat({ label, value, sub, dark, fg }) {
  return (
    <div>
      <div style={{ fontSize: 10.5, fontWeight: 600, color: dark ? 'rgba(255,255,255,0.5)' : T.ink3, letterSpacing: '0.05em', textTransform: 'uppercase' }}>
        {label}
      </div>
      <div style={{ fontSize: 18, fontWeight: 700, color: fg || (dark ? '#fff' : T.ink), letterSpacing: '-0.012em', marginTop: 3, fontFamily: T.mono }}>
        {value}
      </div>
      <div style={{ fontSize: 10.5, color: dark ? 'rgba(255,255,255,0.5)' : T.ink3, marginTop: 1 }}>{sub}</div>
    </div>
  );
}


// ═════════════════════════════════════════════════════════════
// SCREEN 4 · Overview Report
// ═════════════════════════════════════════════════════════════
function ScrOverview({ accent = T.brand, dark = false }) {
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;
  const bg = dark ? '#1A1A1F' : T.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : T.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : T.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, overflow: 'hidden' }}>
      <VTopbar dark={dark}
        crumbs={['工作区', '我的任务', 'Verity 产品白皮书 · 初稿', '总览报告']}
        actions={<><Button kind="ghost" icon="share" dark={dark}>分享</Button>
                  <Button kind="secondary" icon="download" dark={dark}>导出 PDF</Button>
                  <Button kind="primary" accent={accent} icon="eye">进入对比视图</Button></>}
      />
      <div style={{ flex: 1, minHeight: 0, padding: 20, overflow: 'auto', display: 'flex', flexDirection: 'column', gap: 16 }}>
        {/* Header: title + headline number */}
        <div style={{
          background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
          padding: '20px 24px', display: 'grid', gridTemplateColumns: '1.4fr 1fr', gap: 24,
        }}>
          <div>
            <Pill bg={T.warnSoft} fg={T.warn} size={11}>
              <VIcon name="warn" size={10} />需复核 · 24 处
            </Pill>
            <div style={{ fontSize: 22, fontWeight: 700, color: textInk, letterSpacing: '-0.014em', marginTop: 8 }}>
              Verity 产品白皮书 · 初稿
            </div>
            <div style={{ fontSize: 12.5, color: textMute, marginTop: 5 }}>
              22 页 · 13,840 字 · 与 4 个查重源对比 · 任务耗时 4 分 18 秒
            </div>
            {/* Severity bar */}
            <div style={{ marginTop: 18 }}>
              <div style={{
                display: 'flex', height: 12, borderRadius: 6, overflow: 'hidden',
                border: `1px solid ${border}`,
              }}>
                <div style={{ width: '34%', background: T.okSoft }} />
                <div style={{ width: '26%', background: T.hi1 }} />
                <div style={{ width: '22%', background: T.hi2 }} />
                <div style={{ width: '13%', background: T.hi3 }} />
                <div style={{ width: '5%',  background: T.hi4 }} />
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 7 }}>
                {[
                  { label: '原创', pct: '34%', c: T.ok },
                  { label: '低相似', pct: '26%', c: T.hi1 },
                  { label: '中相似', pct: '22%', c: T.hi2 },
                  { label: '高相似', pct: '13%', c: T.hi3 },
                  { label: '复制', pct: '5%', c: T.hi4 },
                ].map((s, i) => (
                  <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
                    <span style={{ width: 6, height: 6, borderRadius: '50%', background: s.c }} />
                    <span style={{ fontSize: 11, color: textMute }}>{s.label}</span>
                    <span style={{ fontSize: 11.5, fontWeight: 700, color: textInk, fontFamily: T.mono }}>{s.pct}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', justifyContent: 'center' }}>
            <div style={{ fontSize: 11, color: textMute, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase' }}>
              总体相似度
            </div>
            <div style={{
              fontSize: 88, fontWeight: 700, color: T.hi3, letterSpacing: '-0.04em',
              lineHeight: 1, fontFamily: T.font, marginTop: 6,
            }}>
              40<span style={{ fontSize: 36, color: textMute, fontWeight: 500 }}>.2%</span>
            </div>
            <div style={{ fontSize: 12, color: textMute, marginTop: 6 }}>
              较同部门平均 <span style={{ color: T.danger, fontWeight: 700 }}>+8.7pp</span> · 较你上一稿 <span style={{ color: T.ok, fontWeight: 700 }}>−4.1pp</span>
            </div>
          </div>
        </div>

        {/* Distribution chart + source breakdown */}
        <div style={{ display: 'grid', gridTemplateColumns: '1.4fr 1fr', gap: 16 }}>
          {/* Section similarity bar chart */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
            <div style={{ display: 'flex', alignItems: 'baseline', marginBottom: 14 }}>
              <span style={{ fontSize: 13, fontWeight: 700, color: textInk }}>各章节相似度</span>
              <span style={{ fontSize: 11, color: textMute, marginLeft: 8 }}>按文档结构</span>
              <div style={{ flex: 1 }} />
              <SegControl options={['章节', '页码', '段落']} value={0} accent={accent} dark={dark} />
            </div>
            <SectionChart dark={dark} textMute={textMute} textInk={textInk} />
          </div>

          {/* Top sources */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: textInk, marginBottom: 14 }}>主要相似来源</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 11 }}>
              {[
                { name: 'Verity 产品介绍 · 2025 v3.docx', src: '内部 / 市场部', pct: 0.62, type: 'docx', hits: 18 },
                { name: '数据安全合规白皮书 2026.pdf',     src: '内部 / 安全部', pct: 0.41, type: 'pdf',  hits: 12 },
                { name: '企业 SOC2 实施指南.pdf',         src: '行业公开',     pct: 0.28, type: 'pdf',  hits: 9 },
                { name: 'README.md · verity-platform',    src: '工程文档',     pct: 0.18, type: 'md',   hits: 5 },
              ].map((s, i) => (
                <SourceRowDetail key={i} s={s} dark={dark} textInk={textInk} textMute={textMute} accent={accent} />
              ))}
            </div>
          </div>
        </div>

        {/* Top spans */}
        <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
          <div style={{ display: 'flex', alignItems: 'baseline', marginBottom: 14 }}>
            <span style={{ fontSize: 13, fontWeight: 700, color: textInk }}>最值得关注的片段</span>
            <span style={{ fontSize: 11, color: textMute, marginLeft: 8 }}>按相似度排序 · 前 4 条</span>
            <div style={{ flex: 1 }} />
            <Button kind="ghost" size="sm" dark={dark}>查看全部 24 处 →</Button>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {[
              { sec: '§3.2 安全架构', text: '本平台采用端到端加密、最小权限模型与全链路审计追溯，所有数据在传输与静止状态下均通过 AES-256 算法加密。', pct: 92, src: 'Verity 产品介绍 · §2.1' },
              { sec: '§5.1 合规性',  text: '我们遵循 ISO 27001、SOC 2 Type II 与 GDPR 的相关条款，并通过持续的内部审计与第三方评估机制保证合规水位。', pct: 84, src: '数据安全合规白皮书 · §4.3' },
              { sec: '§7 部署模式',  text: '产品支持公有云、私有化与混合云三种部署形态，企业可按数据驻留与隔离要求灵活选择。', pct: 71, src: 'SOC2 实施指南 · §6.2' },
              { sec: '§9 客户案例',  text: '某头部金融机构在引入本平台后，合同审阅周期由 7 天缩短至 0.5 天，年度节省合规人力成本约 240 万元。', pct: 38, src: '内部市场素材库 · case-fin-04' },
            ].map((s, i) => (
              <TopSpanRow key={i} s={s} dark={dark} textInk={textInk} textMute={textMute} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function SectionChart({ dark, textMute, textInk }) {
  const data = [
    { name: '§1 引言',       pct: 0.18, segs: [0.18] },
    { name: '§2 产品概述',   pct: 0.42, segs: [0.20, 0.22] },
    { name: '§3 安全架构',   pct: 0.82, segs: [0.30, 0.32, 0.20] },
    { name: '§4 部署',       pct: 0.55, segs: [0.25, 0.18, 0.12] },
    { name: '§5 合规',       pct: 0.74, segs: [0.32, 0.28, 0.14] },
    { name: '§6 案例',       pct: 0.28, segs: [0.18, 0.10] },
    { name: '§7 服务',       pct: 0.46, segs: [0.22, 0.16, 0.08] },
    { name: '§8 路线图',     pct: 0.12, segs: [0.12] },
    { name: '§9 附录',       pct: 0.36, segs: [0.20, 0.16] },
  ];
  const colorFor = (pct) => pct >= 0.7 ? T.hi4 : pct >= 0.5 ? T.hi3 : pct >= 0.3 ? T.hi2 : pct >= 0.15 ? T.hi1 : T.ok;
  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'flex-end', gap: 10, height: 160, paddingBottom: 6, borderBottom: `1px solid ${dark ? 'rgba(255,255,255,0.08)' : T.line}` }}>
        {data.map((d, i) => {
          const h = Math.max(8, d.pct * 150);
          return (
            <div key={i} style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 5 }}>
              <span style={{ fontSize: 10, color: textInk, fontWeight: 600, fontFamily: T.mono }}>
                {(d.pct * 100).toFixed(0)}%
              </span>
              <div style={{ width: '100%', maxWidth: 36, height: h, borderRadius: 4, background: `${colorFor(d.pct)}33`, position: 'relative', overflow: 'hidden' }}>
                <div style={{ position: 'absolute', bottom: 0, left: 0, right: 0, height: '100%', background: colorFor(d.pct), opacity: 0.85 }} />
              </div>
            </div>
          );
        })}
      </div>
      <div style={{ display: 'flex', gap: 10, marginTop: 8 }}>
        {data.map((d, i) => (
          <div key={i} style={{ flex: 1, textAlign: 'center', fontSize: 10, color: textMute, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {d.name}
          </div>
        ))}
      </div>
    </div>
  );
}

function SourceRowDetail({ s, dark, textInk, textMute, accent }) {
  const pctColor = s.pct >= 0.5 ? T.hi3 : s.pct >= 0.3 ? T.hi2 : T.hi1;
  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
        <DocChip type={s.type} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            fontSize: 12, fontWeight: 600, color: textInk,
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}>{s.name}</div>
          <div style={{ fontSize: 10.5, color: textMute, marginTop: 1 }}>{s.src} · {s.hits} 处匹配</div>
        </div>
        <div style={{ fontSize: 14, fontWeight: 700, color: pctColor, fontFamily: T.mono, letterSpacing: '-0.01em' }}>
          {(s.pct * 100).toFixed(0)}%
        </div>
      </div>
      <div style={{ height: 4, marginTop: 6, background: dark ? 'rgba(255,255,255,0.06)' : T.paper2, borderRadius: 2, overflow: 'hidden' }}>
        <div style={{ height: '100%', width: `${s.pct * 100}%`, background: pctColor }} />
      </div>
    </div>
  );
}

function TopSpanRow({ s, dark, textInk, textMute }) {
  const c = s.pct >= 80 ? T.hi4 : s.pct >= 60 ? T.hi3 : s.pct >= 40 ? T.hi2 : T.hi1;
  return (
    <div style={{
      display: 'grid', gridTemplateColumns: '90px 1fr 110px',
      gap: 14, padding: '12px 14px', borderRadius: 8,
      background: dark ? 'rgba(255,255,255,0.025)' : T.paper2,
      alignItems: 'center',
    }}>
      <div>
        <div style={{ fontSize: 11, fontWeight: 600, color: textInk }}>{s.sec}</div>
        <div style={{ fontSize: 10.5, color: textMute, marginTop: 2 }}>{s.src}</div>
      </div>
      <div style={{
        fontSize: 12.5, color: textInk, lineHeight: 1.5,
        borderBottom: `1.5px solid ${c}`, paddingBottom: 1,
        display: 'inline-block',
      }}>"{s.text}"</div>
      <div style={{ textAlign: 'right' }}>
        <div style={{ fontSize: 22, fontWeight: 700, color: c, lineHeight: 1, fontFamily: T.mono, letterSpacing: '-0.01em' }}>{s.pct}%</div>
        <div style={{ fontSize: 10.5, color: textMute, marginTop: 3 }}>相似度</div>
      </div>
    </div>
  );
}

Object.assign(window, {
  ScrImport, ScrTasks, ScrProgress, ScrOverview,
});
