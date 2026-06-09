// screens-b.jsx — Detail Compare, Clusters, Matrix, Export

// ═════════════════════════════════════════════════════════════
// SCREEN 5 · Detail compare (左右分屏 · 下划线高亮 · 边注)
// ═════════════════════════════════════════════════════════════
function ScrDetail({ accent = T.brand, dark = false, hi = 'amber' }) {
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;
  const bg = dark ? '#1A1A1F' : T.paper;
  const paperBg = dark ? '#22222A' : T.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : T.line;

  // Highlight color scheme
  const HI = hiScheme(hi);

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, overflow: 'hidden' }}>
      <VTopbar dark={dark}
        crumbs={['工作区', '我的任务', 'Verity 产品白皮书 · 初稿', '详细对比']}
        actions={<>
          <Button kind="ghost" icon="ai" dark={dark}>AI 改写所有高亮</Button>
          <Button kind="secondary" icon="quote" dark={dark}>生成引用</Button>
          <Button kind="primary" accent={accent} icon="check">标记已处理</Button>
        </>}
      />
      {/* Inner toolbar */}
      <div style={{
        height: 40, flexShrink: 0, padding: '0 16px',
        display: 'flex', alignItems: 'center', gap: 10,
        borderBottom: `1px solid ${border}`,
        background: dark ? 'rgba(255,255,255,0.02)' : T.paper2,
      }}>
        <SegControl options={['左右对比', '聚合', '热力图']} value={0} accent={accent} dark={dark} />
        <div style={{ width: 1, height: 18, background: border }} />
        <Pill bg={dark ? 'rgba(255,255,255,0.04)' : T.white} fg={textInk} style={{ border: `1px solid ${border}`, padding: '3px 8px' }} weight={600}>
          <VIcon name="chevL" size={11} />
          上一处
        </Pill>
        <span style={{ fontSize: 11.5, color: textMute, fontFamily: T.mono }}>
          <b style={{ color: textInk }}>7</b> / 24 处相似
        </span>
        <Pill bg={dark ? 'rgba(255,255,255,0.04)' : T.white} fg={textInk} style={{ border: `1px solid ${border}`, padding: '3px 8px' }} weight={600}>
          下一处
          <VIcon name="chevR" size={11} />
        </Pill>
        <div style={{ width: 1, height: 18, background: border }} />
        <Pill bg={HI.hi3soft} fg={HI.hi3} size={11}>≥70% 高</Pill>
        <Pill bg={HI.hi2soft} fg={HI.hi2} size={11}>40-70% 中</Pill>
        <Pill bg={HI.hi1soft} fg={HI.hi1} size={11}>{'<'}40% 低</Pill>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11, color: textMute }}>同步滚动</span>
        <Toggle on={true} accent={accent} />
        <span style={{ fontSize: 11, color: textMute, marginLeft: 8 }}>显示差异</span>
        <Toggle on={true} accent={accent} />
      </div>

      {/* Two panes */}
      <div style={{ flex: 1, minHeight: 0, display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 0, position: 'relative' }}>
        <DocPane
          side="left"
          dark={dark} border={border} paperBg={paperBg} accent={accent}
          textInk={textInk} textMute={textMute}
          doc={{ type: 'docx', name: 'Verity 产品白皮书 · 初稿', meta: '22 页 · 13,840 字 · 当前章节 §3.2' }}
          HI={HI}
        />
        <DocPane
          side="right"
          dark={dark} border={border} paperBg={paperBg} accent={accent}
          textInk={textInk} textMute={textMute}
          doc={{ type: 'docx', name: 'Verity 产品介绍 · 2025 v3', meta: '14 页 · 8,210 字 · 内部 / 市场部' }}
          HI={HI}
          right
        />
        {/* Center separator with span connectors */}
        <div style={{
          position: 'absolute', left: '50%', top: 0, bottom: 0, width: 1,
          background: border, transform: 'translateX(-0.5px)',
        }} />
      </div>
    </div>
  );
}

function hiScheme(name) {
  if (name === 'rose') return {
    hi1: T.hi1, hi2: '#E37487', hi3: '#D14C6B', hi4: '#A8284C',
    hi1soft: '#FBE7D0', hi2soft: '#FAD8E0', hi3soft: '#F8C5D2', hi4soft: '#F1A8BB',
  };
  if (name === 'green') return {
    hi1: '#9CCB78', hi2: '#5FAA5F', hi3: '#3F8244', hi4: '#1F5F2C',
    hi1soft: '#E3F1D6', hi2soft: '#D2EDD2', hi3soft: '#C5E3C5', hi4soft: '#9FCFA1',
  };
  // default amber
  return {
    hi1: T.hi1, hi2: T.hi2, hi3: T.hi3, hi4: T.hi4,
    hi1soft: T.hi1Soft, hi2soft: T.hi2Soft, hi3soft: T.hi3Soft, hi4soft: T.hi4Soft,
  };
}

function DocPane({ side, doc, dark, border, paperBg, accent, textInk, textMute, right, HI }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', minWidth: 0, background: dark ? '#181820' : T.paper }}>
      {/* doc header */}
      <div style={{
        padding: '12px 18px', borderBottom: `1px solid ${border}`,
        display: 'flex', alignItems: 'center', gap: 10,
        background: dark ? '#1F1F26' : T.white,
      }}>
        <DocChip type={doc.type} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 12.5, fontWeight: 600, color: textInk, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {doc.name}
          </div>
          <div style={{ fontSize: 10.5, color: textMute, marginTop: 1 }}>{doc.meta}</div>
        </div>
        <VIcon name="search" size={13} style={{ color: textMute }} />
        <VIcon name="cog" size={13} style={{ color: textMute }} />
      </div>
      {/* paper */}
      <div style={{ flex: 1, overflow: 'auto', padding: '20px 24px', display: 'flex', justifyContent: 'center' }}>
        <div style={{
          maxWidth: 540, width: '100%',
          background: paperBg, borderRadius: 8,
          border: `1px solid ${border}`,
          boxShadow: dark ? 'none' : T.shadow.sm,
          padding: '36px 40px 32px',
          fontSize: 13, lineHeight: 1.85, color: textInk,
          fontFamily: T.font, letterSpacing: '-0.005em',
        }}>
          {right ? <DocBodyRight HI={HI} textMute={textMute} dark={dark} /> : <DocBodyLeft HI={HI} textMute={textMute} dark={dark} accent={accent} />}
        </div>
      </div>
    </div>
  );
}

