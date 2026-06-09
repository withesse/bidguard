// bid-b.jsx — Matrix (main view) + Compare (逐对对比) screens

// ═══════════════════════════════════════════════════════════════
// SCREEN 4 · Matrix — THE killer view for multi-doc cross compare
// ═══════════════════════════════════════════════════════════════
function BidScrMatrix({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  const docs = [
    { tag: '甲', short: '智慧城邦', full: '智慧城邦科技_技术响应文件', color: '#4F58A8' },
    { tag: '乙', short: '启明信息', full: '启明信息_投标文件_技术标',  color: '#0E9A8F' },
    { tag: '丙', short: '鸿信科技', full: '鸿信科技_市政平台投标书',   color: '#C28430' },
    { tag: '丁', short: '蓝信电子', full: '蓝信电子_技术标响应',       color: '#B54545' },
  ];
  const matrix = [
    [1.00, 0.92, 0.34, 0.42],
    [0.92, 1.00, 0.31, 0.40],
    [0.34, 0.31, 1.00, 0.68],
    [0.42, 0.40, 0.68, 1.00],
  ];

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="检测报告 · 市政信息化平台采购"
        sub="5 月 26 日 14:32 完成 · 4 份标书 · 6 对比对"
        actions={<>
          <CButton kind="ghost" size="md" icon="share" dark={dark}>分享</CButton>
          <CButton kind="secondary" size="md" icon="download" dark={dark}>导出报告</CButton>
          <CButton kind="primary" size="md" accent={accent} icon="diff">逐对对比</CButton>
        </>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '24px 40px 40px' }}>
        <div style={{ maxWidth: 1200, margin: '0 auto', display: 'flex', flexDirection: 'column', gap: 16 }}>

          {/* Headline */}
          <div style={{
            background: cardBg, border: `1px solid ${border}`, borderRadius: 14,
            padding: '22px 28px', display: 'grid', gridTemplateColumns: '1.6fr 1fr', gap: 32,
          }}>
            <div>
              <CPill bg={C.dangerSoft} fg={C.danger} size={11}>
                <CIcon name="info" size={10} />检出围标嫌疑 · 1 对
              </CPill>
              <div style={{ fontSize: 22, fontWeight: 700, color: ink, marginTop: 10, letterSpacing: '-0.014em', fontFamily: C.serif, lineHeight: 1.3 }}>
                甲、乙两份标书在 5 个章节高度同源,建议人工复核。
              </div>
              <div style={{ fontSize: 13, color: mute, marginTop: 8, lineHeight: 1.65 }}>
                它们的整体相似度达到 <b style={{ color: ink }}>92%</b>,远高于其他两两组合(均值 38%),
                且在不属于通用模板的「技术方案、服务承诺、实施计划」等核心章节出现连续雷同。
              </div>
              <div style={{ display: 'flex', gap: 10, marginTop: 16 }}>
                <CButton kind="primary" size="md" accent={accent} icon="diff">查看 甲 × 乙 对比</CButton>
                <CButton kind="secondary" size="md" icon="folder" dark={dark}>查看重复条款 12 处</CButton>
              </div>
            </div>
            {/* Big peak number */}
            <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', justifyContent: 'center', textAlign: 'right' }}>
              <div style={{ fontSize: 10.5, fontWeight: 700, color: mute, letterSpacing: '0.08em', textTransform: 'uppercase' }}>
                峰值相似度
              </div>
              <div style={{
                fontSize: 96, fontWeight: 700, color: C.danger, letterSpacing: '-0.04em',
                lineHeight: 1, fontFamily: C.font, marginTop: 4,
              }}>92<span style={{ fontSize: 36, color: mute, fontWeight: 500 }}>%</span></div>
              <div style={{ fontSize: 12, color: mute, marginTop: 8 }}>
                出现在 <span style={{ color: ink, fontWeight: 700 }}>甲 ← → 乙</span> 之间
              </div>
            </div>
          </div>

          {/* Matrix + insights */}
          <div style={{ display: 'grid', gridTemplateColumns: '1.4fr 1fr', gap: 16 }}>
            {/* Big matrix */}
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 14, padding: 24 }}>
              <div style={{ display: 'flex', alignItems: 'baseline', marginBottom: 18 }}>
                <span style={{ fontSize: 14, fontWeight: 700, color: ink }}>4 × 4 标书相似度矩阵</span>
                <span style={{ fontSize: 11, color: mute, marginLeft: 8 }}>语义级 · 段落粒度</span>
                <div style={{ flex: 1 }} />
                <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                  <span style={{ fontSize: 10.5, color: mute }}>低</span>
                  <div style={{ width: 92, height: 8, borderRadius: 4, background: `linear-gradient(to right, ${C.okSoft}, ${C.hi1}, ${C.hi2}, ${C.hi3}, ${C.hi4})` }} />
                  <span style={{ fontSize: 10.5, color: mute }}>高</span>
                </div>
              </div>
              <BigMatrix docs={docs} matrix={matrix} dark={dark} ink={ink} mute={mute} />

              {/* legend below */}
              <div style={{ marginTop: 18, paddingTop: 18, borderTop: `1px solid ${border}`, display: 'flex', flexDirection: 'column', gap: 6 }}>
                <div style={{ fontSize: 10.5, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 4 }}>
                  对比结果一览
                </div>
                {[
                  { pair: '甲 × 乙', pct: 92, label: '围标嫌疑', c: C.danger, secs: '§3 技术方案 · §5 服务承诺 · §7 实施计划' },
                  { pair: '丙 × 丁', pct: 68, label: '高相似 · 模板雷同', c: C.hi3, secs: '§4 项目管理 · §6 售后服务' },
                  { pair: '甲 × 丁', pct: 42, label: '中相似', c: C.hi2, secs: '§6 售后服务' },
                  { pair: '甲 × 丙', pct: 34, label: '中相似', c: C.hi2, secs: '§2 公司介绍' },
                  { pair: '乙 × 丁', pct: 40, label: '中相似', c: C.hi2, secs: '§6 售后服务' },
                  { pair: '乙 × 丙', pct: 31, label: '低相似', c: C.hi1, secs: '差异充分' },
                ].map((row, i) => (
                  <div key={i} style={{
                    display: 'grid', gridTemplateColumns: '64px 60px 1fr 110px',
                    gap: 12, alignItems: 'center', padding: '6px 8px',
                    borderRadius: 6, background: i === 0 ? (dark ? 'rgba(181,69,69,0.10)' : C.dangerSoft) : 'transparent',
                  }}>
                    <span style={{ fontFamily: C.serif, fontWeight: 700, fontSize: 12.5, color: ink }}>{row.pair}</span>
                    <span style={{ fontSize: 13, fontWeight: 700, color: row.c, fontFamily: C.mono, letterSpacing: '-0.005em' }}>{row.pct}%</span>
                    <span style={{ fontSize: 11.5, color: mute }}>{row.secs}</span>
                    <CPill bg={`${row.c}1a`} fg={row.c} size={10.5}>{row.label}</CPill>
                  </div>
                ))}
              </div>
            </div>

            {/* Insights + doc legend */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
              {/* Document legend */}
              <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
                <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 12 }}>参评标书</div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {docs.map((d, i) => (
                    <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
                      <div style={{
                        width: 22, height: 22, borderRadius: 5,
                        background: d.color, color: '#fff',
                        display: 'flex', alignItems: 'center', justifyContent: 'center',
                        fontSize: 12, fontWeight: 700, fontFamily: C.serif, flexShrink: 0,
                      }}>{d.tag}</div>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontSize: 12, fontWeight: 600, color: ink, lineHeight: 1.3 }}>{d.short}</div>
                        <div style={{ fontSize: 10.5, color: mute, marginTop: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{d.full}</div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              {/* Insights */}
              <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
                <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 12 }}>关键洞察</div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 11 }}>
                  {[
                    { tag: '围标', fg: C.danger, bg: C.dangerSoft, title: '甲 × 乙 高度同源', body: '5 个核心章节相似度 ≥ 85%,且双方在「项目难点应对」一节出现完全相同的措辞,该段落不属于行业通用模板。' },
                    { tag: '模板', fg: C.warn, bg: C.warnSoft, title: '丙 × 丁 共用集成模板', body: '在「项目管理」「售后服务」两节相似度 60-75%,推断使用了同一份行业模板,但其他章节差异充分。' },
                    { tag: '差异', fg: C.ok, bg: C.okSoft, title: '乙 × 丙 差异充分', body: '31% 的相似度主要落在通用条款,核心技术方案完全独立编写,可判定为独立投标。' },
                  ].map((ins, i) => (
                    <div key={i} style={{
                      padding: 12, borderRadius: 8,
                      background: dark ? 'rgba(255,255,255,0.025)' : C.paper2,
                      border: `1px solid ${border}`,
                    }}>
                      <CPill bg={ins.bg} fg={ins.fg} size={10}>{ins.tag}</CPill>
                      <div style={{ fontSize: 12.5, fontWeight: 700, color: ink, marginTop: 7 }}>{ins.title}</div>
                      <div style={{ fontSize: 11, color: mute, marginTop: 4, lineHeight: 1.6 }}>{ins.body}</div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function BigMatrix({ docs, matrix, dark, ink, mute }) {
  const cellColor = (v) => {
    if (v >= 0.9) return C.hi4;
    if (v >= 0.7) return C.hi3;
    if (v >= 0.5) return C.hi2;
    if (v >= 0.3) return C.hi1;
    return C.okSoft;
  };
  const cellFg = (v) => v >= 0.7 ? '#fff' : ink;
  return (
    <div style={{ display: 'grid', gridTemplateColumns: `92px repeat(${docs.length}, 1fr)`, gap: 6 }}>
      <div />
      {docs.map((d, i) => (
        <div key={i} style={{ textAlign: 'center', display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
          <div style={{
            width: 26, height: 26, borderRadius: 6,
            background: d.color, color: '#fff',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            fontSize: 14, fontWeight: 700, fontFamily: C.serif,
          }}>{d.tag}</div>
          <div style={{ fontSize: 10.5, color: mute, fontWeight: 600 }}>{d.short}</div>
        </div>
      ))}
      {docs.map((d, r) => (
        <React.Fragment key={r}>
          <div style={{
            display: 'flex', alignItems: 'center', justifyContent: 'flex-end', gap: 8, paddingRight: 8,
          }}>
            <div style={{ textAlign: 'right' }}>
              <div style={{ fontSize: 11.5, fontWeight: 700, color: ink, fontFamily: C.serif }}>{d.tag}</div>
              <div style={{ fontSize: 10, color: mute }}>{d.short}</div>
            </div>
            <div style={{
              width: 22, height: 22, borderRadius: 5,
              background: d.color, color: '#fff',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 12, fontWeight: 700, fontFamily: C.serif,
            }}>{d.tag}</div>
          </div>
          {matrix[r].map((v, c) => {
            const diag = r === c;
            const isHot = v >= 0.9 && !diag;
            return (
              <div key={c} style={{
                aspectRatio: '1.3 / 1', borderRadius: 8,
                background: diag ? (dark ? 'rgba(255,255,255,0.04)' : C.paper2) : cellColor(v),
                color: diag ? mute : cellFg(v),
                display: 'flex', flexDirection: 'column',
                alignItems: 'center', justifyContent: 'center',
                cursor: diag ? 'default' : 'pointer',
                boxShadow: isHot ? `0 0 0 2px ${C.danger}` : 'none',
                position: 'relative',
              }}>
                {diag ? '—' : (
                  <>
                    <span style={{ fontSize: 22, fontWeight: 700, fontFamily: C.mono, letterSpacing: '-0.014em', lineHeight: 1 }}>
                      {(v * 100).toFixed(0)}
                    </span>
                    <span style={{ fontSize: 10, opacity: 0.7, fontWeight: 600, marginTop: 3 }}>%</span>
                  </>
                )}
                {isHot && (
                  <span style={{
                    position: 'absolute', top: 5, right: 6,
                    fontSize: 9.5, fontWeight: 700, color: '#fff',
                    background: 'rgba(0,0,0,0.25)', padding: '1px 5px', borderRadius: 999,
                    letterSpacing: '0.04em',
                  }}>围标</span>
                )}
              </div>
            );
          })}
        </React.Fragment>
      ))}
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 5 · Compare — pick two of the 2-5 docs, see side-by-side
// ═══════════════════════════════════════════════════════════════
function BidScrCompare({ accent = C.brand, dark = false, hi = 'amber' }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const paperBg = dark ? '#22222A' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;
  const HI = hiSchemeC(hi);

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="逐对对比"
        sub="市政信息化平台采购 · 第 4 处 / 共 12 处雷同"
        actions={<>
          <CButton kind="ghost" size="md" icon="quote" dark={dark}>抽取条款</CButton>
          <CButton kind="primary" size="md" accent={accent} icon="check">标记为已复核</CButton>
        </>}
      />

      {/* Pair selector + toolbar */}
      <div style={{
        height: 56, flexShrink: 0, padding: '0 24px',
        display: 'flex', alignItems: 'center', gap: 14,
        borderBottom: `1px solid ${border}`,
        background: dark ? 'rgba(255,255,255,0.02)' : C.paper2,
      }}>
        <span style={{ fontSize: 11, fontWeight: 600, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>
          对比组合
        </span>
        <PairSelector dark={dark} accent={accent} ink={ink} mute={mute} border={border} />
        <div style={{ flex: 1 }} />
        <CButton kind="secondary" size="sm" icon="chevL" dark={dark}>上一处</CButton>
        <span style={{ fontSize: 11.5, color: mute, fontFamily: C.mono }}>
          <span style={{ color: ink, fontWeight: 700 }}>4</span> / 12 处
        </span>
        <CButton kind="secondary" size="sm" iconRight="chevR" dark={dark}>下一处</CButton>
        <div style={{ width: 1, height: 18, background: border }} />
        <CPill bg={HI.hi3soft} fg={HI.hi3} size={11}>≥60% 高</CPill>
        <CPill bg={HI.hi2soft} fg={HI.hi2} size={11}>30-60% 中</CPill>
      </div>

      <div style={{ flex: 1, minHeight: 0, display: 'grid', gridTemplateColumns: '1fr 1fr', position: 'relative' }}>
        <BidDocPane side="left" tag="甲" tagColor="#4F58A8"
          doc={{ name: '智慧城邦科技_技术响应文件.pdf', meta: '86 页 · §3 技术方案 · 第 12 页' }}
          dark={dark} border={border} paperBg={paperBg} accent={accent} ink={ink} mute={mute} HI={HI}
          left={true}
        />
        <BidDocPane side="right" tag="乙" tagColor="#0E9A8F"
          doc={{ name: '启明信息_投标文件_技术标.docx', meta: '72 页 · §3 技术方案 · 第 9 页' }}
          dark={dark} border={border} paperBg={paperBg} accent={accent} ink={ink} mute={mute} HI={HI}
          left={false}
        />
        <div style={{
          position: 'absolute', left: '50%', top: 0, bottom: 0, width: 1,
          background: border, transform: 'translateX(-0.5px)',
        }} />
      </div>
    </div>
  );
}

function PairSelector({ dark, accent, ink, mute, border }) {
  const pairs = [
    { left: '甲', leftColor: '#4F58A8', right: '乙', rightColor: '#0E9A8F', pct: 92, active: true },
    { left: '甲', leftColor: '#4F58A8', right: '丙', rightColor: '#C28430', pct: 34 },
    { left: '甲', leftColor: '#4F58A8', right: '丁', rightColor: '#B54545', pct: 42 },
    { left: '乙', leftColor: '#0E9A8F', right: '丙', rightColor: '#C28430', pct: 31 },
    { left: '乙', leftColor: '#0E9A8F', right: '丁', rightColor: '#B54545', pct: 40 },
    { left: '丙', leftColor: '#C28430', right: '丁', rightColor: '#B54545', pct: 68 },
  ];
  return (
    <div style={{ display: 'flex', gap: 4, alignItems: 'center' }}>
      {pairs.map((p, i) => (
        <div key={i} style={{
          display: 'flex', alignItems: 'center', gap: 6,
          padding: '5px 10px 5px 6px', borderRadius: 7,
          background: p.active ? (dark ? 'rgba(255,255,255,0.08)' : '#fff') : 'transparent',
          border: `1px solid ${p.active ? accent : 'transparent'}`,
          boxShadow: p.active && !dark ? '0 1px 2px rgba(0,0,0,0.06)' : 'none',
          cursor: 'pointer',
        }}>
          <div style={{ display: 'flex', gap: 2 }}>
            <div style={{ width: 18, height: 18, borderRadius: 4, background: p.leftColor, color: '#fff', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 10.5, fontWeight: 700, fontFamily: C.serif }}>{p.left}</div>
            <div style={{ width: 18, height: 18, borderRadius: 4, background: p.rightColor, color: '#fff', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 10.5, fontWeight: 700, fontFamily: C.serif }}>{p.right}</div>
          </div>
          <span style={{
            fontSize: 11.5, fontWeight: 700, fontFamily: C.mono,
            color: p.pct >= 80 ? C.danger : p.pct >= 60 ? C.hi3 : p.pct >= 30 ? C.hi2 : C.hi1,
          }}>{p.pct}%</span>
        </div>
      ))}
    </div>
  );
}

function BidDocPane({ side, tag, tagColor, doc, dark, border, paperBg, accent, ink, mute, HI, left }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', minWidth: 0, background: dark ? '#181820' : C.paper }}>
      {/* doc header with tag */}
      <div style={{
        padding: '12px 20px', borderBottom: `1px solid ${border}`,
        display: 'flex', alignItems: 'center', gap: 10,
        background: dark ? '#1F1F26' : C.white,
      }}>
        <div style={{
          width: 24, height: 24, borderRadius: 6, background: tagColor, color: '#fff',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontSize: 13, fontWeight: 700, fontFamily: C.serif, flexShrink: 0,
        }}>{tag}</div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 12.5, fontWeight: 600, color: ink, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {doc.name}
          </div>
          <div style={{ fontSize: 10.5, color: mute, marginTop: 1 }}>{doc.meta}</div>
        </div>
        <CIcon name="search" size={13} style={{ color: mute }} />
        <CIcon name="sliders" size={13} style={{ color: mute }} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: '24px 30px', display: 'flex', justifyContent: 'center' }}>
        <div style={{
          maxWidth: 540, width: '100%',
          background: paperBg, borderRadius: 8,
          border: `1px solid ${border}`,
          boxShadow: dark ? 'none' : C.shadow.sm,
          padding: '36px 44px 32px',
          fontSize: 13, lineHeight: 1.88, color: ink,
          fontFamily: C.font, letterSpacing: '-0.003em',
        }}>
          {left ? <BidBodyA HI={HI} mute={mute} dark={dark} accent={accent} /> : <BidBodyB HI={HI} mute={mute} dark={dark} />}
        </div>
      </div>
    </div>
  );
}

function BidBodyA({ HI, mute, dark, accent }) {
  return <>
    <div style={{ fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§3 技术方案</div>
    <h2 style={{ fontSize: 19, fontWeight: 700, margin: '6px 0 14px', letterSpacing: '-0.014em', color: dark ? '#fff' : C.ink }}>
      3.2 总体架构设计
    </h2>
    <p>
      本项目采用「分层解耦、微服务、统一服务总线」的总体架构。
      <CHSpan HI={HI} level={4} refLabel="①">系统自下而上划分为基础设施层、数据资源层、应用支撑层与业务应用层,各层之间通过标准化接口解耦,
      所有业务能力对外以 API 网关统一暴露,确保横向可扩展与纵向可演进</CHSpan>
      。
    </p>
    <p>
      在数据层,
      <CHSpan HI={HI} level={4} refLabel="②">采用读写分离与多级缓存机制,关键业务数据在 PostgreSQL 主库 + 只读副本 + Redis 缓存的三级架构下,
      保证 99.99% 可用性与 200ms 内的端到端响应</CHSpan>
      。
    </p>
    <div style={{ marginTop: 14, fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§3.3 安全合规</div>
    <p style={{ marginTop: 6 }}>
      <CHSpan HI={HI} level={3} refLabel="③">全平台遵循等保 2.0 三级与 ISO 27001 标准,所有数据在传输与静止状态下均通过国密 SM4 加密,
      密钥由本地 HSM 派生并按月轮换</CHSpan>
      。我们将与甲方信息部门共同建立 7×24 安全响应机制。
    </p>

    {/* Side annotation pointing to ①② */}
    <div style={{
      marginTop: 18, padding: 14, borderRadius: 10,
      background: dark ? 'rgba(181,69,69,0.12)' : C.dangerSoft,
      border: `1px solid ${dark ? 'rgba(181,69,69,0.4)' : '#E8C7C7'}`,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginBottom: 8 }}>
        <CIcon name="info" size={13} style={{ color: C.danger }} />
        <span style={{ fontSize: 11, fontWeight: 700, color: C.danger, letterSpacing: '0.04em', textTransform: 'uppercase' }}>
          围标嫌疑 · ①② 句
        </span>
        <span style={{ flex: 1 }} />
        <span style={{ fontSize: 11, color: mute, fontFamily: C.mono }}>与乙: <b style={{ color: C.danger }}>92%</b></span>
      </div>
      <div style={{ fontSize: 12, color: dark ? '#fff' : C.ink, lineHeight: 1.6 }}>
        ① 句在 乙 标书 §3.1 中以完全相同语序出现,且包含 2 处罕见错别字一致;
        ② 句的「200ms 内的端到端响应」措辞与 乙 §3.2 末段一字不差。
      </div>
      <div style={{ display: 'flex', gap: 6, marginTop: 10 }}>
        <CButton kind="ghost" size="sm" dark={dark}>查看 6 处一致细节</CButton>
        <div style={{ flex: 1 }} />
        <CButton kind="secondary" size="sm" icon="quote" dark={dark}>抽取为证据</CButton>
      </div>
    </div>
  </>;
}

function BidBodyB({ HI, mute, dark }) {
  return <>
    <div style={{ fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§3 技术方案</div>
    <h2 style={{ fontSize: 19, fontWeight: 700, margin: '6px 0 14px', letterSpacing: '-0.014em', color: dark ? '#fff' : C.ink }}>
      3.1 整体技术架构
    </h2>
    <p>
      本项目采取「分层解耦、微服务、统一服务总线」的整体架构思路。
      <CHSpan HI={HI} level={4} refLabel="①">系统自下而上划分为基础设施层、数据资源层、应用支撑层与业务应用层,各层之间通过标准化接口解耦,
      所有业务能力对外以 API 网关统一暴露,确保横向可扩展与纵向可演进</CHSpan>
      ,以适应未来三年的业务发展需要。
    </p>
    <p>
      数据层方面,
      <CHSpan HI={HI} level={4} refLabel="②">本方案采用读写分离与多级缓存机制,关键业务数据在 PostgreSQL 主库 + 只读副本 + Redis 缓存的三级架构下,
      保证 99.99% 可用性与 200ms 内的端到端响应</CHSpan>
      ,经我司在多个同类项目中验证。
    </p>
    <div style={{ marginTop: 14, fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§3.2 安全体系</div>
    <p style={{ marginTop: 6 }}>
      <CHSpan HI={HI} level={3} refLabel="③">本平台严格遵循等保 2.0 三级与 ISO 27001 标准,在数据加密方面采用国密 SM4 算法,
      密钥由本地 HSM 派生并按月轮换</CHSpan>
      。
    </p>
    <p style={{ color: mute, fontSize: 12 }}>
      （后续 §3.3 章节为乙方独立设计的容灾与混合云架构,与甲方文档无重叠。）
    </p>
  </>;
}

// reuse helper from screens.jsx
function CHSpan({ text, level = 2, ref: refLabel, HI }) {
  const color = HI['hi' + level];
  return (
    <span style={{
      borderBottom: `2px solid ${color}`,
      paddingBottom: 1, position: 'relative', cursor: 'pointer',
    }}>
      {text}
      {refLabel && (
        <sup style={{
          marginLeft: 2, padding: '0 4px', borderRadius: 3,
          background: color, color: '#fff', fontSize: 9, fontWeight: 700,
          verticalAlign: 'super', fontFamily: C.mono,
        }}>{refLabel}</sup>
      )}
    </span>
  );
}

function hiSchemeC(name) {
  if (name === 'rose') return {
    hi1: '#E89FAE', hi2: '#D86E84', hi3: '#B83F5E', hi4: '#8C2444',
    hi1soft: '#F8D9DF', hi2soft: '#F4C5CF', hi3soft: '#EFAFBE', hi4soft: '#E89DAE',
  };
  if (name === 'blue') return {
    hi1: '#A6BDDE', hi2: '#6B8BC4', hi3: '#3D63A8', hi4: '#1E4080',
    hi1soft: '#D8E2F1', hi2soft: '#BDCFE7', hi3soft: '#9FB8DA', hi4soft: '#7E9DCB',
  };
  return {
    hi1: C.hi1, hi2: C.hi2, hi3: C.hi3, hi4: C.hi4,
    hi1soft: C.hi1Soft, hi2soft: C.hi2Soft, hi3soft: C.hi3Soft, hi4soft: C.hi4Soft,
  };
}

Object.assign(window, { BidScrMatrix, BidScrCompare });
