// app.jsx — Top-level layout: design canvas of macOS + Windows + variation artboards

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "primaryColor": "#5B5BD6",
  "highlight": "amber",
  "sidebarLayout": "full",
  "fontScale": "regular",
  "dark": false
}/*EDITMODE-END*/;

function App() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const accent = t.primaryColor;
  const dark = !!t.dark;
  const hi = t.highlight;
  const collapsed = t.sidebarLayout === 'collapsed';
  const fontScalePx = { compact: 13, regular: 14, comfy: 15 }[t.fontScale] || 14;

  // Inject runtime font-size on the root so screens inherit
  React.useEffect(() => {
    document.documentElement.style.setProperty('--v-font-size', fontScalePx + 'px');
  }, [fontScalePx]);

  // Standard artboard size — 1320×820 (chrome eats ~36px of titlebar)
  const W = 1320, H = 820;

  const renderApp = (Screen, opts = {}) => (
    <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
      <VSidebar
        active={opts.active || 'tasks'}
        dark={dark}
        accent={accent}
        collapsed={collapsed}
      />
      <Screen accent={accent} dark={dark} hi={hi} />
    </div>
  );

  return (
    <>
      <DesignCanvas>
        <DCSection id="mac" title="macOS · Verity 查重工作台"
          subtitle="基于 macOS Tahoe 风格 · 全部 7 个核心界面 · Liquid Glass 题栏 · 12px 圆角">
          <DCArtboard id="mac-tasks"     label="01 · 任务列表 / 历史记录" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · 我的任务" accent={accent}>
              {renderApp(ScrTasks, { active: 'tasks' })}
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="mac-import"    label="02 · 导入 / 拖拽上传" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · 新建查重任务" accent={accent}>
              {renderApp(ScrImport, { active: 'inbox' })}
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="mac-progress"  label="03 · 查重进度" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · 查重进度" accent={accent}>
              {renderApp(ScrProgress, { active: 'tasks' })}
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="mac-overview"  label="04 · 总览报告" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · 总览报告" accent={accent}>
              {renderApp(ScrOverview, { active: 'tasks' })}
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="mac-detail"    label="05 · 左右分屏对比" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · 详细对比" accent={accent}>
              {renderApp(ScrDetail, { active: 'tasks' })}
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="mac-clusters"  label="06 · 片段聚合 + 评论协作" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · 片段聚合" accent={accent}>
              {renderApp(ScrClusters, { active: 'snippets' })}
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="mac-matrix"    label="07 · 交叉比对矩阵" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · 交叉比对矩阵" accent={accent}>
              {renderApp(ScrMatrix, { active: 'tasks' })}
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="mac-export"    label="08 · 导出报告 / 设置" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · 导出报告" accent={accent}>
              {renderApp(ScrExport, { active: 'tasks' })}
            </MacFrame>
          </DCArtboard>
        </DCSection>

        <DCSection id="win" title="Windows 11 · Verity 查重工作台"
          subtitle="基于 Fluent / WinUI 3 · 8px 圆角 · Mica 风格题栏 · 与 macOS 版面共享设计语言">
          <DCArtboard id="win-tasks"    label="01 · 任务列表 / 历史记录" width={W} height={H}>
            <WinFrame width={W} height={H} title="Verity · 我的任务" accent={accent}>
              {renderApp(ScrTasks, { active: 'tasks' })}
            </WinFrame>
          </DCArtboard>

          <DCArtboard id="win-import"   label="02 · 导入 / 拖拽上传" width={W} height={H}>
            <WinFrame width={W} height={H} title="Verity · 新建查重任务" accent={accent}>
              {renderApp(ScrImport, { active: 'inbox' })}
            </WinFrame>
          </DCArtboard>

          <DCArtboard id="win-detail"   label="03 · 左右分屏对比" width={W} height={H}>
            <WinFrame width={W} height={H} title="Verity · 详细对比" accent={accent}>
              {renderApp(ScrDetail, { active: 'tasks' })}
            </WinFrame>
          </DCArtboard>

          <DCArtboard id="win-overview" label="04 · 总览报告" width={W} height={H}>
            <WinFrame width={W} height={H} title="Verity · 总览报告" accent={accent}>
              {renderApp(ScrOverview, { active: 'tasks' })}
            </WinFrame>
          </DCArtboard>

          <DCArtboard id="win-clusters" label="05 · 片段聚合" width={W} height={H}>
            <WinFrame width={W} height={H} title="Verity · 片段聚合" accent={accent}>
              {renderApp(ScrClusters, { active: 'snippets' })}
            </WinFrame>
          </DCArtboard>
        </DCSection>

        <DCSection id="variations" title="视觉变体 · 同一界面三种方向"
          subtitle="以「左右分屏对比」为画布,对比品牌色 / 暗色 / 高亮配色三种取向">
          <DCArtboard id="var-default" label="A · 默认 · 蓝紫 + 琥珀下划线" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · A 默认" accent="#5B5BD6">
              <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
                <VSidebar active="tasks" dark={false} accent="#5B5BD6" />
                <ScrDetail accent="#5B5BD6" dark={false} hi="amber" />
              </div>
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="var-dark" label="B · 暗色 · 青绿 + 玫红下划线" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · B 暗色" accent="#0EA98F" dark>
              <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
                <VSidebar active="tasks" dark={true} accent="#0EA98F" />
                <ScrDetail accent="#0EA98F" dark={true} hi="rose" />
              </div>
            </MacFrame>
          </DCArtboard>

          <DCArtboard id="var-warm" label="C · 暖色编辑器风 · 红土 + 暖琥珀" width={W} height={H}>
            <MacFrame width={W} height={H} title="Verity · C 暖色" accent="#C84D2E">
              <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
                <VSidebar active="tasks" dark={false} accent="#C84D2E" />
                <ScrDetail accent="#C84D2E" dark={false} hi="amber" />
              </div>
            </MacFrame>
          </DCArtboard>
        </DCSection>
      </DesignCanvas>

      {/* Tweaks panel */}
      <TweaksPanel title="Tweaks · Verity 查重工作台">
        <TweakSection label="主题">
          <TweakColor label="品牌色" value={t.primaryColor}
            options={['#5B5BD6', '#2C6FE0', '#0EA98F', '#C84D2E', '#7C3AED', '#1F1F26']}
            onChange={(v) => setTweak('primaryColor', v)} />
          <TweakToggle label="暗色模式" value={t.dark} onChange={(v) => setTweak('dark', v)} />
        </TweakSection>
        <TweakSection label="布局 & 排版">
          <TweakRadio label="侧栏" value={t.sidebarLayout}
            options={['full', 'collapsed']}
            onChange={(v) => setTweak('sidebarLayout', v)} />
          <TweakRadio label="字号" value={t.fontScale}
            options={['compact', 'regular', 'comfy']}
            onChange={(v) => setTweak('fontScale', v)} />
        </TweakSection>
        <TweakSection label="高亮配色">
          <TweakRadio label="方案" value={t.highlight}
            options={['amber', 'rose', 'green']}
            onChange={(v) => setTweak('highlight', v)} />
        </TweakSection>
      </TweaksPanel>
    </>
  );
}

const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(<App />);
