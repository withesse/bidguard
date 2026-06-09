// c-app.jsx — Design canvas hosting macOS + Windows + variations of 「原本」

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "primaryColor": "#4F58A8",
  "highlight": "amber",
  "sidebarLayout": "comfort",
  "fontScale": "regular",
  "dark": false
}/*EDITMODE-END*/;

function App() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const accent = t.primaryColor;
  const dark = !!t.dark;
  const hi = t.highlight;
  const layout = t.sidebarLayout;

  const W = 1280, H = 820;

  const renderApp = (Screen, opts = {}) => (
    <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
      <CSidebar active={opts.active || 'home'} dark={dark} accent={accent} layout={layout} />
      <Screen accent={accent} dark={dark} hi={hi} />
    </div>
  );

  return (
    <>
      <DesignCanvas>
        {/* ─────────────────────────────────────────────── */}
        <DCSection id="mac" title="macOS · 原本"
          subtitle="6 个核心界面 · 留白克制 · 单文档为中心的个人工作流">
          <DCArtboard id="mac-home"     label="01 · 首页 / 上传" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本" accent={accent}>
              {renderApp(CScrHome, { active: 'home' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-scan"     label="02 · 检测中" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 正在检测" accent={accent}>
              {renderApp(CScrScan, { active: 'home' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-report"   label="03 · 检测报告" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 检测报告" accent={accent}>
              {renderApp(CScrReport, { active: 'home' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-compare"  label="04 · 逐段对比" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 逐段对比" accent={accent}>
              {renderApp(CScrCompare, { active: 'home' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-docs"     label="05 · 我的文档" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 我的文档" accent={accent}>
              {renderApp(CScrDocs, { active: 'docs' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-settings" label="06 · 设置" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 设置" accent={accent}>
              {renderApp(CScrSettings, { active: 'settings' })}
            </CMacFrame>
          </DCArtboard>
        </DCSection>

        {/* ─────────────────────────────────────────────── */}
        <DCSection id="win" title="Windows 11 · 原本"
          subtitle="Fluent / WinUI 3 题栏 · 与 macOS 共享设计语言 · 仅展示视觉差异">
          <DCArtboard id="win-home"     label="01 · 首页 / 上传" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本" accent={accent}>
              {renderApp(CScrHome, { active: 'home' })}
            </CWinFrame>
          </DCArtboard>

          <DCArtboard id="win-report"   label="02 · 检测报告" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本 · 检测报告" accent={accent}>
              {renderApp(CScrReport, { active: 'home' })}
            </CWinFrame>
          </DCArtboard>

          <DCArtboard id="win-compare"  label="03 · 逐段对比" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本 · 逐段对比" accent={accent}>
              {renderApp(CScrCompare, { active: 'home' })}
            </CWinFrame>
          </DCArtboard>

          <DCArtboard id="win-docs"     label="04 · 我的文档" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本 · 我的文档" accent={accent}>
              {renderApp(CScrDocs, { active: 'docs' })}
            </CWinFrame>
          </DCArtboard>
        </DCSection>

        {/* ─────────────────────────────────────────────── */}
        <DCSection id="variations" title="视觉变体 · 同一界面三种方向"
          subtitle="以「首页 / 上传」为对比画布 · 默认 / 暗色 / 暖纸三种方向">
          <DCArtboard id="var-default" label="A · 默认 · 素雅靛蓝" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · A 素雅靛蓝" accent="#4F58A8">
              <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
                <CSidebar active="home" dark={false} accent="#4F58A8" layout="comfort" />
                <CScrHome accent="#4F58A8" dark={false} hi="amber" />
              </div>
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="var-dark"    label="B · 暗色 · 青绿冷调" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · B 暗色青绿" accent="#0E9A8F" dark>
              <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
                <CSidebar active="home" dark={true} accent="#0E9A8F" layout="comfort" />
                <CScrHome accent="#0E9A8F" dark={true} hi="amber" />
              </div>
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="var-warm"    label="C · 暖纸 · 红土陶土" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · C 暖纸红土" accent="#C84D2E">
              <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
                <CSidebar active="home" dark={false} accent="#C84D2E" layout="comfort" />
                <CScrHome accent="#C84D2E" dark={false} hi="amber" />
              </div>
            </CMacFrame>
          </DCArtboard>
        </DCSection>
      </DesignCanvas>

      {/* Tweaks */}
      <TweaksPanel title="原本 · Tweaks">
        <TweakSection label="主题">
          <TweakColor label="品牌色" value={t.primaryColor}
            options={['#4F58A8', '#2E5BFF', '#0E9A8F', '#C84D2E', '#7C3AED', '#2B2D33']}
            onChange={(v) => setTweak('primaryColor', v)} />
          <TweakToggle label="暗色模式" value={t.dark} onChange={(v) => setTweak('dark', v)} />
        </TweakSection>
        <TweakSection label="侧栏 & 字号">
          <TweakRadio label="侧栏" value={t.sidebarLayout}
            options={['comfort', 'compact']}
            onChange={(v) => setTweak('sidebarLayout', v)} />
          <TweakRadio label="字号" value={t.fontScale}
            options={['compact', 'regular', 'comfy']}
            onChange={(v) => setTweak('fontScale', v)} />
        </TweakSection>
        <TweakSection label="高亮配色">
          <TweakRadio label="方案" value={t.highlight}
            options={['amber', 'rose', 'blue']}
            onChange={(v) => setTweak('highlight', v)} />
        </TweakSection>
      </TweaksPanel>
    </>
  );
}

const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(<App />);
