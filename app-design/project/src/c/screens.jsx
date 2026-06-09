// c-screens.jsx — All 6 screens for 「原本」C-end app
// Home, Scanning, Report, Compare, Docs, Settings

// ═══════════════════════════════════════════════════════════════
// SCREEN 1 · Home — hero dropzone + recent docs
// ═══════════════════════════════════════════════════════════════
function CScrHome({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="原本"
        sub="文档查重 · 仅在你的电脑上运行"
        actions={<CButton kind="ghost" size="md" icon="info" dark={dark}>什么是查重</CButton>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '32px 48px 40px' }}>
        {/* Headline */}
        <div style={{ maxWidth: 760, marginBottom: 28 }}>
          <div style={{ fontSize: 11, color: accent, fontWeight: 700, letterSpacing: '0.08em', textTransform: 'uppercase' }}>
            五月二十六日 · 周二
          </div>
          <div style={{
            fontSize: 28, fontWeight: 700, color: ink, marginTop: 8,
            letterSpacing: '-0.018em', lineHeight: 1.25,
            fontFamily: C.serif,
          }}>
            把这一稿,放到天平上称一称。
          </div>
          <div style={{ fontSize: 13.5, color: mute, marginTop: 8, lineHeight: 1.6 }}>
            上传一份待检测文档,我会和你之前的稿件、网络公开内容做比对,
            找出可能让你显得"在重复自己"的段落。
          </div>
        </div>

        {/* Big dropzone */}
        <div style={{
          maxWidth: 760, height: 220,
          border: `1.5px dashed ${dark ? 'rgba(255,255,255,0.18)' : C.ink5}`,
          borderRadius: 16, background: dark ? 'rgba(255,255,255,0.02)' : C.white,
          display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
          gap: 14, transition: 'all 0.2s', position: 'relative', overflow: 'hidden',
        }}>
          {/* faint accent corner */}
          <div style={{
            position: 'absolute', top: -40, right: -40, width: 160, height: 160,
            borderRadius: '50%', background: `radial-gradient(circle, ${accent}14, transparent 70%)`,
          }} />
          <div style={{
            width: 56, height: 56, borderRadius: 14, background: `${accent}14`,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            color: accent,
          }}>
            <CIcon name="upload" size={26} strokeWidth={1.6} />
          </div>
          <div style={{ textAlign: 'center' }}>
            <div style={{ fontSize: 14, fontWeight: 600, color: ink }}>
              拖入文件,或 <span style={{ color: accent, textDecoration: 'underline', textDecorationColor: `${accent}66`, textUnderlineOffset: 3, cursor: 'pointer' }}>从电脑选择</span>
            </div>
            <div style={{ fontSize: 11.5, color: mute, marginTop: 5 }}>
              支持 Word · PDF · PPT · Excel · Markdown / TXT &nbsp;·&nbsp; 单份不超过 50 MB
            </div>
          </div>
          <div style={{ display: 'flex', gap: 7, marginTop: 2 }}>
            {['DOCX', 'PDF', 'PPTX', 'XLSX', 'MD', 'TXT'].map(t => (
              <span key={t} style={{
                fontSize: 9.5, fontWeight: 700, letterSpacing: '0.04em',
                padding: '2px 6px', borderRadius: 3,
                background: dark ? 'rgba(255,255,255,0.06)' : C.paper2,
                color: dark ? 'rgba(255,255,255,0.6)' : C.ink3,
                fontFamily: C.mono,
              }}>{t}</span>
            ))}
          </div>
        </div>

        {/* Last detection summary */}
        <div style={{
          maxWidth: 760, marginTop: 28, padding: '18px 22px',
          background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
          display: 'flex', alignItems: 'center', gap: 22,
        }}>
          <div>
            <div style={{ fontSize: 10.5, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', fontWeight: 600 }}>
              上次检测
            </div>
            <div style={{ fontSize: 13.5, fontWeight: 600, color: ink, marginTop: 5, letterSpacing: '-0.005em' }}>
              2026Q2 业务复盘报告 v3
            </div>
            <div style={{ fontSize: 11, color: mute, marginTop: 3 }}>昨日 17:42 · DOCX · 14 页</div>
          </div>
          <div style={{ width: 1, height: 44, background: border, marginLeft: 'auto' }} />
          <div>
            <div style={{ fontSize: 10.5, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', fontWeight: 600 }}>
              相似度
            </div>
            <div style={{
              fontSize: 24, fontWeight: 700, color: C.hi2, marginTop: 3,
              letterSpacing: '-0.018em', lineHeight: 1, fontFamily: C.font,
            }}>22<span style={{ fontSize: 13, color: mute, fontWeight: 500 }}>.4%</span></div>
            <div style={{ fontSize: 10.5, color: mute, marginTop: 3 }}>低风险 · 12 处提示</div>
          </div>
          <CButton kind="secondary" size="md" icon="eye" dark={dark} style={{ marginLeft: 14 }}>查看报告</CButton>
        </div>

        {/* Recent docs grid */}
        <div style={{ maxWidth: 1100, marginTop: 36 }}>
          <div style={{ display: 'flex', alignItems: 'baseline', marginBottom: 14 }}>
            <span style={{ fontSize: 13.5, fontWeight: 700, color: ink }}>最近检测</span>
            <span style={{ fontSize: 11.5, color: mute, marginLeft: 10 }}>本周 6 份</span>
            <div style={{ flex: 1 }} />
            <CButton kind="ghost" size="sm" dark={dark}>查看全部 →</CButton>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 12 }}>
            {[
              { type: 'docx', name: '2026Q2 业务复盘报告 v3', pct: 22.4, sev: 'low',  meta: '昨日 17:42 · 14 页',  pages: 12 },
              { type: 'pptx', name: '产品方案-客户演示 v3',     pct: 41.0, sev: 'mid',  meta: '5 月 24 日 · 38 页', pages: 19 },
              { type: 'docx', name: '个人简历 林一帆 v8',       pct: 67.8, sev: 'high', meta: '5 月 22 日 · 2 页',   pages: 7 },
              { type: 'pdf',  name: '行业研究笔记 · 数据分析',  pct: 13.5, sev: 'low',  meta: '5 月 19 日 · 9 页',   pages: 4 },
              { type: 'docx', name: '部门月度汇报 · 5月',       pct: 35.2, sev: 'mid',  meta: '5 月 18 日 · 6 页',   pages: 11 },
              { type: 'docx', name: '产品需求文档 · v0.4',       pct: 8.1,  sev: 'low',  meta: '5 月 14 日 · 21 页',  pages: 3 },
            ].map((d, i) => (
              <RecentDocCard key={i} d={d} dark={dark} cardBg={cardBg} border={border} ink={ink} mute={mute} accent={accent} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function RecentDocCard({ d, dark, cardBg, border, ink, mute, accent }) {
  const sev = { low: { c: C.ok, label: '低相似' }, mid: { c: C.hi2, label: '中相似' }, high: { c: C.hi3, label: '高相似' } }[d.sev];
  return (
    <div style={{
      background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
      padding: 16, cursor: 'pointer', position: 'relative',
    }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 9 }}>
        <CDocChip type={d.type} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            fontSize: 12.5, fontWeight: 600, color: ink, lineHeight: 1.35,
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}>{d.name}</div>
          <div style={{ fontSize: 10.5, color: mute, marginTop: 3 }}>{d.meta}</div>
        </div>
      </div>
      {/* big number */}
      <div style={{ display: 'flex', alignItems: 'flex-end', justifyContent: 'space-between', marginTop: 16 }}>
        <div>
          <div style={{
            fontSize: 30, fontWeight: 700, color: sev.c,
            letterSpacing: '-0.024em', lineHeight: 1, fontFamily: C.font,
          }}>
            {Math.floor(d.pct)}<span style={{ fontSize: 14, color: mute, fontWeight: 500 }}>.{Math.round((d.pct % 1) * 10)}%</span>
          </div>
          <div style={{ fontSize: 10.5, color: mute, marginTop: 6, fontWeight: 600 }}>
            <span style={{ color: sev.c }}>●</span> {sev.label} · {d.pages} 处提示
          </div>
        </div>
        <CIcon name="chevR" size={13} style={{ color: mute, marginBottom: 4 }} />
      </div>
      {/* mini bar */}
      <div style={{ height: 3, background: dark ? 'rgba(255,255,255,0.05)' : C.paper2, borderRadius: 2, marginTop: 12, overflow: 'hidden' }}>
        <div style={{ height: '100%', width: `${d.pct}%`, background: sev.c }} />
      </div>
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 2 · Scanning — calm, single-document, slow animation feel
// ═══════════════════════════════════════════════════════════════
function CScrScan({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="正在检测"
        sub="可以离开这个界面 · 完成时通知你"
        actions={<>
          <CButton kind="ghost" size="md" dark={dark}>暂停</CButton>
          <CButton kind="secondary" size="md" dark={dark}>取消</CButton>
        </>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '32px 48px 40px' }}>
        <div style={{
          maxWidth: 820, margin: '0 auto',
          background: cardBg, border: `1px solid ${border}`, borderRadius: 16,
          padding: '36px 40px',
        }}>
          {/* Doc header */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <CDocChip type="docx" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 14, fontWeight: 600, color: ink, letterSpacing: '-0.005em' }}>
                2026Q2 业务复盘报告 v4 · 草稿.docx
              </div>
              <div style={{ fontSize: 11.5, color: mute, marginTop: 3 }}>
                14 页 · 8,420 字 · 2 分 03 秒前开始
              </div>
            </div>
            <CPill bg={C.brandSoft} fg={accent} size={11}>
              <CIcon name="lock" size={10} />本地检测
            </CPill>
          </div>

          {/* Big scanning visual — calm document with scan line */}
          <div style={{
            marginTop: 28, height: 200, position: 'relative',
            background: dark ? 'rgba(255,255,255,0.025)' : C.paper2, borderRadius: 12,
            overflow: 'hidden',
            border: `1px solid ${border}`,
          }}>
            {/* mock document lines */}
            <div style={{ position: 'absolute', inset: '22px 44px', display: 'flex', flexDirection: 'column', gap: 8 }}>
              {[100, 92, 70, 86, 60, 95, 78, 88, 50, 72, 90, 64].map((w, i) => (
                <div key={i} style={{
                  height: 6, width: `${w}%`, borderRadius: 2,
                  background: dark ? 'rgba(255,255,255,0.08)' : C.ink5, opacity: 0.6,
                }} />
              ))}
            </div>
            {/* scan line */}
            <div style={{
              position: 'absolute', left: 0, right: 0, height: 64,
              top: '42%',
              background: `linear-gradient(to bottom, transparent, ${accent}22 40%, ${accent}33 50%, ${accent}22 60%, transparent)`,
              animation: 'cscan 2.6s ease-in-out infinite',
            }} />
            <div style={{
              position: 'absolute', left: 0, right: 0, height: 2, top: '50%',
              background: accent, opacity: 0.6,
              animation: 'cscanline 2.6s ease-in-out infinite',
            }} />
            {/* page counter */}
            <div style={{
              position: 'absolute', top: 12, right: 14,
              fontSize: 10.5, color: mute, fontFamily: C.mono,
              background: dark ? '#1A1A22' : '#fff', padding: '3px 8px', borderRadius: 4,
              border: `1px solid ${border}`,
            }}>
              第 <span style={{ color: ink, fontWeight: 700 }}>9</span> / 14 页
            </div>
          </div>

          {/* Progress */}
          <div style={{ marginTop: 24 }}>
            <div style={{ display: 'flex', alignItems: 'baseline' }}>
              <span style={{ fontSize: 13, fontWeight: 600, color: ink }}>读取与比对中</span>
              <span style={{ flex: 1 }} />
              <span style={{ fontSize: 18, fontWeight: 700, color: ink, fontFamily: C.font, letterSpacing: '-0.014em' }}>
                64<span style={{ fontSize: 11, color: mute, fontWeight: 500 }}>%</span>
              </span>
            </div>
            <div style={{ height: 4, background: dark ? 'rgba(255,255,255,0.08)' : C.paper2, borderRadius: 2, marginTop: 8, overflow: 'hidden' }}>
              <div style={{ height: '100%', width: '64%', background: accent, transition: 'width 0.4s' }} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 8, fontSize: 11, color: mute }}>
              <span>已比对 9 / 14 页</span>
              <span>预计剩余 1 分 12 秒</span>
            </div>
          </div>

          {/* Pipeline */}
          <div style={{ marginTop: 28, display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12 }}>
            {[
              { label: '解析文档',     status: 'done' },
              { label: '段落语义化',   status: 'done' },
              { label: '比对查重源',   status: 'running', pct: 64 },
              { label: '生成报告',     status: 'pending' },
            ].map((s, i) => (
              <ScanStep key={i} s={s} dark={dark} accent={accent} ink={ink} mute={mute} border={border} />
            ))}
          </div>

          {/* Live findings */}
          <div style={{ marginTop: 24, padding: '14px 16px', borderRadius: 10, background: dark ? 'rgba(255,255,255,0.025)' : C.paper2, border: `1px solid ${border}` }}>
            <div style={{ fontSize: 10.5, fontWeight: 700, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 9 }}>
              已发现 · 8 处提示
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              {[
                { sec: '§2.1', text: '本季度业务增长主要来源于…', pct: 68, sev: 'high' },
                { sec: '§3 客户结构变化', text: '核心客户复购率较上季度…', pct: 42, sev: 'mid' },
                { sec: '§4.2 风险与应对', text: '在合规与数据安全方面…', pct: 36, sev: 'mid' },
              ].map((f, i) => (
                <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 11.5 }}>
                  <span style={{ color: mute, fontFamily: C.mono, width: 96, flexShrink: 0 }}>{f.sec}</span>
                  <span style={{ color: ink, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>"{f.text}"</span>
                  <span style={{ fontSize: 12, fontWeight: 700, color: f.sev === 'high' ? C.hi3 : C.hi2, fontFamily: C.mono }}>{f.pct}%</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function ScanStep({ s, dark, accent, ink, mute, border }) {
  const done = s.status === 'done';
  const running = s.status === 'running';
  return (
    <div style={{
      padding: 12, borderRadius: 10,
      background: dark ? 'rgba(255,255,255,0.02)' : '#fff',
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
        <span style={{ fontSize: 11.5, fontWeight: 600, color: done ? mute : ink }}>{s.label}</span>
      </div>
      {running && (
        <div style={{ height: 3, background: dark ? 'rgba(255,255,255,0.08)' : C.paper2, borderRadius: 2, marginTop: 9, overflow: 'hidden' }}>
          <div style={{ height: '100%', width: `${s.pct}%`, background: accent }} />
        </div>
      )}
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 3 · Report — big headline number, breakdown, sources
// ═══════════════════════════════════════════════════════════════
function CScrReport({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="检测报告"
        sub="2026Q2 业务复盘报告 v3 · 昨日 17:42 完成"
        actions={<>
          <CButton kind="ghost" size="md" icon="share" dark={dark}>分享</CButton>
          <CButton kind="secondary" size="md" icon="download" dark={dark}>导出 PDF</CButton>
          <CButton kind="primary" size="md" icon="eye" accent={accent}>逐段对比</CButton>
        </>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '28px 48px 40px' }}>
        <div style={{ maxWidth: 1100, margin: '0 auto', display: 'flex', flexDirection: 'column', gap: 18 }}>

          {/* Headline */}
          <div style={{
            background: cardBg, border: `1px solid ${border}`, borderRadius: 16,
            padding: '32px 36px', display: 'grid', gridTemplateColumns: '1.2fr 1fr', gap: 36,
          }}>
            <div>
              <CPill bg={C.warnSoft} fg={C.warn} size={11}>
                中等相似 · 建议复核
              </CPill>
              <div style={{ fontSize: 20, fontWeight: 700, color: ink, marginTop: 12, letterSpacing: '-0.014em', fontFamily: C.serif }}>
                有 12 处地方,和你以前写过的稿子相似。
              </div>
              <div style={{ fontSize: 13, color: mute, marginTop: 8, lineHeight: 1.6 }}>
                其中 4 处属于段落级雷同,建议改写或直接引用。剩余 8 处多为常用表述,可以保留,但
                请避免连续出现。
              </div>
              <div style={{ display: 'flex', gap: 10, marginTop: 18 }}>
                <CButton kind="primary" size="md" accent={accent} icon="ai">AI 一键改写</CButton>
                <CButton kind="secondary" size="md" icon="diff" dark={dark}>逐段对比</CButton>
              </div>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', justifyContent: 'center' }}>
              <div style={{ fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.08em', textTransform: 'uppercase' }}>
                总体相似度
              </div>
              <div style={{
                fontSize: 96, fontWeight: 700, color: C.hi2, letterSpacing: '-0.04em',
                lineHeight: 1, fontFamily: C.font, marginTop: 6,
              }}>
                22<span style={{ fontSize: 36, color: mute, fontWeight: 500 }}>.4%</span>
              </div>
              <div style={{ fontSize: 12, color: mute, marginTop: 10 }}>
                同等工作经验 · 平均 <span style={{ color: ink, fontWeight: 700 }}>34%</span>
              </div>
            </div>
          </div>

          {/* Breakdown */}
          <div style={{ display: 'grid', gridTemplateColumns: '1.4fr 1fr', gap: 16 }}>
            {/* Stacked composition */}
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 22 }}>
              <div style={{ display: 'flex', alignItems: 'baseline', marginBottom: 16 }}>
                <span style={{ fontSize: 13, fontWeight: 700, color: ink }}>文档构成</span>
                <span style={{ fontSize: 11, color: mute, marginLeft: 8 }}>按字数</span>
              </div>
              <div style={{
                display: 'flex', height: 14, borderRadius: 7, overflow: 'hidden',
                border: `1px solid ${border}`,
              }}>
                <div style={{ width: '77%', background: C.okSoft }} />
                <div style={{ width: '15%', background: C.hi1 }} />
                <div style={{ width: '6%',  background: C.hi2 }} />
                <div style={{ width: '2%',  background: C.hi3 }} />
              </div>
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 14, marginTop: 16 }}>
                {[
                  { lbl: '原创', pct: '77%', c: C.ok, sub: '6,485 字' },
                  { lbl: '低相似', pct: '15%', c: C.hi1, sub: '1,263 字' },
                  { lbl: '中相似', pct: '6%',  c: C.hi2, sub: '505 字' },
                  { lbl: '高相似', pct: '2%',  c: C.hi3, sub: '167 字' },
                ].map((s, i) => (
                  <div key={i}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <span style={{ width: 6, height: 6, borderRadius: '50%', background: s.c }} />
                      <span style={{ fontSize: 11, color: mute, fontWeight: 600 }}>{s.lbl}</span>
                    </div>
                    <div style={{ fontSize: 20, fontWeight: 700, color: ink, marginTop: 5, letterSpacing: '-0.018em', fontFamily: C.font }}>{s.pct}</div>
                    <div style={{ fontSize: 10.5, color: mute, marginTop: 2 }}>{s.sub}</div>
                  </div>
                ))}
              </div>
            </div>

            {/* Sources */}
            <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 22 }}>
              <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 16 }}>主要相似来源</div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
                {[
                  { name: '2025Q4 业务复盘.docx', src: '我的文档 · 7 个月前', pct: 0.48, type: 'docx' },
                  { name: '部门 OKR 总结 v2.docx', src: '我的文档 · 12 月前', pct: 0.31, type: 'docx' },
                  { name: '行业研究笔记',         src: '我的文档 · 笔记',     pct: 0.18, type: 'md' },
                ].map((s, i) => (
                  <div key={i}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <CDocChip type={s.type} />
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontSize: 12, fontWeight: 600, color: ink, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.name}</div>
                        <div style={{ fontSize: 10.5, color: mute, marginTop: 1 }}>{s.src}</div>
                      </div>
                      <span style={{ fontSize: 13, fontWeight: 700, color: s.pct >= 0.4 ? C.hi3 : s.pct >= 0.25 ? C.hi2 : C.hi1, fontFamily: C.mono }}>
                        {Math.round(s.pct * 100)}%
                      </span>
                    </div>
                    <div style={{ height: 3, marginTop: 6, background: dark ? 'rgba(255,255,255,0.06)' : C.paper2, borderRadius: 2, overflow: 'hidden' }}>
                      <div style={{ height: '100%', width: `${s.pct * 100}%`, background: s.pct >= 0.4 ? C.hi3 : s.pct >= 0.25 ? C.hi2 : C.hi1 }} />
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Top spans */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 22 }}>
            <div style={{ display: 'flex', alignItems: 'baseline', marginBottom: 14 }}>
              <span style={{ fontSize: 13, fontWeight: 700, color: ink }}>建议优先处理</span>
              <span style={{ fontSize: 11, color: mute, marginLeft: 8 }}>按相似度排序</span>
              <div style={{ flex: 1 }} />
              <CButton kind="ghost" size="sm" dark={dark}>展开全部 12 处 →</CButton>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              {[
                { sec: '§2.1 业务概览', text: '本季度业务保持稳健增长,核心指标较上季度有所提升,主要得益于客户结构的持续优化与新产品线的快速上量。', pct: 68, src: '2025Q4 业务复盘 · §2' },
                { sec: '§3 客户结构', text: '核心客户复购率较上季度提升约 4 个百分点,前十大客户合计贡献了 62% 的营收,集中度仍处于行业中位。', pct: 42, src: '部门 OKR 总结 · §1' },
                { sec: '§4.2 风险', text: '在合规与数据安全方面,我们持续投入资源,通过年度第三方审计与季度内部巡检构建完整防线。', pct: 36, src: '行业研究笔记 · 安全' },
                { sec: '§6 展望', text: '下一阶段我们将聚焦于客户成功、产品深度与组织韧性三条主线,在不确定的环境中保持稳定增长。', pct: 28, src: '部门 OKR 总结 · §6' },
              ].map((s, i) => (
                <TopSpan key={i} s={s} dark={dark} ink={ink} mute={mute} />
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function TopSpan({ s, dark, ink, mute }) {
  const c = s.pct >= 65 ? C.hi3 : s.pct >= 40 ? C.hi2 : C.hi1;
  return (
    <div style={{
      display: 'grid', gridTemplateColumns: '120px 1fr 90px',
      gap: 14, padding: '12px 14px', borderRadius: 8,
      background: dark ? 'rgba(255,255,255,0.025)' : C.paper2, alignItems: 'center',
    }}>
      <div>
        <div style={{ fontSize: 11, fontWeight: 600, color: ink }}>{s.sec}</div>
        <div style={{ fontSize: 10.5, color: mute, marginTop: 2 }}>{s.src}</div>
      </div>
      <div style={{ fontSize: 12.5, color: ink, lineHeight: 1.55 }}>
        "<span style={{ borderBottom: `1.5px solid ${c}`, paddingBottom: 1 }}>{s.text}</span>"
      </div>
      <div style={{ textAlign: 'right' }}>
        <div style={{ fontSize: 20, fontWeight: 700, color: c, lineHeight: 1, fontFamily: C.font, letterSpacing: '-0.014em' }}>{s.pct}%</div>
        <div style={{ fontSize: 10.5, color: mute, marginTop: 4 }}>相似</div>
      </div>
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 4 · Compare — left/right with underline highlights + side notes
// ═══════════════════════════════════════════════════════════════
function CScrCompare({ accent = C.brand, dark = false, hi = 'amber' }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const paperBg = dark ? '#22222A' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;
  const HI = hiSchemeC(hi);

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="逐段对比 · 2026Q2 业务复盘报告 v3"
        sub="§2.1 业务概览 · 第 7 处 / 共 12 处"
        actions={<>
          <CButton kind="ghost" size="md" icon="ai" dark={dark}>AI 改写本段</CButton>
          <CButton kind="primary" size="md" accent={accent} icon="check">标记为已处理</CButton>
        </>}
      />
      {/* mini toolbar */}
      <div style={{
        height: 44, flexShrink: 0, padding: '0 24px',
        display: 'flex', alignItems: 'center', gap: 12,
        borderBottom: `1px solid ${border}`,
        background: dark ? 'rgba(255,255,255,0.02)' : C.paper2,
      }}>
        <CButton kind="secondary" size="sm" icon="chevL" dark={dark}>上一处</CButton>
        <span style={{ fontSize: 11.5, color: mute, fontFamily: C.mono }}>
          <span style={{ color: ink, fontWeight: 700 }}>7</span> / 12 处相似
        </span>
        <CButton kind="secondary" size="sm" iconRight="chevR" dark={dark}>下一处</CButton>
        <div style={{ width: 1, height: 18, background: border }} />
        <CPill bg={HI.hi3soft} fg={HI.hi3} size={11}>≥60% 高</CPill>
        <CPill bg={HI.hi2soft} fg={HI.hi2} size={11}>30-60% 中</CPill>
        <CPill bg={HI.hi1soft} fg={HI.hi1} size={11}>{'<'}30% 低</CPill>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11, color: mute }}>同步滚动</span>
        <CToggle on accent={accent} />
      </div>

      <div style={{ flex: 1, minHeight: 0, display: 'grid', gridTemplateColumns: '1fr 1fr', position: 'relative' }}>
        <CDocPane
          side="left" dark={dark} border={border} paperBg={paperBg} ink={ink} mute={mute} accent={accent}
          doc={{ type: 'docx', name: '2026Q2 业务复盘报告 v3', meta: '14 页 · 8,420 字 · 当前段 §2.1' }}
          HI={HI}
        />
        <CDocPane
          side="right" dark={dark} border={border} paperBg={paperBg} ink={ink} mute={mute} accent={accent}
          doc={{ type: 'docx', name: '2025Q4 业务复盘.docx', meta: '12 页 · 7,210 字 · 我的文档 · 7 个月前' }}
          HI={HI} right
        />
        <div style={{
          position: 'absolute', left: '50%', top: 0, bottom: 0, width: 1,
          background: border, transform: 'translateX(-0.5px)',
        }} />
      </div>
    </div>
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
  // amber
  return {
    hi1: C.hi1, hi2: C.hi2, hi3: C.hi3, hi4: C.hi4,
    hi1soft: C.hi1Soft, hi2soft: C.hi2Soft, hi3soft: C.hi3Soft, hi4soft: C.hi4Soft,
  };
}

function CDocPane({ side, doc, dark, border, paperBg, accent, ink, mute, right, HI }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', minWidth: 0, background: dark ? '#181820' : C.paper }}>
      <div style={{
        padding: '12px 20px', borderBottom: `1px solid ${border}`,
        display: 'flex', alignItems: 'center', gap: 10,
        background: dark ? '#1F1F26' : C.white,
      }}>
        <CDocChip type={doc.type} />
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
          {right ? <CBodyRight HI={HI} mute={mute} dark={dark} /> : <CBodyLeft HI={HI} mute={mute} dark={dark} accent={accent} />}
        </div>
      </div>
    </div>
  );
}

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

function CBodyLeft({ HI, mute, dark, accent }) {
  return <>
    <div style={{ fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§2 业务概览</div>
    <h2 style={{ fontSize: 19, fontWeight: 700, margin: '6px 0 14px', letterSpacing: '-0.014em', color: dark ? '#fff' : C.ink }}>
      2.1 整体业绩与趋势
    </h2>
    <p>
      <CHSpan HI={HI} level={3} refLabel="①">本季度业务保持稳健增长,核心指标较上季度有所提升,主要得益于客户结构的持续优化与新产品线的快速上量</CHSpan>
      。整体来看,我们在面对宏观环境波动的同时,基本完成了既定目标。
    </p>
    <p>
      在收入构成方面,
      <CHSpan HI={HI} level={2} refLabel="②">核心客户复购率较上季度提升约 4 个百分点,前十大客户合计贡献了 62% 的营收</CHSpan>
      ,集中度仍处于行业中位水平,但需关注大客户依赖带来的潜在风险。
    </p>
    <p>
      在新业务方面,Q2 上线的两条新产品线表现亮眼,合计贡献增量收入约 1,800 万元,
      <CHSpan HI={HI} level={1}>毛利率高于公司平均水平 3.2 个百分点</CHSpan>
      ,验证了我们对市场需求的判断方向正确。
    </p>

    {/* AI suggestion */}
    <div style={{
      marginTop: 18, padding: 14, borderRadius: 10,
      background: dark ? 'rgba(79,88,168,0.12)' : `${accent}10`,
      border: `1px dashed ${accent}55`,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginBottom: 8 }}>
        <CIcon name="ai" size={13} style={{ color: accent }} />
        <span style={{ fontSize: 11, fontWeight: 700, color: accent, letterSpacing: '0.04em', textTransform: 'uppercase' }}>
          改写建议 · ① 句
        </span>
        <span style={{ flex: 1 }} />
        <span style={{ fontSize: 10, color: mute, fontFamily: C.mono }}>68% → 14%</span>
      </div>
      <div style={{ fontSize: 12.5, color: dark ? '#fff' : C.ink, lineHeight: 1.7 }}>
        Q2 业务延续了 Q1 的上升势头,在<u style={{ textDecorationColor: accent, textDecorationStyle: 'wavy' }}>客户结构调优</u>与新品线放量的双轮驱动下,
        关键指标均录得环比改善。
      </div>
      <div style={{ display: 'flex', gap: 6, marginTop: 10 }}>
        <CButton kind="secondary" size="sm" dark={dark}>再生成</CButton>
        <CButton kind="ghost" size="sm" dark={dark}>查看差异</CButton>
        <div style={{ flex: 1 }} />
        <CButton kind="primary" size="sm" accent={accent} icon="check">采用</CButton>
      </div>
    </div>
  </>;
}

function CBodyRight({ HI, mute, dark }) {
  return <>
    <div style={{ fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§2 业务回顾</div>
    <h2 style={{ fontSize: 19, fontWeight: 700, margin: '6px 0 14px', letterSpacing: '-0.014em', color: dark ? '#fff' : C.ink }}>
      2.1 季度业绩
    </h2>
    <p>
      <CHSpan HI={HI} level={3} refLabel="①">本季度业务保持稳健增长,核心指标较上季度有所提升,主要得益于客户结构的持续优化与新产品线的逐步放量</CHSpan>
      。即便外部环境存在一些扰动,我们仍按计划完成了主要目标。
    </p>
    <p>
      从客户结构来看,
      <CHSpan HI={HI} level={2} refLabel="②">核心客户复购率较上季度提升约 3 个百分点,前十大客户合计贡献了 58% 的营收</CHSpan>
      。我们将持续优化客户矩阵,降低对头部客户的过度依赖。
    </p>
    <p style={{ color: mute, fontSize: 12 }}>
      （后续内容讨论组织结构调整,与本季度复盘内容无直接重叠。）
    </p>
  </>;
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 5 · My Docs — calm grid/list of history
// ═══════════════════════════════════════════════════════════════
function CScrDocs({ accent = C.brand, dark = false }) {
  const ink = dark ? '#fff' : C.ink;
  const mute = dark ? 'rgba(255,255,255,0.55)' : C.ink3;
  const bg = dark ? '#15151B' : C.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : C.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : C.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, minWidth: 0 }}>
      <CTopbar dark={dark} accent={accent}
        title="我的文档"
        sub="共 48 份 · 本月新增 12 份"
        actions={<>
          <CButton kind="ghost" size="md" icon="filter" dark={dark}>筛选</CButton>
          <CButton kind="ghost" size="md" icon="sort" dark={dark}>排序</CButton>
          <CButton kind="primary" size="md" accent={accent} icon="plus">上传文档</CButton>
        </>}
      />
      <div style={{ flex: 1, overflow: 'auto', padding: '24px 48px 40px' }}>
        {/* Filter strip */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 18 }}>
          <CSegControl options={['全部', '报告', '简历', 'PPT', '笔记', '其他']} value={0} accent={accent} dark={dark} />
          <div style={{ flex: 1 }} />
          <CPill bg={dark ? 'rgba(255,255,255,0.04)' : C.white} fg={mute} style={{ border: `1px solid ${border}`, padding: '4px 10px' }}>
            最近 90 天
          </CPill>
        </div>

        {/* Grouped list */}
        <DocSection title="本周" docs={DOCS_THIS_WEEK} cardBg={cardBg} border={border} ink={ink} mute={mute} dark={dark} />
        <DocSection title="五月" docs={DOCS_MAY} cardBg={cardBg} border={border} ink={ink} mute={mute} dark={dark} />
        <DocSection title="四月" docs={DOCS_APR} cardBg={cardBg} border={border} ink={ink} mute={mute} dark={dark} />
      </div>
    </div>
  );
}

const DOCS_THIS_WEEK = [
  { type: 'docx', name: '2026Q2 业务复盘报告 v3',     pct: 22.4, date: '昨日 17:42', pages: 14, words: '8,420 字', sev: 'mid' },
  { type: 'pptx', name: '产品方案-客户演示 v3',        pct: 41.0, date: '5 月 24 日', pages: 38, words: '—',         sev: 'mid' },
  { type: 'docx', name: '个人简历 林一帆 v8',          pct: 67.8, date: '5 月 22 日', pages: 2,  words: '1,240 字', sev: 'high' },
];
const DOCS_MAY = [
  { type: 'pdf',  name: '行业研究笔记 · 数据分析',     pct: 13.5, date: '5 月 19 日', pages: 9,  words: '4,310 字', sev: 'low' },
  { type: 'docx', name: '部门月度汇报 · 5月',          pct: 35.2, date: '5 月 18 日', pages: 6,  words: '3,180 字', sev: 'mid' },
  { type: 'docx', name: '产品需求文档 · v0.4',         pct: 8.1,  date: '5 月 14 日', pages: 21, words: '11,200 字', sev: 'low' },
  { type: 'pptx', name: '提案-合作伙伴介绍 v2',        pct: 28.6, date: '5 月 10 日', pages: 24, words: '—',         sev: 'mid' },
  { type: 'docx', name: '会议纪要 · 战略闭门会',       pct: 4.7,  date: '5 月  6 日', pages: 4,  words: '1,820 字', sev: 'low' },
];
const DOCS_APR = [
  { type: 'docx', name: '2026Q1 业务复盘报告 v2',     pct: 18.9, date: '4 月 28 日', pages: 12, words: '7,540 字', sev: 'low' },
  { type: 'pdf',  name: '行业大会 · 主题演讲讲稿',     pct: 52.1, date: '4 月 22 日', pages: 7,  words: '3,920 字', sev: 'high' },
  { type: 'pptx', name: '产品发布 · 内部宣讲 v4',      pct: 31.5, date: '4 月 15 日', pages: 32, words: '—',         sev: 'mid' },
  { type: 'md',   name: '读书笔记 · 复杂系统设计',     pct: 6.2,  date: '4 月 11 日', pages: 1,  words: '2,180 字', sev: 'low' },
];

function DocSection({ title, docs, cardBg, border, ink, mute, dark }) {
  return (
    <div style={{ marginBottom: 28 }}>
      <div style={{
        fontSize: 10.5, fontWeight: 600, color: mute, letterSpacing: '0.08em', textTransform: 'uppercase',
        marginBottom: 10,
      }}>{title}</div>
      <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, overflow: 'hidden' }}>
        {docs.map((d, i) => (
          <DocListRow key={i} d={d} ink={ink} mute={mute} border={border} dark={dark} last={i === docs.length - 1} />
        ))}
      </div>
    </div>
  );
}

function DocListRow({ d, ink, mute, border, dark, last }) {
  const sevColor = d.sev === 'high' ? C.hi3 : d.sev === 'mid' ? C.hi2 : C.ok;
  const sevLabel = d.sev === 'high' ? '高相似' : d.sev === 'mid' ? '中相似' : '低相似';
  return (
    <div style={{
      display: 'grid', gridTemplateColumns: '60px 1fr 160px 110px 90px 24px',
      alignItems: 'center', gap: 16, padding: '14px 18px',
      borderBottom: last ? 'none' : `1px solid ${border}`,
    }}>
      <CDocChip type={d.type} />
      <div>
        <div style={{ fontSize: 13, fontWeight: 600, color: ink, letterSpacing: '-0.005em' }}>{d.name}</div>
        <div style={{ fontSize: 11, color: mute, marginTop: 3 }}>{d.pages} 页 · {d.words}</div>
      </div>
      <div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ fontSize: 13.5, fontWeight: 700, color: sevColor, fontFamily: C.font, letterSpacing: '-0.014em' }}>
            {Math.floor(d.pct)}<span style={{ fontSize: 10.5, color: mute, fontWeight: 500 }}>.{Math.round((d.pct % 1) * 10)}%</span>
          </span>
        </div>
        <div style={{ height: 3, background: dark ? 'rgba(255,255,255,0.05)' : C.paper2, borderRadius: 2, marginTop: 5, overflow: 'hidden' }}>
          <div style={{ height: '100%', width: `${d.pct}%`, background: sevColor }} />
        </div>
      </div>
      <CPill bg={dark ? 'rgba(255,255,255,0.05)' : C.paper2} fg={sevColor} size={11}>
        <span style={{ width: 5, height: 5, borderRadius: '50%', background: sevColor }} />
        {sevLabel}
      </CPill>
      <span style={{ fontSize: 11, color: mute, textAlign: 'right' }}>{d.date}</span>
      <CIcon name="chevR" size={12} style={{ color: mute }} />
    </div>
  );
}


// ═══════════════════════════════════════════════════════════════
// SCREEN 6 · Settings — quiet preferences
// ═══════════════════════════════════════════════════════════════
function CScrSettings({ accent = C.brand, dark = false, onTweak }) {
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

          {/* Account */}
          <SettingsCard title="账户" dark={dark} cardBg={cardBg} border={border} ink={ink} mute={mute}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
              <CAvatar name="林" color="#7B8FE5" size={48} />
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 14.5, fontWeight: 700, color: ink, letterSpacing: '-0.008em' }}>林一帆</div>
                <div style={{ fontSize: 11.5, color: mute, marginTop: 3 }}>lin.yifan@example.com · 加入于 2025 年 8 月</div>
              </div>
              <CButton kind="secondary" size="md" dark={dark}>编辑资料</CButton>
            </div>
          </SettingsCard>

          {/* Plan */}
          <SettingsCard title="订阅" dark={dark} cardBg={cardBg} border={border} ink={ink} mute={mute}>
            <SettingsRow label="当前方案" sub="个人版 · 每月 10 次检测" dark={dark} ink={ink} mute={mute}>
              <CPill bg={C.brandSoft} fg={accent} size={11}>个人版</CPill>
            </SettingsRow>
            <SettingsRow label="本月用量" sub="还剩 8 次,5 月 31 日重置" dark={dark} ink={ink} mute={mute}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
                <div style={{ width: 80, height: 4, background: dark ? 'rgba(255,255,255,0.08)' : C.paper2, borderRadius: 2, overflow: 'hidden' }}>
                  <div style={{ height: '100%', width: '20%', background: accent }} />
                </div>
                <span style={{ fontSize: 11.5, color: mute, fontFamily: C.mono }}>2 / 10</span>
              </div>
            </SettingsRow>
            <SettingsRow label="升级至专业版" sub="¥ 38 / 月 · 不限次数 · 团队共享" dark={dark} ink={ink} mute={mute} last>
              <CButton kind="primary" size="sm" accent={accent}>了解一下</CButton>
            </SettingsRow>
          </SettingsCard>

          {/* Detection */}
          <SettingsCard title="检测偏好" dark={dark} cardBg={cardBg} border={border} ink={ink} mute={mute}>
            <SettingsRow label="最低相似度阈值" sub="低于此值不会被标记" dark={dark} ink={ink} mute={mute}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <div style={{ width: 120, height: 3, background: dark ? 'rgba(255,255,255,0.08)' : C.paper3, borderRadius: 2, position: 'relative' }}>
                  <div style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: '20%', background: accent, borderRadius: 2 }} />
                  <div style={{
                    position: 'absolute', left: '20%', top: '50%', transform: 'translate(-50%,-50%)',
                    width: 12, height: 12, borderRadius: '50%', background: '#fff',
                    boxShadow: `0 0 0 1.5px ${accent}, 0 1px 4px rgba(0,0,0,0.15)`,
                  }} />
                </div>
                <span style={{ fontSize: 12, color: ink, fontFamily: C.mono, fontWeight: 600, minWidth: 28 }}>20%</span>
              </div>
            </SettingsRow>
            <SettingsRow label="忽略引用与脚注" sub="带引号的内容不计入相似" dark={dark} ink={ink} mute={mute}>
              <CToggle on accent={accent} />
            </SettingsRow>
            <SettingsRow label="开启 AI 改写建议" sub="完成检测后自动生成" dark={dark} ink={ink} mute={mute}>
              <CToggle on accent={accent} />
            </SettingsRow>
            <SettingsRow label="检测前自动备份原稿" sub="存到「我的文档/原本备份」" dark={dark} ink={ink} mute={mute} last>
              <CToggle on accent={accent} />
            </SettingsRow>
          </SettingsCard>

          {/* Privacy */}
          <SettingsCard title="隐私" dark={dark} cardBg={cardBg} border={border} ink={ink} mute={mute}>
            <SettingsRow label="本地优先模式" sub="文档不会上传到服务器" dark={dark} ink={ink} mute={mute}>
              <CToggle on accent={accent} />
            </SettingsRow>
            <SettingsRow label="加入匿名使用改进计划" sub="不包含文档内容" dark={dark} ink={ink} mute={mute} last>
              <CToggle on={false} accent={accent} />
            </SettingsRow>
          </SettingsCard>

          {/* Appearance */}
          <SettingsCard title="外观" dark={dark} cardBg={cardBg} border={border} ink={ink} mute={mute}>
            <SettingsRow label="主题色" sub="影响按钮、图表与高亮" dark={dark} ink={ink} mute={mute}>
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
          </SettingsCard>

          {/* About */}
          <div style={{ fontSize: 11, color: mute, textAlign: 'center', padding: '8px 0 20px' }}>
            <CLogo size={20} color={accent} style={{ verticalAlign: 'middle', marginRight: 6 }} />
            <span style={{ verticalAlign: 'middle' }}>原本 · Verum &nbsp;·&nbsp; 个人版 v1.4.2 &nbsp;·&nbsp; <a style={{ color: mute }}>关于</a> · <a style={{ color: mute }}>反馈</a> · <a style={{ color: mute }}>退出</a></span>
          </div>
        </div>
      </div>
    </div>
  );
}

function SettingsCard({ title, children, dark, cardBg, border, ink, mute }) {
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

function SettingsRow({ label, sub, children, dark, ink, mute, last }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 16, padding: '14px 0',
      borderBottom: last ? 'none' : `1px solid ${dark ? 'rgba(255,255,255,0.06)' : C.line}`,
    }}>
      <div style={{ flex: 1 }}>
        <div style={{ fontSize: 13, fontWeight: 600, color: ink, letterSpacing: '-0.005em' }}>{label}</div>
        {sub && <div style={{ fontSize: 11.5, color: mute, marginTop: 3 }}>{sub}</div>}
      </div>
      {children}
    </div>
  );
}


Object.assign(window, {
  CScrHome, CScrScan, CScrReport, CScrCompare, CScrDocs, CScrSettings,
});
