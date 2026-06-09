// bid-c.jsx — Clusters (重复条款聚合 / 围标提示) + Export + Settings

// ═══════════════════════════════════════════════════════════════
// SCREEN 6 · Clusters — common segments across multiple bids
// ═══════════════════════════════════════════════════════════════
function BidScrClusters({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="重复条款聚合"
        sub="市政信息化平台采购 · 共 12 组雷同条款 · 含 2 组围标嫌疑"
        actions={<>
          <CButton kind="ghost" size="md" icon="filter" dark={dark}>筛选</CButton>
          <CButton kind="secondary" size="md" icon="quote" dark={dark}>抽取证据</CButton>
          <CButton kind="primary" size="md" accent={accent} icon="download">导出 Excel</CButton>
        </>}
      />
      <div style={{ flex: 1, minHeight: 0, display: 'grid', gridTemplateColumns: '300px 1fr', gap: 16, padding: 20, overflow: 'hidden' }}>
        {/* LEFT: list */}
        <div style={{
          background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
          display: 'flex', flexDirection: 'column', overflow: 'hidden',
        }}>
          <div style={{ padding: '12px 14px', borderBottom: `1px solid ${border}`, display: 'flex', alignItems: 'center' }}>
            <span style={{ fontSize: 12.5, fontWeight: 700, color: ink }}>聚合条款</span>
            <div style={{ flex: 1 }} />
            <CPill bg={dark ? 'rgba(255,255,255,0.06)' : C.paper2} fg={mute} size={10}>12 组</CPill>
          </div>
          <div style={{ flex: 1, overflow: 'auto' }}>
            {CLUSTERS.map((c, i) => (
              <ClusterListItem key={i} c={c} active={i === 0} dark={dark} border={border} ink={ink} mute={mute} accent={accent} />
            ))}
          </div>
        </div>

        {/* RIGHT: cluster detail */}
        <div style={{
          background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
          display: 'flex', flexDirection: 'column', overflow: 'hidden',
        }}>
          {/* header */}
          <div style={{ padding: '16px 22px', borderBottom: `1px solid ${border}` }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <CPill bg={C.dangerSoft} fg={C.danger} size={11}>
                <CIcon name="info" size={10} />围标嫌疑
              </CPill>
              <span style={{ fontSize: 11.5, color: mute }}>聚合 #01 · 出现于 2 份标书</span>
              <div style={{ flex: 1 }} />
              <span style={{ fontSize: 11, color: mute }}>组内平均相似度</span>
              <span style={{ fontSize: 15, fontWeight: 700, color: C.danger, fontFamily: C.mono }}>91%</span>
            </div>
            <div style={{ fontSize: 18, fontWeight: 700, color: ink, marginTop: 8, letterSpacing: '-0.012em', fontFamily: C.serif }}>
              §3.2 总体架构 · 分层解耦与 API 网关
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginTop: 9 }}>
              <span style={{ fontSize: 10.5, color: mute, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase' }}>出现于</span>
              <DocTag tag="甲" name="智慧城邦" color="#4F58A8" />
              <DocTag tag="乙" name="启明信息" color="#0E9A8F" />
            </div>
          </div>

          <div style={{ flex: 1, overflow: 'auto', padding: 22 }}>
            {/* Canonical text */}
            <div style={{
              padding: '14px 18px', borderRadius: 10,
              background: dark ? 'rgba(255,255,255,0.025)' : C.paper2,
              border: `1px solid ${border}`,
            }}>
              <div style={{ fontSize: 10, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 7 }}>
                归一化文本(双方共享)
              </div>
              <div style={{ fontSize: 13.5, lineHeight: 1.85, color: ink, fontFamily: C.font }}>
                "系统自下而上划分为<u style={{ textDecorationColor: C.hi4, textDecorationStyle: 'solid', textDecorationThickness: 2, textUnderlineOffset: 3 }}>基础设施层、数据资源层、应用支撑层与业务应用层</u>,
                各层之间通过标准化接口解耦,所有业务能力对外以 <u style={{ textDecorationColor: C.hi4, textDecorationStyle: 'solid', textDecorationThickness: 2, textUnderlineOffset: 3 }}>API 网关统一暴露</u>,
                确保横向可扩展与纵向可演进。"
              </div>
            </div>

            {/* Anomaly evidence */}
            <div style={{
              marginTop: 14, padding: '12px 14px', borderRadius: 8,
              background: dark ? 'rgba(181,69,69,0.10)' : C.dangerSoft,
              border: `1px solid ${dark ? 'rgba(181,69,69,0.3)' : '#E8C7C7'}`,
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
                <CIcon name="info" size={13} style={{ color: C.danger }} />
                <span style={{ fontSize: 11.5, fontWeight: 700, color: C.danger }}>为何判定为围标嫌疑</span>
              </div>
              <ul style={{ fontSize: 11.5, color: ink, marginTop: 8, paddingLeft: 22, lineHeight: 1.7 }}>
                <li>双方语序、标点、错别字(如「演进」误作「演近」)完全一致 </li>
                <li>该段落不属于行业通用模板,与任一公开模板库相似度低于 12%</li>
                <li>整体技术方案章节相似度 92%,远高于本批次其他 5 对组合均值 38%</li>
                <li>两家公司控股股东在工商登记中存在关联(基于公开数据匹配)</li>
              </ul>
            </div>

            {/* Per-doc occurrences */}
            <div style={{ fontSize: 11, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginTop: 22, marginBottom: 10 }}>
              在各份标书中的呈现
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              {[
                {
                  tag: '甲', color: '#4F58A8', name: '智慧城邦科技_技术响应文件', sec: '§3.2 第 12 页',
                  diff: [
                    ['eq', '系统自下而上划分为'],
                    ['eq', '基础设施层、数据资源层、应用支撑层与业务应用层'],
                    ['eq', ',各层之间通过标准化接口解耦'],
                    ['add', ','], ['eq', '所有业务能力对外以 API 网关统一暴露'],
                  ],
                },
                {
                  tag: '乙', color: '#0E9A8F', name: '启明信息_投标文件_技术标', sec: '§3.1 第 9 页',
                  diff: [
                    ['eq', '系统自下而上划分为'],
                    ['eq', '基础设施层、数据资源层、应用支撑层与业务应用层'],
                    ['eq', ',各层之间通过标准化接口解耦'],
                    ['eq', ',所有业务能力对外以 API 网关统一暴露'],
                  ],
                },
              ].map((o, i) => (
                <BidOccurrence key={i} o={o} dark={dark} border={border} ink={ink} mute={mute} />
              ))}
            </div>

            {/* Pair-level statistics */}
            <div style={{ marginTop: 22, padding: '14px 18px', borderRadius: 10, background: dark ? 'rgba(255,255,255,0.025)' : C.paper2, border: `1px solid ${border}` }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 10 }}>
                甲 × 乙 全文相似度热力(章节维度)
              </div>
              <SectionHeatmap dark={dark} mute={mute} ink={ink} />
            </div>
          </div>

          {/* Bottom actions */}
          <div style={{
            padding: '12px 22px', borderTop: `1px solid ${border}`,
            display: 'flex', alignItems: 'center', gap: 9,
            background: dark ? 'rgba(255,255,255,0.02)' : C.paper2,
          }}>
            <CButton kind="secondary" size="sm" icon="diff" dark={dark}>查看 甲 × 乙 逐段对比</CButton>
            <CButton kind="secondary" size="sm" icon="info" dark={dark}>查看公司工商关联</CButton>
            <div style={{ flex: 1 }} />
            <CButton kind="ghost" size="sm" dark={dark}>标记为已审</CButton>
            <CButton kind="primary" size="sm" accent={C.danger} icon="quote">列入异常报告</CButton>
          </div>
        </div>
      </div>
    </div>
  );
}

const CLUSTERS = [
  { sev: 'critical', title: '§3.2 分层架构 · 完全雷同', docs: ['甲','乙'], colors: ['#4F58A8','#0E9A8F'], pct: 100 },
  { sev: 'critical', title: '§5.1 服务承诺 · 措辞一字不差', docs: ['甲','乙'], colors: ['#4F58A8','#0E9A8F'], pct: 98 },
  { sev: 'high',     title: '§3.3 安全合规体系', docs: ['甲','乙'], colors: ['#4F58A8','#0E9A8F'], pct: 88 },
  { sev: 'high',     title: '§7.1 项目实施总体计划', docs: ['丙','丁'], colors: ['#C28430','#B54545'], pct: 78 },
  { sev: 'mid',      title: '§4.2 项目管理体系', docs: ['丙','丁'], colors: ['#C28430','#B54545'], pct: 64 },
  { sev: 'mid',      title: '§6 售后服务标准', docs: ['甲','乙','丁'], colors: ['#4F58A8','#0E9A8F','#B54545'], pct: 58 },
  { sev: 'mid',      title: '§3.4 数据治理方案', docs: ['甲','乙'], colors: ['#4F58A8','#0E9A8F'], pct: 52 },
  { sev: 'low',      title: '§2.1 公司基本介绍', docs: ['甲','丙'], colors: ['#4F58A8','#C28430'], pct: 38 },
  { sev: 'low',      title: '§8.2 培训方案', docs: ['乙','丁'], colors: ['#0E9A8F','#B54545'], pct: 36 },
  { sev: 'low',      title: '§5.2 应急响应承诺', docs: ['甲','丁'], colors: ['#4F58A8','#B54545'], pct: 35 },
  { sev: 'low',      title: '§9 附录 · 资质证书清单', docs: ['甲','乙','丙','丁'], colors: ['#4F58A8','#0E9A8F','#C28430','#B54545'], pct: 32, tpl: true },
  { sev: 'low',      title: '§A.3 法律法规引用', docs: ['甲','乙','丙','丁'], colors: ['#4F58A8','#0E9A8F','#C28430','#B54545'], pct: 31, tpl: true },
];

function ClusterListItem({ c, active, dark, border, ink, mute, accent }) {
  const sevColor = c.sev === 'critical' ? C.danger : c.sev === 'high' ? C.hi3 : c.sev === 'mid' ? C.hi2 : C.hi1;
  const sevLabel = c.sev === 'critical' ? '围标嫌疑' : c.sev === 'high' ? '高相似' : c.sev === 'mid' ? '中相似' : (c.tpl ? '通用模板' : '低相似');
  return (
    <div style={{
      padding: '12px 14px', borderBottom: `1px solid ${border}`,
      background: active ? (dark ? 'rgba(255,255,255,0.06)' : C.brandSoft) : 'transparent',
      position: 'relative', cursor: 'pointer',
    }}>
      {active && <div style={{ position: 'absolute', left: 0, top: 8, bottom: 8, width: 2.5, background: accent, borderRadius: 2 }} />}
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <span style={{ width: 6, height: 6, borderRadius: '50%', background: sevColor }} />
        <span style={{ fontSize: 12, fontWeight: active ? 700 : 600, color: ink, flex: 1, lineHeight: 1.35 }}>{c.title}</span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginTop: 7, paddingLeft: 13 }}>
        <div style={{ display: 'flex', gap: 2 }}>
          {c.docs.map((d, i) => (
            <div key={i} style={{
              width: 16, height: 16, borderRadius: 3, background: c.colors[i], color: '#fff',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 9.5, fontWeight: 700, fontFamily: C.serif,
            }}>{d}</div>
          ))}
        </div>
        <CPill bg={c.sev === 'critical' ? C.dangerSoft : C.paper2} fg={sevColor} size={10}>{sevLabel}</CPill>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11.5, fontWeight: 700, color: sevColor, fontFamily: C.mono }}>{c.pct}%</span>
      </div>
    </div>
  );
}