// Render a highlighted span (underline + side reference)
function HSpan({ text, level = 2, ref: refLabel, HI, dark }) {
  const color = HI['hi' + level];
  const soft = HI['hi' + level + 'soft'];
  return (
    <span style={{
      background: `linear-gradient(to top, ${soft} 35%, transparent 35%)`,
      borderBottom: `2px solid ${color}`,
      padding: '0 1px',
      position: 'relative',
      cursor: 'pointer',
    }}>
      {text}
      {refLabel && (
        <sup style={{
          marginLeft: 2, padding: '0 4px', borderRadius: 3,
          background: color, color: '#fff', fontSize: 9.5, fontWeight: 700,
          verticalAlign: 'super', fontFamily: T.mono,
        }}>{refLabel}</sup>
      )}
    </span>
  );
}

function DocBodyLeft({ HI, textMute, dark, accent }) {
  return <>
    <div style={{ fontSize: 10.5, fontWeight: 600, color: textMute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§3 安全架构</div>
    <h2 style={{ fontSize: 19, fontWeight: 700, margin: '6px 0 14px', letterSpacing: '-0.014em', color: dark ? '#fff' : T.ink }}>
      3.2 数据保护与加密体系
    </h2>
    <p>
      Verity 平台围绕「最小权限」「全链路审计」与「端到端加密」三项核心原则构建了完整的数据保护体系。
      <HSpan HI={HI} level={4} refLabel="①">所有数据在传输与静止状态下均通过 AES-256 算法加密，密钥由企业自管 KMS 派生并按月轮换</HSpan>
      ，保证即使在底层基础设施被攻破的情形下，数据本身仍不可读。
    </p>
    <p>
      访问控制采用基于角色的权限模型（RBAC），并结合属性与策略动态裁决：
      <HSpan HI={HI} level={2} refLabel="②">系统会在用户每一次发起敏感操作前进行实时权限校验，并将完整的上下文（请求体、设备指纹、网络位置）写入审计日志</HSpan>
      ，所有日志均通过 WORM 介质归档，留存周期不少于 5 年。
    </p>
    <div style={{ marginTop: 14, fontSize: 10.5, fontWeight: 600, color: textMute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§3.3 网络与基础设施</div>
    <p style={{ marginTop: 6 }}>
      平台部署在多可用区的隔离 VPC 内，控制面与数据面物理分离。
      <HSpan HI={HI} level={3} refLabel="③">每一次跨可用区的服务调用都必须通过双向 TLS 验证，并由独立的服务网格进行流量审计与速率控制</HSpan>
      ，从而将横向移动的风险降到最低。
    </p>
    <p>
      我们对所有第三方依赖进行 SBOM 跟踪与持续漏洞扫描，
      <HSpan HI={HI} level={1}>关键依赖的高危漏洞响应 SLA 为 24 小时之内</HSpan>
      ，常规漏洞响应 SLA 不超过 7 个自然日。
    </p>

    {/* AI suggestion popover */}
    <div style={{
      marginTop: 16, padding: 14, borderRadius: 10,
      background: dark ? 'rgba(91,91,214,0.10)' : `${accent}10`,
      border: `1px dashed ${accent}55`,
      position: 'relative',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginBottom: 7 }}>
        <VIcon name="ai" size={13} style={{ color: accent }} />
        <span style={{ fontSize: 11, fontWeight: 700, color: accent, letterSpacing: '0.04em', textTransform: 'uppercase' }}>
          AI 改写建议 · ② 段
        </span>
        <span style={{ flex: 1 }} />
        <span style={{ fontSize: 10, color: textMute, fontFamily: T.mono }}>92% → 27%</span>
      </div>
      <div style={{ fontSize: 12, color: dark ? '#fff' : T.ink, lineHeight: 1.7 }}>
        访问控制基于 RBAC 与策略引擎组合，<u style={{ textDecorationColor: accent, textDecorationStyle: 'wavy' }}>对涉密操作进行实时鉴权</u>，并完整记录请求上下文与设备元信息……
      </div>
      <div style={{ display: 'flex', gap: 6, marginTop: 10 }}>
        <Button kind="secondary" size="sm" dark={dark}>再生成</Button>
        <Button kind="ghost" size="sm" dark={dark}>对比差异</Button>
        <div style={{ flex: 1 }} />
        <Button kind="primary" size="sm" accent={accent} icon="check">采纳</Button>
      </div>
    </div>
  </>;
}

function DocBodyRight({ HI, textMute, dark }) {
  return <>
    <div style={{ fontSize: 10.5, fontWeight: 600, color: textMute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§2 平台能力</div>
    <h2 style={{ fontSize: 19, fontWeight: 700, margin: '6px 0 14px', letterSpacing: '-0.014em', color: dark ? '#fff' : T.ink }}>
      2.1 安全设计原则
    </h2>
    <p>
      Verity 在产品设计之初便确立了「最小权限」「端到端加密」与「全链路审计」三项基本原则。
      <HSpan HI={HI} level={4} refLabel="①">所有数据在传输与静止状态下均通过 AES-256 算法加密，密钥由企业自管 KMS 派生并按月进行密钥轮换</HSpan>
      。这种设计保证了即便底层基础设施失守，数据也始终不可读。
    </p>
    <p>
      在访问控制层面，平台采用基于角色的权限模型并融合 ABAC 策略：
      <HSpan HI={HI} level={2} refLabel="②">在用户发起任何一次敏感操作之前，系统都会进行实时权限校验，并将完整的请求上下文（请求体、设备指纹、网络位置）记入审计日志</HSpan>
      ，所有日志均使用 WORM 介质归档存储不少于五年。
    </p>
    <div style={{ marginTop: 14, fontSize: 10.5, fontWeight: 600, color: textMute, letterSpacing: '0.06em', textTransform: 'uppercase' }}>§2.2 网络隔离</div>
    <p style={{ marginTop: 6 }}>
      平台部署于多可用区相互隔离的 VPC，数据面与控制面在物理层即被分离。
      <HSpan HI={HI} level={3} refLabel="③">跨可用区的所有服务调用必须经过双向 TLS 鉴权，并由独立服务网格执行流量审计与速率控制</HSpan>
      。
    </p>
    <p style={{ color: textMute, fontSize: 12 }}>
      （后续内容讲述 SOC2 与 ISO 27001 的实施细节，与左侧文档无重叠。）
    </p>
  </>;
}

function Toggle({ on, accent }) {
  return (
    <div style={{
      width: 26, height: 14, borderRadius: 999,
      background: on ? accent : '#C9C5CF',
      position: 'relative', cursor: 'pointer',
      transition: 'background 0.15s',
    }}>
      <div style={{
        position: 'absolute', top: 1.5, left: on ? 13.5 : 1.5,
        width: 11, height: 11, borderRadius: '50%',
        background: '#fff', boxShadow: '0 1px 2px rgba(0,0,0,0.2)',
        transition: 'left 0.15s',
      }} />
    </div>
  );
}


// ═════════════════════════════════════════════════════════════
// SCREEN 6 · Clusters (重复片段聚合视图)
// ═════════════════════════════════════════════════════════════
function ScrClusters({ accent = T.brand, dark = false }) {
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;
  const bg = dark ? '#1A1A1F' : T.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : T.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : T.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, overflow: 'hidden' }}>
      <VTopbar dark={dark}
        crumbs={['工作区', '我的任务', '2026Q1 供应商保密协议', '片段聚合']}
        actions={<>
          <Button kind="ghost" icon="filter" dark={dark}>筛选</Button>
          <Button kind="secondary" icon="branch" dark={dark}>批量处理</Button>
          <Button kind="primary" accent={accent} icon="ai">AI 一键改写</Button>
        </>}
      />
      <div style={{ flex: 1, minHeight: 0, padding: 20, display: 'grid', gridTemplateColumns: '280px 1fr 320px', gap: 16, overflow: 'hidden' }}>
        {/* LEFT: cluster list */}
        <div style={{
          background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
          display: 'flex', flexDirection: 'column', overflow: 'hidden',
        }}>
          <div style={{ padding: '12px 14px', borderBottom: `1px solid ${border}`, display: 'flex', alignItems: 'center' }}>
            <span style={{ fontSize: 12.5, fontWeight: 700, color: textInk }}>重复片段聚合</span>
            <div style={{ flex: 1 }} />
            <Pill bg={dark ? 'rgba(255,255,255,0.06)' : T.paper2} fg={textMute} size={10}>共 38 组</Pill>
          </div>
          <div style={{ flex: 1, overflow: 'auto' }}>
            {CLUSTERS.map((c, i) => (
              <ClusterRow key={i} c={c} dark={dark} border={border} textInk={textInk} textMute={textMute} accent={accent} active={i === 1} />
            ))}
          </div>
        </div>

        {/* CENTER: detail of selected cluster */}
        <ClusterDetail dark={dark} cardBg={cardBg} border={border} textInk={textInk} textMute={textMute} accent={accent} />

        {/* RIGHT: comments + activity */}
        <ClusterSidebar dark={dark} cardBg={cardBg} border={border} textInk={textInk} textMute={textMute} accent={accent} />
      </div>
    </div>
  );
}

const CLUSTERS = [
  { title: '保密义务条款 · 模板段落', occurs: 5, pct: 94, tag: '模板',  status: 'review' },
  { title: '披露范围与例外情形',      occurs: 4, pct: 82, tag: '关键',  status: 'review' },
  { title: '违约责任与赔偿计算',      occurs: 4, pct: 71, tag: '关键',  status: 'pending' },
  { title: '争议解决与适用法律',      occurs: 3, pct: 68, tag: '常规',  status: 'pending' },
  { title: '数据安全合规条款',        occurs: 3, pct: 58, tag: '常规',  status: 'done' },
  { title: '附件与签署页样式',        occurs: 6, pct: 42, tag: '次要',  status: 'done' },
  { title: '人员调动与交接说明',      occurs: 2, pct: 38, tag: '次要',  status: 'pending' },
  { title: '协议有效期与续约条款',    occurs: 3, pct: 35, tag: '次要',  status: 'pending' },
];

function ClusterRow({ c, dark, border, textInk, textMute, accent, active }) {
  const sevColor = c.pct >= 80 ? T.hi4 : c.pct >= 60 ? T.hi3 : c.pct >= 40 ? T.hi2 : T.hi1;
  return (
    <div style={{
      padding: '11px 14px', borderBottom: `1px solid ${border}`,
      background: active ? (dark ? 'rgba(91,91,214,0.15)' : `${accent}10`) : 'transparent',
      position: 'relative', cursor: 'pointer',
    }}>
      {active && <div style={{ position: 'absolute', left: 0, top: 8, bottom: 8, width: 2.5, background: accent, borderRadius: 2 }} />}
      <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
        <span style={{ width: 6, height: 6, borderRadius: '50%', background: sevColor }} />
        <span style={{ fontSize: 12, fontWeight: active ? 700 : 600, color: textInk, flex: 1, lineHeight: 1.3 }}>{c.title}</span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginTop: 6, paddingLeft: 13 }}>
        <Pill bg={dark ? 'rgba(255,255,255,0.06)' : T.paper2} fg={textMute} size={10}>{c.tag}</Pill>
        <span style={{ fontSize: 10.5, color: textMute }}>{c.occurs} 处</span>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 11.5, fontWeight: 700, color: sevColor, fontFamily: T.mono }}>{c.pct}%</span>
      </div>
    </div>
  );
}

