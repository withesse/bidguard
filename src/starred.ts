// 收藏 / 置顶的任务 id（本地持久化）。「我的任务」只显示收藏的，「历史记录」显示全部。
const KEY = "bidguard-starred";

export function getStarred(): Set<string> {
  try {
    const r = localStorage.getItem(KEY);
    if (r) return new Set(JSON.parse(r) as string[]);
  } catch {
    // 回落空集
  }
  return new Set();
}

export function isStarred(id: string): boolean {
  return getStarred().has(id);
}

/** 切换收藏状态，返回切换后的状态。 */
export function toggleStarred(id: string): boolean {
  const s = getStarred();
  let now: boolean;
  if (s.has(id)) {
    s.delete(id);
    now = false;
  } else {
    s.add(id);
    now = true;
  }
  try {
    localStorage.setItem(KEY, JSON.stringify([...s]));
  } catch {
    // 忽略
  }
  return now;
}
