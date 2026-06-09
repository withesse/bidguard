// 检测偏好（本地持久化）
export type Scope = "full" | "tech" | "business";

export interface Settings {
  semantic: boolean; // 语义查重（embedding 叠加）
  scope: Scope; // 比对范围：完整 / 仅技术标 / 仅商务标
  threshold: number; // 最低相似度阈值 0..1（低于此值不进入报告）
  ignoreTemplates: boolean; // 忽略通用模板 / 查重源样板
  flagCollusion: boolean; // 围标风险提示
  industryLink: boolean; // 联动工商关联（需外部数据源，未配置则不参与判定）
  autoClean: boolean; // 自动清理 30 天前任务
}

const KEY = "bidguard-settings";
const LEGACY_SEMANTIC = "bidguard-semantic";

export const DEFAULT_SETTINGS: Settings = {
  semantic: false,
  scope: "full",
  threshold: 0.35,
  ignoreTemplates: true,
  flagCollusion: true,
  industryLink: false,
  autoClean: false,
};

export function getSettings(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
    // 兼容旧版单键
    const legacy = localStorage.getItem(LEGACY_SEMANTIC);
    if (legacy != null) return { ...DEFAULT_SETTINGS, semantic: legacy === "true" };
  } catch {
    // 解析失败回落默认
  }
  return { ...DEFAULT_SETTINGS };
}

export function setSettings(patch: Partial<Settings>): Settings {
  const next = { ...getSettings(), ...patch };
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // 静默忽略
  }
  return next;
}

// —— 向后兼容的便捷 API ——
export function getSemantic(): boolean {
  return getSettings().semantic;
}
export function setSemantic(v: boolean): void {
  setSettings({ semantic: v });
}
