// bid-app.jsx — Design canvas: macOS + Windows + variations for 标书 use case

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

  const W = 1320, H = 840;

  const renderApp = (Screen, opts = {}) => (
    <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
      <BidSidebar active={opts.active || 'home'} dark={dark} accent={accent} layout={layout} />
      <Screen accent={accent} dark={dark} hi={hi} />
    </div>
  );

  return (
    <>
      <DesignCanvas>
        {/* ─────────────────────────────────────────────── */}
        <DCSection id="mac" title="macOS · 原本 · 标书查重"
          subtitle="8 个核心界面 · 2-5 份标书交叉比对 · 围标嫌疑识别">
          <DCArtboard id="mac-home"     label="01 · 新建任务" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 标书查重" accent={accent}>
              {renderApp(BidScrHome, { active: 'home' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-tasks"    label="02 · 我的任务" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 我的任务" accent={accent}>
              {renderApp(BidScrTasks, { active: 'tasks' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-scan"     label="03 · 检测中" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 正在比对" accent={accent}>
              {renderApp(BidScrScan, { active: 'tasks' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-matrix"   label="04 · 报告 · 交叉矩阵" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 检测报告" accent={accent}>
              {renderApp(BidScrMatrix, { active: 'tasks' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-compare"  label="05 · 逐对对比" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 逐对对比" accent={accent}>
              {renderApp(BidScrCompare, { active: 'tasks' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-clusters" label="06 · 重复条款聚合" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 重复条款聚合" accent={accent}>
              {renderApp(BidScrClusters, { active: 'tasks' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-export"   label="07 · 导出报告" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 导出报告" accent={accent}>
              {renderApp(BidScrExport, { active: 'tasks' })}
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="mac-settings" label="08 · 设置" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · 设置" accent={accent}>
              {renderApp(BidScrSettings, { active: 'settings' })}
            </CMacFrame>
          </DCArtboard>
        </DCSection>

        {/* ─────────────────────────────────────────────── */}
        <DCSection id="win" title="Windows 11 · 原本 · 标书查重"
          subtitle="Fluent / WinUI 3 · 与 macOS 共享设计语言">
          <DCArtboard id="win-home"     label="01 · 新建任务" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本 · 标书查重" accent={accent}>
              {renderApp(BidScrHome, { active: 'home' })}
            </CWinFrame>
          </DCArtboard>

          <DCArtboard id="win-matrix"   label="02 · 报告 · 交叉矩阵" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本 · 检测报告" accent={accent}>
              {renderApp(BidScrMatrix, { active: 'tasks' })}
            </CWinFrame>
          </DCArtboard>

          <DCArtboard id="win-compare"  label="03 · 逐对对比" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本 · 逐对对比" accent={accent}>
              {renderApp(BidScrCompare, { active: 'tasks' })}
            </CWinFrame>
          </DCArtboard>

          <DCArtboard id="win-clusters" label="04 · 重复条款聚合" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本 · 重复条款聚合" accent={accent}>
              {renderApp(BidScrClusters, { active: 'tasks' })}
            </CWinFrame>
          </DCArtboard>

          <DCArtboard id="win-tasks"    label="05 · 我的任务" width={W} height={H}>
            <CWinFrame width={W} height={H} title="原本 · 我的任务" accent={accent}>
              {renderApp(BidScrTasks, { active: 'tasks' })}
            </CWinFrame>
          </DCArtboard>
        </DCSection>

        {/* ─────────────────────────────────────────────── */}
        <DCSection id="variations" title="视觉变体 · 报告主屏三种方向"
          subtitle="以「交叉矩阵」为对比画布 · 默认 / 暗色 / 暖纸三种取向">
          <DCArtboard id="var-default" label="A · 默认 · 素雅靛蓝" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · A 素雅靛蓝" accent="#4F58A8">
              <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
                <BidSidebar active="tasks" dark={false} accent="#4F58A8" layout="comfort" />
                <BidScrMatrix accent="#4F58A8" dark={false} />
              </div>
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="var-dark"    label="B · 暗色 · 青绿冷调" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · B 暗色青绿" accent="#0E9A8F" dark>
              <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
                <BidSidebar active="tasks" dark={true} accent="#0E9A8F" layout="comfort" />
                <BidScrMatrix accent="#0E9A8F" dark={true} />
              </div>
            </CMacFrame>
          </DCArtboard>

          <DCArtboard id="var-warm"    label="C · 暖纸 · 红土陶土" width={W} height={H}>
            <CMacFrame width={W} height={H} title="原本 · C 暖纸红土" accent="#C84D2E">
              <div style={{ flex: 1, display: 'flex', minHeight: 0, minWidth: 0 }}>
                <BidSidebar active="tasks" dark={false} accent="#C84D2E" layout="comfort" />
                <BidScrMatrix accent="#C84D2E" dark={false} />
              </div>
            </CMacFrame>
          </DCArtboard>
        </DCSection>
      </DesignCanvas>

      <TweaksPanel title="原本 · 标书查重 · Tweaks">
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