function ClusterDetail({ dark, cardBg, border, textInk, textMute, accent }) {
  return (
    <div style={{
      background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
      display: 'flex', flexDirection: 'column', overflow: 'hidden',
    }}>
      <div style={{ padding: '14px 18px', borderBottom: `1px solid ${border}` }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <Pill bg={T.hi3Soft} fg={T.hi3} size={11}>关键</Pill>
          <span style={{ fontSize: 11.5, color: textMute }}>聚合 #02 · 出现于 4 份文档</span>
          <div style={{ flex: 1 }} />
          <span style={{ fontSize: 11, color: textMute }}>平均相似度</span>
          <span style={{ fontSize: 14, fontWeight: 700, color: T.hi3, fontFamily: T.mono }}>82%</span>
        </div>
        <div style={{ fontSize: 17, fontWeight: 700, color: textInk, marginTop: 6, letterSpacing: '-0.012em' }}>
          披露范围与例外情形
        </div>
      </div>

      <div style={{ flex: 1, overflow: 'auto', padding: 18 }}>
        {/* canonical text */}
        <div style={{
          padding: '14px 16px', borderRadius: 10,
          background: dark ? 'rgba(255,255,255,0.025)' : T.paper2,
          border: `1px solid ${border}`, marginBottom: 14,
        }}>
          <div style={{
            fontSize: 10, fontWeight: 700, color: textMute,
            letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 6,
          }}>
            归一化文本（共 4 份文档共享）
          </div>
          <div style={{ fontSize: 13, lineHeight: 1.8, color: textInk }}>
            "接收方所获悉的<u style={{ textDecorationColor: T.hi3, textDecorationStyle: 'solid', textDecorationThickness: 2, textUnderlineOffset: 3 }}>披露方任何形式的保密信息</u>，包括但不限于商业计划、技术资料、客户名单及商务条件等，
            <u style={{ textDecorationColor: T.hi3, textDecorationStyle: 'solid', textDecorationThickness: 2, textUnderlineOffset: 3 }}>不得以任何方式向第三方披露或用于本协议约定目的之外的用途</u>。"
          </div>
        </div>

        {/* occurrences */}
        <div style={{ fontSize: 11, fontWeight: 700, color: textMute, letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 8 }}>
          在各文档中的实例
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {[
            { name: '保密协议-华东v3.docx', sec: '§3.1', pct: 92, var: '与归一化文本仅有 2 处语序差异', diff: [['add', '所有'], ['eq', '商业计划、'], ['del', '产品路线、'], ['eq', '客户名单']] },
            { name: '保密协议-西南v3.docx', sec: '§3.1', pct: 88, var: '增加"任何关联实体"的限定词',   diff: [['eq', '披露方'], ['add', '及其关联实体'], ['eq', '保密信息']] },
            { name: '保密协议-华北v3.docx', sec: '§3.2', pct: 81, var: '"披露方"改为"提供方"',         diff: [['del', '披露方'], ['add', '提供方'], ['eq', '所获悉的保密信息']] },
            { name: '保密协议-海外v1.docx', sec: '§4',   pct: 67, var: '加入双语英文条款',             diff: [['eq', '保密信息 / '], ['add', 'Confidential Information']] },
          ].map((o, i) => (
            <OccurrenceCard key={i} o={o} dark={dark} border={border} textInk={textInk} textMute={textMute} />
          ))}
        </div>
      </div>

      {/* Bottom action bar */}
      <div style={{
        padding: '12px 18px', borderTop: `1px solid ${border}`,
        display: 'flex', alignItems: 'center', gap: 9,
        background: dark ? 'rgba(255,255,255,0.02)' : T.paper2,
      }}>
        <Pill bg={dark ? 'rgba(255,255,255,0.05)' : T.white} fg={textInk} style={{ border: `1px solid ${border}`, padding: '4px 9px' }} weight={600}>
          <VIcon name="check" size={11} />标记为可接受
        </Pill>
        <Pill bg={dark ? 'rgba(255,255,255,0.05)' : T.white} fg={textInk} style={{ border: `1px solid ${border}`, padding: '4px 9px' }} weight={600}>
          <VIcon name="archive" size={11} />归入模板库
        </Pill>
        <Pill bg={dark ? 'rgba(255,255,255,0.05)' : T.white} fg={textInk} style={{ border: `1px solid ${border}`, padding: '4px 9px' }} weight={600}>
          <VIcon name="flag" size={11} />升级为政策
        </Pill>
        <div style={{ flex: 1 }} />
        <Button kind="primary" accent={accent} icon="ai">AI 改写所有 4 处</Button>
      </div>
    </div>
  );
}

function OccurrenceCard({ o, dark, border, textInk, textMute }) {
  const sevColor = o.pct >= 80 ? T.hi3 : o.pct >= 60 ? T.hi2 : T.hi1;
  return (
    <div style={{
      padding: '12px 14px', borderRadius: 10,
      background: dark ? 'rgba(255,255,255,0.025)' : '#fff',
      border: `1px solid ${border}`,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <DocChip type="docx" />
        <span style={{ fontSize: 12, fontWeight: 600, color: textInk }}>{o.name}</span>
        <span style={{ fontSize: 11, color: textMute, fontFamily: T.mono }}>{o.sec}</span>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 13, fontWeight: 700, color: sevColor, fontFamily: T.mono }}>{o.pct}%</span>
      </div>
      <div style={{ fontSize: 11.5, color: textMute, marginTop: 5 }}>{o.var}</div>
      <div style={{
        marginTop: 8, padding: '8px 10px', borderRadius: 6,
        background: dark ? 'rgba(255,255,255,0.025)' : T.paper2,
        fontSize: 11.5, fontFamily: T.mono, lineHeight: 1.6,
        color: textInk,
      }}>
        {o.diff.map(([op, text], i) => {
          if (op === 'eq') return <span key={i} style={{ color: textMute }}>{text}</span>;
          if (op === 'add') return <span key={i} style={{ background: T.okSoft, color: T.ok, padding: '0 2px', borderRadius: 2 }}>+{text}</span>;
          if (op === 'del') return <span key={i} style={{ background: T.dangerSoft, color: T.danger, padding: '0 2px', borderRadius: 2, textDecoration: 'line-through' }}>−{text}</span>;
          return null;
        })}
      </div>
    </div>
  );
}

function ClusterSidebar({ dark, cardBg, border, textInk, textMute, accent }) {
  return (
    <div style={{
      background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
      display: 'flex', flexDirection: 'column', overflow: 'hidden',
    }}>
      <div style={{ display: 'flex', padding: 2, margin: 10, marginBottom: 0, background: dark ? 'rgba(255,255,255,0.04)' : T.paper2, borderRadius: 7 }}>
        <TabBtn active label="评论 · 4" dark={dark} accent={accent} />
        <TabBtn label="活动" dark={dark} accent={accent} />
        <TabBtn label="引用" dark={dark} accent={accent} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: '12px 14px', display: 'flex', flexDirection: 'column', gap: 14 }}>
        {[
          { name: '林一帆', color: '#1F7A3D', initial: '林', time: '上午 10:24',
            text: '这段在新版供应商协议里最好保留模板表述，法务统一过的语序，请勿改动。',
            tag: { fg: T.brand, bg: T.brandSoft, text: '@周明远' } },
          { name: '周明远', color: '#5B5BD6', initial: '周', time: '上午 10:31',
            text: '收到。我把华东v3 的 §3.1 标为「保留」，剩下 3 份让 AI 出统一改写稿。' },
          { name: '陈思雨', color: '#B86A1F', initial: '陈', time: '11:02',
            text: '海外版我加了双语版本，建议把英文表述单独抽到术语表里。',
            attach: '双语条款示例.docx' },
          { name: '高伟', color: '#1B5BB7', initial: '高', time: '11:48',
            text: '同意。另外建议升级为「公司级模板段落」加入审核库。', resolved: true },
        ].map((c, i) => (
          <CommentItem key={i} c={c} dark={dark} textInk={textInk} textMute={textMute} accent={accent} />
        ))}
      </div>
      <div style={{ padding: 12, borderTop: `1px solid ${border}` }}>
        <div style={{
          display: 'flex', alignItems: 'center', gap: 7, padding: '8px 10px',
          background: dark ? 'rgba(255,255,255,0.04)' : T.paper2,
          border: `1px solid ${border}`, borderRadius: 8,
        }}>
          <Avatar name="周" color="#5B5BD6" size={20} />
          <span style={{ fontSize: 11.5, color: textMute, flex: 1 }}>添加评论 · @ 提及成员</span>
          <VIcon name="paperclip" size={13} style={{ color: textMute }} />
          <VIcon name="ai" size={13} style={{ color: accent }} />
        </div>
      </div>
    </div>
  );
}