function DocTag({ tag, name, color }) {
  return (
    <div style={{
      display: 'inline-flex', alignItems: 'center', gap: 5,
      padding: '3px 8px 3px 3px', borderRadius: 999,
      background: `${color}14`, border: `1px solid ${color}33`,
    }}>
      <div style={{
        width: 16, height: 16, borderRadius: 4, background: color, color: '#fff',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        fontSize: 10, fontWeight: 700, fontFamily: C.serif,
      }}>{tag}</div>
      <span style={{ fontSize: 11, fontWeight: 600, color: color }}>{name}</span>
    </div>
  );
}

function BidOccurrence({ o, dark, border, ink, mute }) {
  return (
    <div style={{
      padding: '12px 14px', borderRadius: 10,
      background: dark ? 'rgba(255,255,255,0.025)' : '#fff',
      border: `1px solid ${border}`,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
        <div style={{
          width: 20, height: 20, borderRadius: 5,
          background: o.color, color: '#fff',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontSize: 11, fontWeight: 700, fontFamily: C.serif,
        }}>{o.tag}</div>
        <span style={{ fontSize: 12, fontWeight: 600, color: ink }}>{o.name}</span>
        <span style={{ fontSize: 11, color: mute, fontFamily: C.mono }}>{o.sec}</span>
      </div>
      <div style={{
        marginTop: 9, padding: '10px 12px', borderRadius: 6,
        background: dark ? 'rgba(255,255,255,0.025)' : C.paper2,
        fontSize: 12, lineHeight: 1.7, color: ink,
      }}>
        {o.diff.map(([op, text], i) => {
          if (op === 'eq') return <span key={i}>{text}</span>;
          if (op === 'add') return <span key={i} style={{ background: C.okSoft, color: C.ok, padding: '0 2px', borderRadius: 2, fontFamily: C.mono }}>+{text}</span>;
          if (op === 'del') return <span key={i} style={{ background: C.dangerSoft, color: C.danger, padding: '0 2px', borderRadius: 2, textDecoration: 'line-through', fontFamily: C.mono }}>−{text}</span>;
          return null;
        })}
      </div>
    </div>
  );
}

function SectionHeatmap({ dark, mute, ink }) {
  const sections = ['§1', '§2', '§3', '§3.2', '§3.3', '§4', '§5', '§5.1', '§6', '§7', '§8', '§9'];
  const values = [0.18, 0.26, 0.88, 1.00, 0.92, 0.72, 0.95, 0.98, 0.62, 0.78, 0.42, 0.34];
  const cellColor = (v) => {
    if (v >= 0.9) return C.hi4;
    if (v >= 0.7) return C.hi3;
    if (v >= 0.5) return C.hi2;
    if (v >= 0.3) return C.hi1;
    return C.okSoft;
  };
  return (
    <div>
      <div style={{ display: 'grid', gridTemplateColumns: `repeat(${sections.length}, 1fr)`, gap: 4 }}>
        {values.map((v, i) => (
          <div key={i} style={{
            aspectRatio: '1/1.4', borderRadius: 4, background: cellColor(v),
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            color: v >= 0.7 ? '#fff' : ink,
            fontSize: 10.5, fontWeight: 700, fontFamily: C.mono,
          }}>{(v * 100).toFixed(0)}</div>
        ))}
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: `repeat(${sections.length}, 1fr)`, gap: 4, marginTop: 6 }}>
        {sections.map((s, i) => (
          <div key={i} style={{ textAlign: 'center', fontSize: 9.5, color: mute, fontFamily: C.mono }}>{s}</div>
        ))}
      </div>
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 7 · Export — preview + format options
// ═══════════════════════════════════════════════════════════════
function BidScrExport({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="导出报告"
        sub="市政信息化平台采购 · 4 份标书 · 6 对比对"
        actions={<CButton kind="primary" size="md" accent={accent} icon="download">立即导出</CButton>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '28px 48px 40px' }}>
        <div style={{ maxWidth: 1100, margin: '0 auto', display: 'grid', gridTemplateColumns: '420px 1fr', gap: 18 }}>

          {/* LEFT: options */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 10 }}>
                文件格式
              </div>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 8 }}>
                {[
                  { t: 'pdf',  label: 'PDF', sub: '便于分发签批', active: true },
                  { t: 'docx', label: 'Word', sub: '可继续编辑' },
                  { t: 'xls',  label: 'Excel', sub: '矩阵 + 明细' },
                ].map((o, i) => (
                  <div key={i} style={{
                    padding: 12, borderRadius: 8, cursor: 'pointer',
                    border: `1.5px solid ${o.active ? accent : border}`,
                    background: o.active ? (dark ? 'rgba(79,88,168,0.10)' : `${accent}10`) : (dark ? 'rgba(255,255,255,0.02)' : '#fff'),
                  }}>
                    <CDocChip type={o.t === 'xls' ? 'xls' : o.t} />
                    <div style={{ fontSize: 12, fontWeight: 600, color: ink, marginTop: 8 }}>{o.label}</div>
                    <div style={{ fontSize: 10.5, color: mute, marginTop: 2 }}>{o.sub}</div>
                  </div>
                ))}
              </div>
            </div>

            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 10 }}>
                包含内容
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 7 }}>
                <BidCheck label="封面 + 评审摘要" checked dark={dark} accent={accent} />
                <BidCheck label="N × N 相似度矩阵" checked dark={dark} accent={accent} />
                <BidCheck label="围标嫌疑结论与证据链" checked dark={dark} accent={accent} />
                <BidCheck label="逐对左右对比快照" checked dark={dark} accent={accent} />
                <BidCheck label="重复条款明细清单(全部 12 组)" checked dark={dark} accent={accent} />
                <BidCheck label="章节级热力图" checked dark={dark} accent={accent} />
                <BidCheck label="工商关联辅助参考(若启用)" dark={dark} accent={accent} />
              </div>
            </div>

            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 10 }}>
                安全与签发
              </div>
              <SettingsRow label="添加评审水印" sub="评审人 · 时间 · 文件编号" dark={dark} ink={ink} mute={mute}>
                <CToggle on accent={accent} />
              </SettingsRow>
              <SettingsRow label="文件密码保护" sub="打开报告时需输入密码" dark={dark} ink={ink} mute={mute}>
                <CToggle on accent={accent} />
              </SettingsRow>
              <SettingsRow label="附带源文件清单" sub="不包含标书原文" dark={dark} ink={ink} mute={mute} last>
                <CToggle on accent={accent} />
              </SettingsRow>
            </div>
          </div>

          {/* RIGHT: preview */}
          <div style={{
            background: dark ? '#15151B' : '#E8E5DE', borderRadius: 12,
            border: `1px solid ${border}`, padding: 24,
            display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 16,
            overflow: 'auto',
          }}>
            <div style={{ fontSize: 11.5, color: mute }}>报告预览 · 共 32 页</div>
            <ReportPageCover accent={accent} />
            <ReportPageMatrix accent={accent} />
          </div>
        </div>
      </div>
    </div>
  );
}

