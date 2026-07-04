// 本地持久化偏好。检测参数（语义/范围/阈值/模板/围标）已迁至 DB app_settings，
// 此处仅剩「自动清理」开关；比对范围枚举 Scope 供设置页的 DB 配置控件复用。
export type Scope = "full" | "tech" | "business";

export interface Settings {
  autoClean: boolean; // 自动清理 30 天前任务
}

const KEY = "bidguard-settings";

export const DEFAULT_SETTINGS: Settings = {
  autoClean: false,
};

export function getSettings(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
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