function TabBtn({ active, label, dark, accent }) {
  return (
    <div style={{
      flex: 1, textAlign: 'center', padding: '4px 0',
      fontSize: 11.5, fontWeight: 600, borderRadius: 5,
      background: active ? (dark ? '#2A2A33' : '#fff') : 'transparent',
      color: active ? (dark ? '#fff' : T.ink) : (dark ? 'rgba(255,255,255,0.55)' : T.ink3),
      boxShadow: active && !dark ? '0 1px 2px rgba(0,0,0,0.06)' : 'none',
      cursor: 'pointer',
    }}>{label}</div>
  );
}

function CommentItem({ c, dark, textInk, textMute, accent }) {
  return (
    <div style={{ display: 'flex', gap: 9, opacity: c.resolved ? 0.6 : 1 }}>
      <Avatar name={c.initial} color={c.color} size={26} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 7 }}>
          <span style={{ fontSize: 12, fontWeight: 700, color: textInk }}>{c.name}</span>
          <span style={{ fontSize: 10.5, color: textMute }}>{c.time}</span>
          {c.resolved && <Pill bg={T.okSoft} fg={T.ok} size={9.5}>已解决</Pill>}
        </div>
        <div style={{ fontSize: 12, color: dark ? 'rgba(255,255,255,0.85)' : T.ink2, lineHeight: 1.55, marginTop: 4 }}>
          {c.tag && <span style={{ color: c.tag.fg, fontWeight: 700, background: c.tag.bg, padding: '0 4px', borderRadius: 3 }}>{c.tag.text}</span>}
          {c.tag && ' '}
          {c.text}
        </div>
        {c.attach && (
          <div style={{
            display: 'inline-flex', alignItems: 'center', gap: 6, marginTop: 6,
            padding: '4px 8px', borderRadius: 6,
            background: dark ? 'rgba(255,255,255,0.05)' : T.paper2,
            fontSize: 11, color: textInk,
          }}>
            <DocChip type="docx" />
            {c.attach}
          </div>
        )}
      </div>
    </div>
  );
}