function BidCheck({ checked, label, dark, accent }) {
  return (
    <label style={{ display: 'flex', alignItems: 'center', gap: 9, cursor: 'pointer' }}>
      <div style={{
        width: 14, height: 14, borderRadius: 4, flexShrink: 0,
        border: `1.5px solid ${checked ? accent : (dark ? 'rgba(255,255,255,0.2)' : C.ink5)}`,
        background: checked ? accent : 'transparent',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}>
        {checked && (
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
            <path d="M2 5l2 2 4-4" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        )}
      </div>
      <span style={{ fontSize: 12, color: dark ? 'rgba(255,255,255,0.85)' : C.ink2 }}>{label}</span>
    </label>
  );
}

function ReportPageCover({ accent }) {
  return (
    <div style={{
      width: 360, padding: '34px 38px 36px', background: '#fff',
      boxShadow: '0 8px 24px rgba(0,0,0,0.10), 0 1px 0 rgba(0,0,0,0.04)',
      fontFamily: C.font, color: '#16161B',
    }}>
      <div style={{ borderTop: `4px solid ${accent}`, paddingTop: 18 }}>
        <div style={{ fontSize: 9.5, color: '#6B6B76', letterSpacing: '0.08em', textTransform: 'uppercase', fontWeight: 700 }}>
          原本 · 标书查重评审报告
        </div>
        <div style={{ fontSize: 22, fontWeight: 700, color: '#16161B', marginTop: 10, letterSpacing: '-0.014em', lineHeight: 1.2, fontFamily: C.serif }}>
          市政信息化平台采购<br/>5 家供应商围标核查
        </div>
        <div style={{ fontSize: 10, color: '#6B6B76', marginTop: 12, lineHeight: 1.6 }}>
          评审编号 · 047 / 2026<br/>
          评审专员 · 周明远<br/>
          生成时间 · 2026-05-26 14:32
        </div>
      </div>
      <div style={{
        marginTop: 18, padding: '14px 16px', borderRadius: 8,
        background: C.dangerSoft, border: `1px solid #E8C7C7`,
      }}>
        <div style={{ fontSize: 9, fontWeight: 700, color: C.danger, letterSpacing: '0.06em', textTransform: 'uppercase' }}>
          关键结论
        </div>
        <div style={{ fontSize: 11.5, fontWeight: 700, color: '#16161B', marginTop: 5, lineHeight: 1.5 }}>
          甲、乙两份标书存在 12 组高度雷同条款,整体相似度 92%,
          其中 2 组属于围标嫌疑,建议依据采购法及实施细则进行处理。
        </div>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr 1fr', gap: 10, marginTop: 16 }}>
        {[
          { l: '参评标书', v: '4' },
          { l: '比对数', v: '6' },
          { l: '雷同条款', v: '12' },
          { l: '围标嫌疑', v: '2', c: C.danger },
        ].map((s, i) => (
          <div key={i}>
            <div style={{ fontSize: 8, fontWeight: 700, color: '#6B6B76', letterSpacing: '0.06em', textTransform: 'uppercase' }}>{s.l}</div>
            <div style={{ fontSize: 18, fontWeight: 700, color: s.c || '#16161B', marginTop: 3, letterSpacing: '-0.014em', fontFamily: C.font }}>{s.v}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ReportPageMatrix({ accent }) {
  const docs = [
    { tag: '甲', color: '#4F58A8' },
    { tag: '乙', color: '#0E9A8F' },
    { tag: '丙', color: '#C28430' },
    { tag: '丁', color: '#B54545' },
  ];
  const matrix = [
    [1, 0.92, 0.34, 0.42],
    [0.92, 1, 0.31, 0.40],
    [0.34, 0.31, 1, 0.68],
    [0.42, 0.40, 0.68, 1],
  ];
  const cellColor = (v) => {
    if (v >= 0.9) return C.hi4;
    if (v >= 0.7) return C.hi3;
    if (v >= 0.5) return C.hi2;
    if (v >= 0.3) return C.hi1;
    return C.okSoft;
  };
  return (
    <div style={{
      width: 360, padding: '28px 32px 30px', background: '#fff',
      boxShadow: '0 8px 24px rgba(0,0,0,0.10), 0 1px 0 rgba(0,0,0,0.04)',
      fontFamily: C.font, color: '#16161B',
    }}>
      <div style={{ fontSize: 12.5, fontWeight: 700, color: '#16161B', fontFamily: C.serif }}>
        2. 标书相似度矩阵
      </div>
      <div style={{ fontSize: 10, color: '#6B6B76', marginTop: 5, lineHeight: 1.6 }}>
        基于语义级段落比对,数值表示两份标书之间的整体相似程度。
      </div>
      <div style={{ marginTop: 14, display: 'grid', gridTemplateColumns: '24px repeat(4, 1fr)', gap: 3 }}>
        <div />
        {docs.map(d => (
          <div key={d.tag} style={{ textAlign: 'center', fontSize: 9, fontWeight: 700, color: '#16161B', fontFamily: C.serif }}>{d.tag}</div>
        ))}
        {matrix.map((row, r) => (
          <React.Fragment key={r}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'flex-end', fontSize: 9, fontWeight: 700, color: '#16161B', fontFamily: C.serif }}>{docs[r].tag}</div>
            {row.map((v, c) => {
              const d = r === c;
              return (
                <div key={c} style={{
                  aspectRatio: '1.3 / 1', borderRadius: 3,
                  background: d ? '#F4F2EB' : cellColor(v),
                  color: !d && v >= 0.7 ? '#fff' : '#16161B',
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontSize: 9.5, fontWeight: 700, fontFamily: C.mono,
                }}>{d ? '—' : (v * 100).toFixed(0)}</div>
              );
            })}
          </React.Fragment>
        ))}
      </div>
      <div style={{ marginTop: 16, fontSize: 10, fontWeight: 700, color: '#16161B', fontFamily: C.serif }}>
        2.1 主要发现
      </div>
      <ul style={{ fontSize: 9.5, color: '#3A3A44', paddingLeft: 16, lineHeight: 1.7, marginTop: 4 }}>
        <li><b>甲 × 乙: 92%</b> · 5 个核心章节高度同源,且存在共有错别字、关联工商记录,判定为围标嫌疑</li>
        <li><b>丙 × 丁: 68%</b> · 在项目管理与售后服务两节出现模板雷同,但其他章节差异充分</li>
        <li>其余 4 组的相似度均在 30%-42% 区间,主要落在通用条款</li>
      </ul>
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 8 · Settings — bid-flavored
// ═══════════════════════════════════════════════════════════════
function BidScrSettings({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent} title="设置" sub="个人偏好与账户" />
      <div style={{ flex: 1, overflow: 'auto', padding: '28px 48px 40px' }}>
        <div style={{ maxWidth: 720, margin: '0 auto', display: 'flex', flexDirection: 'column', gap: 20 }}>

          <BidSettingsCard title="账户" cardBg={cardBg} border={border} mute={mute}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
              <CAvatar name="周" color="#7B8FE5" size={48} />
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 14.5, fontWeight: 700, color: ink }}>周明远</div>
                <div style={{ fontSize: 11.5, color: mute, marginTop: 3 }}>采购管理 · 评审专员 · zhou.my@example.com</div>
              </div>
              <CButton kind="secondary" size="md" dark={dark}>编辑资料</CButton>
            </div>
          </BidSettingsCard>

          <BidSettingsCard title="订阅" cardBg={cardBg} border={border} mute={mute}>
            <SettingsRow label="当前方案" sub="标准版 · 每月 8 次评审任务" dark={dark} ink={ink} mute={mute}>
              <CPill bg={C.brandSoft} fg={accent} size={11}>标准版</CPill>
            </SettingsRow>
            <SettingsRow label="本月用量" sub="还剩 5 次,5 月 31 日重置" dark={dark} ink={ink} mute={mute}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
                <div style={{ width: 80, height: 4, background: dark ? 'rgba(255,255,255,0.08)' : C.paper2, borderRadius: 2, overflow: 'hidden' }}>
                  <div style={{ height: '100%', width: '38%', background: accent }} />
                </div>
                <span style={{ fontSize: 11.5, color: mute, fontFamily: C.mono }}>3 / 8</span>
              </div>
            </SettingsRow>
            <SettingsRow label="升级至专业版" sub="¥ 88 / 月 · 不限次数 · 含工商关联数据接口" dark={dark} ink={ink} mute={mute} last>
              <CButton kind="primary" size="sm" accent={accent}>了解一下</CButton>
            </SettingsRow>
          </BidSettingsCard>

          <BidSettingsCard title="检测偏好" cardBg={cardBg} border={border} mute={mute}>
            <SettingsRow label="默认相似度阈值" sub="新任务的初始阈值" dark={dark} ink={ink} mute={mute}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <div style={{ width: 120, height: 3, background: dark ? 'rgba(255,255,255,0.08)' : C.paper3, borderRadius: 2, position: 'relative' }}>
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
            <SettingsRow label="围标嫌疑提示" sub="3 份及以上共同高相似片段触发" dark={dark} ink={ink} mute={mute}>
              <CToggle on accent={accent} />
            </SettingsRow>
            <SettingsRow label="联动工商关联数据" sub="检测时辅助匹配股东关联(专业版)" dark={dark} ink={ink} mute={mute} last>
              <CToggle on={false} accent={accent} />
            </SettingsRow>
          </BidSettingsCard>

          <BidSettingsCard title="隐私" cardBg={cardBg} border={border} mute={mute}>
            <SettingsRow label="本地优先模式" sub="标书不上传至服务器" dark={dark} ink={ink} mute={mute}>
              <CToggle on accent={accent} />
            </SettingsRow>
            <SettingsRow label="自动清理 30 天前的任务" sub="清理原始文件,保留报告" dark={dark} ink={ink} mute={mute} last>
              <CToggle on={false} accent={accent} />
            </SettingsRow>
          </BidSettingsCard>

          <BidSettingsCard title="外观" cardBg={cardBg} border={border} mute={mute}>
            <SettingsRow label="主题色" sub="影响按钮、矩阵与高亮" dark={dark} ink={ink} mute={mute}>
              <div style={{ display: 'flex', gap: 6 }}>
                {['#4F58A8', '#2E5BFF', '#0E9A8F', '#C84D2E', '#2B2D33'].map(c => (
                  <div key={c} style={{
                    width: 22, height: 22, borderRadius: 6, background: c, cursor: 'pointer',
                    boxShadow: c === accent ? `0 0 0 2px ${dark ? '#15151B' : C.paper}, 0 0 0 3.5px ${c}` : 'none',
                  }} />
                ))}
              </div>
            </SettingsRow>
            <SettingsRow label="深色模式" sub="跟随系统" dark={dark} ink={ink} mute={mute} last>
              <CSegControl options={['浅色', '深色', '跟随系统']} value={2} accent={accent} dark={dark} />
            </SettingsRow>
          </BidSettingsCard>

          <div style={{ fontSize: 11, color: mute, textAlign: 'center', padding: '8px 0 20px' }}>
            <CLogo size={20} color={accent} style={{ verticalAlign: 'middle', marginRight: 6 }} />
            <span style={{ verticalAlign: 'middle' }}>原本 · Verum · 标书查重 v2.1.0 · <a style={{ color: mute }}>关于</a> · <a style={{ color: mute }}>反馈</a> · <a style={{ color: mute }}>退出</a></span>
          </div>
        </div>
      </div>
    </div>
  );
}

function BidSettingsCard({ title, children, cardBg, border, mute }) {
  return (
    <div>
      <div style={{
        fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.08em', textTransform: 'uppercase',
        marginBottom: 10, paddingLeft: 4,
      }}>{title}</div>
      <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: '4px 18px' }}>
        {children}
      </div>
    </div>
  );
}

Object.assign(window, { BidScrClusters, BidScrExport, BidScrSettings });