// ═════════════════════════════════════════════════════════════
// SCREEN 7 · Matrix (多文档交叉比对矩阵)
// ═════════════════════════════════════════════════════════════
function ScrMatrix({ accent = T.brand, dark = false }) {
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;
  const bg = dark ? '#1A1A1F' : T.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : T.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : T.line;

  const docs = [
    { short: '华东v3',  full: '保密协议·华东v3', dept: '法务' },
    { short: '西南v3',  full: '保密协议·西南v3', dept: '法务' },
    { short: '华北v3',  full: '保密协议·华北v3', dept: '法务' },
    { short: '海外v1',  full: '保密协议·海外v1', dept: '法务' },
    { short: '模板v2',  full: '保密协议·模板v2.5', dept: '总部' },
    { short: '历史v1',  full: '保密协议·2024归档', dept: '归档' },
  ];

  // generate similarity values
  const matrix = [
    [1.00, 0.92, 0.88, 0.62, 0.94, 0.71],
    [0.92, 1.00, 0.85, 0.58, 0.91, 0.68],
    [0.88, 0.85, 1.00, 0.55, 0.89, 0.73],
    [0.62, 0.58, 0.55, 1.00, 0.61, 0.42],
    [0.94, 0.91, 0.89, 0.61, 1.00, 0.74],
    [0.71, 0.68, 0.73, 0.42, 0.74, 1.00],
  ];

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, overflow: 'hidden' }}>
      <VTopbar dark={dark}
        crumbs={['工作区', '我的任务', '2026Q1 供应商保密协议', '交叉比对矩阵']}
        actions={<>
          <Button kind="ghost" icon="filter" dark={dark}>按相似度筛选</Button>
          <Button kind="secondary" icon="grid" dark={dark}>切换视图</Button>
          <Button kind="primary" accent={accent} icon="download">导出矩阵</Button>
        </>}
      />
      <div style={{ flex: 1, minHeight: 0, padding: 20, display: 'grid', gridTemplateColumns: '1fr 320px', gap: 16, overflow: 'hidden' }}>
        {/* matrix card */}
        <div style={{
          background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
          padding: 22, display: 'flex', flexDirection: 'column', overflow: 'auto',
        }}>
          <div style={{ display: 'flex', alignItems: 'baseline', marginBottom: 14 }}>
            <span style={{ fontSize: 14, fontWeight: 700, color: textInk }}>6 × 6 文档相似度矩阵</span>
            <span style={{ fontSize: 11.5, color: textMute, marginLeft: 8 }}>使用 SBERT-Chinese-768d 嵌入计算</span>
            <div style={{ flex: 1 }} />
            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <span style={{ fontSize: 10.5, color: textMute }}>低</span>
              <div style={{ width: 88, height: 8, borderRadius: 4, background: `linear-gradient(to right, ${T.okSoft}, ${T.hi1}, ${T.hi2}, ${T.hi3}, ${T.hi4})` }} />
              <span style={{ fontSize: 10.5, color: textMute }}>高</span>
            </div>
          </div>
          <Matrix docs={docs} matrix={matrix} dark={dark} textInk={textInk} textMute={textMute} accent={accent} />
        </div>

        {/* Side: insights */}
        <div style={{
          background: cardBg, border: `1px solid ${border}`, borderRadius: 12,
          padding: 18, overflow: 'auto', display: 'flex', flexDirection: 'column', gap: 14,
        }}>
          <div style={{ fontSize: 13, fontWeight: 700, color: textInk }}>关键洞察</div>
          {[
            { tag: '聚簇', fg: T.brand, bg: T.brandSoft, title: '华东 / 西南 / 华北 v3 高度同源', body: '三份文档与「模板 v2.5」的相似度均 ≥ 89%，建议直接引用模板并仅维护差异条款。' },
            { tag: '异常', fg: T.warn, bg: T.warnSoft, title: '海外v1 与历史v1 偏离最大', body: '海外版本仅与模板相似 61%，包含 18 处双语条款，建议单独维护英文术语映射表。' },
            { tag: '归档', fg: T.ok, bg: T.okSoft, title: '历史 v1 可归档', body: '所有现行版本对历史 v1 的相似度均低于 75%，可移至「冷备」库以减小检索面。' },
          ].map((ins, i) => (
            <div key={i} style={{
              padding: 12, borderRadius: 10,
              background: dark ? 'rgba(255,255,255,0.025)' : T.paper2,
              border: `1px solid ${border}`,
            }}>
              <Pill bg={ins.bg} fg={ins.fg} size={10}>{ins.tag}</Pill>
              <div style={{ fontSize: 12.5, fontWeight: 700, color: textInk, marginTop: 7 }}>{ins.title}</div>
              <div style={{ fontSize: 11.5, color: textMute, marginTop: 5, lineHeight: 1.55 }}>{ins.body}</div>
            </div>
          ))}
          <Button kind="ghost" size="sm" icon="ai" dark={dark} style={{ alignSelf: 'flex-start' }}>生成完整 AI 摘要</Button>
        </div>
      </div>
    </div>
  );
}

function Matrix({ docs, matrix, dark, textInk, textMute, accent }) {
  const cellColor = (v) => {
    if (v >= 0.9) return T.hi4;
    if (v >= 0.7) return T.hi3;
    if (v >= 0.5) return T.hi2;
    if (v >= 0.3) return T.hi1;
    return T.okSoft;
  };
  const cellFg = (v) => v >= 0.7 ? '#fff' : T.ink;
  return (
    <div style={{ display: 'grid', gridTemplateColumns: `100px repeat(${docs.length}, 1fr)`, gap: 4, minWidth: 0 }}>
      {/* top-left empty */}
      <div />
      {/* column headers */}
      {docs.map((d, i) => (
        <div key={i} style={{ textAlign: 'center' }}>
          <div style={{ fontSize: 11, fontWeight: 700, color: textInk }}>{d.short}</div>
          <div style={{ fontSize: 9.5, color: textMute, marginTop: 1 }}>{d.dept}</div>
        </div>
      ))}
      {/* rows */}
      {docs.map((d, r) => (
        <React.Fragment key={r}>
          <div style={{
            display: 'flex', alignItems: 'center', justifyContent: 'flex-end', gap: 6,
            paddingRight: 8,
          }}>
            <div style={{ textAlign: 'right' }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: textInk, lineHeight: 1.3 }}>{d.short}</div>
              <div style={{ fontSize: 9.5, color: textMute }}>{d.dept}</div>
            </div>
          </div>
          {matrix[r].map((v, c) => {
            const isDiag = r === c;
            return (
              <div key={c} style={{
                aspectRatio: '1.4 / 1',
                background: isDiag ? (dark ? 'rgba(255,255,255,0.04)' : T.paper2) : cellColor(v),
                color: isDiag ? textMute : cellFg(v),
                borderRadius: 6,
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                fontSize: 13, fontWeight: 700, fontFamily: T.mono, letterSpacing: '-0.01em',
                position: 'relative',
              }}>
                {isDiag ? '—' : `${(v * 100).toFixed(0)}`}
                {!isDiag && (
                  <span style={{
                    position: 'absolute', bottom: 4, right: 6,
                    fontSize: 8.5, opacity: 0.6, fontFamily: T.font, fontWeight: 600,
                  }}>%</span>
                )}
              </div>
            );
          })}
        </React.Fragment>
      ))}
    </div>
  );
}


// ═════════════════════════════════════════════════════════════
// SCREEN 8 · Export / Settings
// ═════════════════════════════════════════════════════════════
function ScrExport({ accent = T.brand, dark = false }) {
  const textInk = dark ? '#fff' : T.ink;
  const textMute = dark ? 'rgba(255,255,255,0.55)' : T.ink3;
  const bg = dark ? '#1A1A1F' : T.paper;
  const cardBg = dark ? 'rgba(255,255,255,0.04)' : T.white;
  const border = dark ? 'rgba(255,255,255,0.08)' : T.line;

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', background: bg, overflow: 'hidden' }}>
      <VTopbar dark={dark}
        crumbs={['工作区', '我的任务', 'Verity 产品白皮书 · 初稿', '导出报告']}
        actions={<Button kind="primary" accent={accent} icon="download">立即导出</Button>}
      />
      <div style={{ flex: 1, minHeight: 0, padding: 20, display: 'grid', gridTemplateColumns: '420px 1fr', gap: 16, overflow: 'auto' }}>
        {/* LEFT: settings */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
            <SettingField label="文件格式" textMute={textMute}>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 8 }}>
                {[
                  { t: 'pdf', label: 'PDF · 标准', sub: '便于分发与签批', active: true },
                  { t: 'docx', label: 'Word', sub: '可编辑修订' },
                  { t: 'xlsx', label: 'Excel · 数据表', sub: '片段明细 CSV' },
                ].map((o, i) => (
                  <div key={i} style={{
                    padding: 10, borderRadius: 8, cursor: 'pointer',
                    border: `1.5px solid ${o.active ? accent : border}`,
                    background: o.active ? (dark ? 'rgba(91,91,214,0.12)' : `${accent}10`) : (dark ? 'rgba(255,255,255,0.02)' : '#fff'),
                  }}>
                    <DocChip type={o.t === 'xlsx' ? 'xls' : o.t} />
                    <div style={{ fontSize: 12, fontWeight: 600, color: textInk, marginTop: 7 }}>{o.label}</div>
                    <div style={{ fontSize: 10.5, color: textMute, marginTop: 2 }}>{o.sub}</div>
                  </div>
                ))}
              </div>
            </SettingField>
          </div>

          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18, display: 'flex', flexDirection: 'column', gap: 14 }}>
            <SettingField label="报告模板" textMute={textMute}>
              <SegControl options={['法务合规', '学术规范', '内部精简', '自定义']} value={0} accent={accent} dark={dark} />
            </SettingField>
            <SettingField label="包含内容" textMute={textMute}>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 7 }}>
                <Check label="封面与执行摘要" checked dark={dark} accent={accent} />
                <Check label="总览相似度统计图表" checked dark={dark} accent={accent} />
                <Check label="逐章节相似度详情" checked dark={dark} accent={accent} />
                <Check label="左右对比快照（最多 24 处）" checked dark={dark} accent={accent} />
                <Check label="AI 改写建议（含修订追踪）" checked dark={dark} accent={accent} />
                <Check label="GB/T 7714 引用列表" checked dark={dark} accent={accent} />
                <Check label="附录：完整片段清单" dark={dark} accent={accent} />
              </div>
            </SettingField>
            <SettingField label="水印与权限" textMute={textMute}>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
                  <span style={{ fontSize: 12, color: textInk, flex: 1 }}>添加水印</span>
                  <Toggle on accent={accent} />
                </div>
                <div style={{
                  padding: '7px 10px', borderRadius: 6,
                  background: dark ? 'rgba(255,255,255,0.04)' : T.paper2,
                  border: `1px solid ${border}`, fontSize: 11.5, color: textInk,
                  fontFamily: T.mono,
                }}>
                  机密 · 法务合规部 · {`{user}`} · {`{date}`}
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
                  <VIcon name="lock" size={12} style={{ color: textMute }} />
                  <span style={{ fontSize: 12, color: textInk, flex: 1 }}>密码保护</span>
                  <Toggle on accent={accent} />
                </div>
              </div>
            </SettingField>
            <SettingField label="导出后" textMute={textMute}>
              <Check label="自动归档至任务并通知 @负责人" checked dark={dark} accent={accent} />
            </SettingField>
          </div>
        </div>

        {/* RIGHT: preview */}
        <div style={{
          background: dark ? '#15151B' : '#E8E5DE', borderRadius: 12,
          border: `1px solid ${border}`, padding: 24,
          display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 16,
          overflow: 'auto',
        }}>
          <div style={{ fontSize: 11.5, color: textMute }}>报告预览 · 共 38 页</div>
          {/* Page 1 — cover */}
          <ReportPage>
            <div style={{ borderTop: `4px solid ${accent}`, paddingTop: 22, marginBottom: 18 }}>
              <div style={{ fontSize: 10, color: '#65626E', letterSpacing: '0.08em', textTransform: 'uppercase', fontWeight: 700 }}>
                Verity · 查重报告
              </div>
              <div style={{ fontSize: 26, fontWeight: 700, color: '#16151A', marginTop: 10, letterSpacing: '-0.018em', lineHeight: 1.2 }}>
                Verity 产品白皮书 · 初稿
              </div>
              <div style={{ fontSize: 11, color: '#65626E', marginTop: 14 }}>
                作者：陈思雨 · 市场部 · 22 页 · 13,840 字<br/>
                生成时间：2026-05-26 14:28<br/>
                查重源：内部文档库 + 行业公开文献（共 5.2 万份）
              </div>
            </div>
            <div style={{
              display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 14,
              padding: '16px 0', borderTop: `1px solid #E8E4DC`, borderBottom: `1px solid #E8E4DC`,
            }}>
              <div>
                <div style={{ fontSize: 9, fontWeight: 700, color: '#65626E', letterSpacing: '0.06em', textTransform: 'uppercase' }}>总体相似度</div>
                <div style={{ fontSize: 32, fontWeight: 700, color: T.hi3, marginTop: 4, letterSpacing: '-0.02em', fontFamily: T.font }}>40.2%</div>
              </div>
              <div>
                <div style={{ fontSize: 9, fontWeight: 700, color: '#65626E', letterSpacing: '0.06em', textTransform: 'uppercase' }}>需复核片段</div>
                <div style={{ fontSize: 32, fontWeight: 700, color: '#16151A', marginTop: 4, letterSpacing: '-0.02em', fontFamily: T.font }}>24<span style={{ fontSize: 13, color: '#65626E', marginLeft: 4 }}>处</span></div>
              </div>
            </div>
            <div style={{ fontSize: 10.5, color: '#3D3B45', marginTop: 16, lineHeight: 1.6 }}>
              本报告由 Verity 查重平台自动生成。报告依据企业内部对比库 + 行业公开文献库识别出 24 处需要复核的相似片段，其中
              高相似度 (≥70%) 9 处、中相似度 (40%-70%) 12 处、低相似度 ({'<'}40%) 3 处。建议依据「§3.2 安全架构」「§5.1 合规性」两章重点修订……
            </div>
          </ReportPage>
          <ReportPage>
            <div style={{ fontSize: 13, fontWeight: 700, color: '#16151A', marginBottom: 12 }}>2. 各章节相似度分布</div>
            <div style={{ display: 'flex', alignItems: 'flex-end', gap: 6, height: 100, paddingBottom: 6, borderBottom: `1px solid #E8E4DC` }}>
              {[0.18, 0.42, 0.82, 0.55, 0.74, 0.28, 0.46, 0.12, 0.36].map((v, i) => {
                const c = v >= 0.7 ? T.hi4 : v >= 0.5 ? T.hi3 : v >= 0.3 ? T.hi2 : v >= 0.15 ? T.hi1 : T.ok;
                return <div key={i} style={{ flex: 1, height: `${v * 100}%`, background: c, borderRadius: 2, opacity: 0.85 }} />;
              })}
            </div>
            <div style={{ fontSize: 9, color: '#65626E', textAlign: 'center', marginTop: 6 }}>§1 — §9</div>
            <div style={{ marginTop: 16, fontSize: 11, fontWeight: 700, color: '#16151A' }}>2.1 高相似度章节摘要</div>
            <ul style={{ fontSize: 10.5, color: '#3D3B45', lineHeight: 1.7, paddingLeft: 18, marginTop: 6 }}>
              <li>§3.2 安全架构 · 92% · 与「Verity 产品介绍 2025 v3 §2.1」高度同源</li>
              <li>§5.1 合规性 · 84% · 与「数据安全合规白皮书 §4.3」段落相似</li>
              <li>§7 部署模式 · 71% · 与「SOC2 实施指南 §6.2」结构相似</li>
            </ul>
            <div style={{ marginTop: 14, fontSize: 11, fontWeight: 700, color: '#16151A' }}>2.2 建议修订路径</div>
            <div style={{ fontSize: 10.5, color: '#3D3B45', lineHeight: 1.7, marginTop: 6 }}>
              建议采用「先模板化后差异化」原则：将与企业内部白皮书重叠的核心定义段落统一抽取至术语表，
              再在白皮书章节中以引用形式呈现……
            </div>
          </ReportPage>
        </div>
      </div>
    </div>
  );
}

function ReportPage({ children }) {
  return (
    <div style={{
      width: 360, padding: '34px 40px 40px', background: '#fff',
      boxShadow: '0 8px 24px rgba(0,0,0,0.10), 0 1px 0 rgba(0,0,0,0.04)',
      fontSize: 11, color: '#16151A', fontFamily: T.font,
      lineHeight: 1.55,
    }}>
      {children}
    </div>
  );
}


Object.assign(window, {
  ScrDetail, ScrClusters, ScrMatrix, ScrExport,
});
