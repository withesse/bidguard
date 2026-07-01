// 屏 · 查重源 —— 通用样板库（落库：导入分块时标记命中样板）。
// 折叠分区 + 搜索 + 行内编辑/启停/删除确认；批量导入见 BatchImportModal（批 3）。
import { useId, useMemo, useState, type CSSProperties, type ReactNode } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button, Pill, Toggle, SegControl } from "../components/primitives";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import {
  useDeleteTemplate,
  useSaveTemplate,
  useSetTemplateEnabled,
  useTemplates,
} from "../queries/data";
import type { TemplateDto } from "../api/types";
import { BatchImportModal } from "./BatchImportModal";

const UNCATEGORIZED = "未分类";
const COLLAPSE_KEY = "library.collapsed";

const catOf = (t: TemplateDto) => t.category?.trim() || UNCATEGORIZED;

function loadCollapsed(): Set<string> {
  try {
    const raw = localStorage.getItem(COLLAPSE_KEY);
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}
function saveCollapsed(s: Set<string>) {
  try {
    localStorage.setItem(COLLAPSE_KEY, JSON.stringify([...s]));
  } catch {
    /* localStorage 不可用时忽略，折叠态退化为不持久 */
  }
}

/** 把命中关键词高亮（不区分大小写）。 */
function highlight(text: string, q: string, accent: string): ReactNode {
  if (!q) return text;
  const lower = text.toLowerCase();
  const needle = q.toLowerCase();
  const out: ReactNode[] = [];
  let i = 0;
  let k = 0;
  while (i < text.length) {
    const at = lower.indexOf(needle, i);
    if (at < 0) {
      out.push(text.slice(i));
      break;
    }
    if (at > i) out.push(text.slice(i, at));
    out.push(
      <mark key={k++} style={{ background: `${accent}33`, color: "inherit", borderRadius: 2 }}>
        {text.slice(at, at + needle.length)}
      </mark>,
    );
    i = at + needle.length;
  }
  return <>{out}</>;
}

export function Library() {
  const { dark, accent } = useTheme();
  const toast = useToast();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  const { data } = useTemplates();
  const items = useMemo(() => data ?? [], [data]);
  const save = useSaveTemplate();
  const delTpl = useDeleteTemplate();
  const setEnabled = useSetTemplateEnabled();

  const [query, setQuery] = useState("");
  const [activeCat, setActiveCat] = useState<string | null>(null);
  const [sort, setSort] = useState(0); // 0=最近 1=名称
  const [collapsed, setCollapsed] = useState<Set<string>>(loadCollapsed);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [showBatch, setShowBatch] = useState(false);

  const enabledCount = items.filter((t) => t.enabled).length;
  const existingTexts = useMemo(() => new Set(items.map((t) => t.text.trim())), [items]);

  // 分类清单（计数，'未分类'置底）
  const categories = useMemo(() => {
    const m = new Map<string, number>();
    for (const t of items) m.set(catOf(t), (m.get(catOf(t)) ?? 0) + 1);
    const arr = [...m.entries()];
    arr.sort((a, b) =>
      a[0] === UNCATEGORIZED ? 1 : b[0] === UNCATEGORIZED ? -1 : a[0].localeCompare(b[0], "zh"),
    );
    return arr;
  }, [items]);
  const catNames = categories.map(([c]) => c);

  // 过滤 + 排序 + 按分类分组
  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    let list = items;
    if (q) list = list.filter((t) => t.name.toLowerCase().includes(q) || t.text.toLowerCase().includes(q));
    if (activeCat) list = list.filter((t) => catOf(t) === activeCat);
    const sorted = [...list].sort((a, b) =>
      sort === 1 ? a.name.localeCompare(b.name, "zh") : b.createdAt.localeCompare(a.createdAt),
    );
    const m = new Map<string, TemplateDto[]>();
    for (const t of sorted) {
      const c = catOf(t);
      if (!m.has(c)) m.set(c, []);
      m.get(c)!.push(t);
    }
    return [...m.entries()].sort((a, b) =>
      a[0] === UNCATEGORIZED ? 1 : b[0] === UNCATEGORIZED ? -1 : a[0].localeCompare(b[0], "zh"),
    );
  }, [items, query, activeCat, sort]);

  const toggleCollapse = (cat: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(cat)) next.delete(cat);
      else next.add(cat);
      saveCollapsed(next);
      return next;
    });
  };

  const onSave = (
    payload: { id?: string; name: string; text: string; category?: string | null },
    onOk: () => void,
  ) => {
    if (!payload.name.trim() || !payload.text.trim()) return;
    // 归一分类：空串与保留哨兵「未分类」都落 null，避免把合成桶写成真实分类
    const cat = payload.category?.trim();
    save.mutate(
      {
        ...payload,
        name: payload.name.trim(),
        text: payload.text.trim(),
        category: cat && cat !== UNCATEGORIZED ? cat : null,
      },
      { onSuccess: onOk, onError: (e) => toast.show("保存失败：" + errMsg(e), "error") },
    );
  };

  const queryActive = query.trim().length > 0;
  const visibleCount = groups.reduce((n, [, ts]) => n + ts.length, 0);

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar
        title="查重源"
        sub={`共 ${items.length} 条 · 启用 ${enabledCount} 条`}
        search={{ value: query, onChange: setQuery, placeholder: "搜索名称或正文…" }}
        actions={
          <div style={{ display: "flex", gap: 8 }}>
            <Button kind="secondary" size="sm" icon="upload" onClick={() => setShowBatch(true)}>
              批量导入
            </Button>
            <Button kind="primary" size="sm" icon="plus" onClick={() => setAdding((a) => !a)}>
              新增样板
            </Button>
          </div>
        }
      />
      <div style={{ flex: 1, overflow: "auto" }}>
        {/* sticky 工具条：分类筛选 + 排序 + 重新导入提示 */}
        <div
          style={{
            position: "sticky",
            top: 0,
            zIndex: 5,
            background: bg,
            borderBottom: `1px solid ${border}`,
            padding: "10px 48px",
          }}
        >
          <div style={{ maxWidth: 860, margin: "0 auto", display: "flex", flexDirection: "column", gap: 8 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
              <FilterPill label={`全部 ${items.length}`} active={activeCat === null} accent={accent} dark={dark} onClick={() => setActiveCat(null)} />
              {categories.map(([c, n]) => (
                <FilterPill key={c} label={`${c} ${n}`} active={activeCat === c} accent={accent} dark={dark} onClick={() => setActiveCat(c)} />
              ))}
              <div style={{ flex: 1 }} />
              <SegControl options={["最近", "名称"]} value={sort} onChange={setSort} />
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 11, color: mute }}>
              <Icon name="info" size={12} style={{ color: mute }} />
              修改样板后需<b style={{ color: dark ? "rgba(255,255,255,0.75)" : C.ink2, fontWeight: 600 }}>重新导入文档</b>才生效；停用的样板不参与剔除。
              {queryActive && <span style={{ marginLeft: 4 }}>· 搜索「{query.trim()}」命中 {visibleCount} 条</span>}
            </div>
          </div>
        </div>

        <div style={{ padding: "20px 48px 40px" }}>
          <div style={{ maxWidth: 860, margin: "0 auto", display: "flex", flexDirection: "column", gap: 14 }}>
            {/* 新增表单 */}
            {adding && (
              <EditForm
                title="新增样板"
                cats={catNames}
                presetCategory={activeCat ?? ""}
                ink={ink}
                mute={mute}
                border={border}
                cardBg={cardBg}
                dark={dark}
                onCancel={() => setAdding(false)}
                onSubmit={(name, text, category) =>
                  onSave({ name, text, category }, () => setAdding(false))
                }
              />
            )}

            {/* 分组列表 */}
            {visibleCount === 0 ? (
              <div style={{ color: mute, fontSize: 12.5, textAlign: "center", padding: "40px 0" }}>
                {items.length === 0
                  ? "查重源为空，新查重将不剔除任何样板。"
                  : queryActive
                    ? "没有匹配的样板。"
                    : "该分类暂无样板。"}
              </div>
            ) : (
              groups.map(([cat, ts]) => {
                const isCollapsed = collapsed.has(cat) && !queryActive;
                const enN = ts.filter((t) => t.enabled).length;
                return (
                  <div key={cat} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                    {/* 分组头 */}
                    <div
                      onClick={() => toggleCollapse(cat)}
                      role="button"
                      tabIndex={0}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          toggleCollapse(cat);
                        }
                      }}
                      style={{ display: "flex", alignItems: "center", gap: 7, cursor: "pointer", padding: "2px 2px" }}
                    >
                      <Icon name={isCollapsed ? "chevR" : "chevD"} size={13} style={{ color: mute }} />
                      <Icon name="folder" size={13} style={{ color: accent }} />
                      <span style={{ fontSize: 12.5, fontWeight: 700, color: ink }}>{cat}</span>
                      <Pill fg={mute} bg={dark ? "rgba(255,255,255,0.06)" : C.paper2} size={10}>
                        {ts.length}
                        {enN < ts.length ? ` · 启用 ${enN}` : ""}
                      </Pill>
                    </div>
                    {/* 行 */}
                    {!isCollapsed &&
                      ts.map((t) =>
                        editingId === t.id ? (
                          <EditForm
                            key={t.id}
                            title="编辑样板"
                            cats={catNames}
                            initial={t}
                            ink={ink}
                            mute={mute}
                            border={border}
                            cardBg={cardBg}
                            dark={dark}
                            onCancel={() => setEditingId(null)}
                            onSubmit={(name, text, category) =>
                              onSave({ id: t.id, name, text, category }, () => setEditingId(null))
                            }
                          />
                        ) : (
                          <TemplateRow
                            key={t.id}
                            t={t}
                            query={query.trim()}
                            ink={ink}
                            mute={mute}
                            border={border}
                            cardBg={cardBg}
                            accent={accent}
                            dark={dark}
                            onEdit={() => setEditingId(t.id)}
                            onToggle={() =>
                              setEnabled.mutate(
                                { id: t.id, enabled: !t.enabled },
                                { onError: (e) => toast.show("操作失败：" + errMsg(e), "error") },
                              )
                            }
                            onDelete={() =>
                              delTpl.mutate(t.id, {
                                onError: (e) => toast.show("删除失败：" + errMsg(e), "error"),
                              })
                            }
                          />
                        ),
                      )}
                  </div>
                );
              })
            )}
          </div>
        </div>
      </div>

      {showBatch && (
        <BatchImportModal
          existingTexts={existingTexts}
          presetCategory={activeCat && activeCat !== UNCATEGORIZED ? activeCat : undefined}
          onClose={() => setShowBatch(false)}
        />
      )}
    </div>
  );
}

// —— 单条样板（查看态） ——
function TemplateRow({
  t,
  query,
  ink,
  mute,
  border,
  cardBg,
  accent,
  dark,
  onEdit,
  onToggle,
  onDelete,
}: {
  t: TemplateDto;
  query: string;
  ink: string;
  mute: string;
  border: string;
  cardBg: string;
  accent: string;
  dark: boolean;
  onEdit: () => void;
  onToggle: () => void;
  onDelete: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div
      style={{
        background: cardBg,
        border: `1px solid ${border}`,
        borderRadius: 12,
        padding: "13px 16px",
        opacity: t.enabled ? 1 : 0.55,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontSize: 12.5, fontWeight: 700, color: ink }}>{highlight(t.name, query, accent)}</span>
        {t.hitCount > 0 && (
          <Pill fg={accent} bg={`${accent}1A`} size={9.5}>
            命中 {t.hitCount} 份
          </Pill>
        )}
        {!t.enabled && (
          <Pill fg={mute} bg={dark ? "rgba(255,255,255,0.06)" : C.paper2} size={9.5}>
            已停用
          </Pill>
        )}
        <div style={{ flex: 1 }} />
        {confirming ? (
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span style={{ fontSize: 11, color: mute }}>删除？</span>
            <Button kind="ghost" size="sm" onClick={() => setConfirming(false)}>
              取消
            </Button>
            <Button kind="secondary" size="sm" onClick={onDelete}>
              删除
            </Button>
          </div>
        ) : (
          <>
            <Toggle on={t.enabled} onChange={onToggle} />
            <Icon name="wrench" size={13} style={{ color: mute, cursor: "pointer" }} onClick={onEdit} />
            <Icon name="x" size={13} style={{ color: mute, cursor: "pointer" }} onClick={() => setConfirming(true)} />
          </>
        )}
      </div>
      <div
        style={{
          fontSize: 12,
          color: mute,
          marginTop: 8,
          lineHeight: 1.75,
          display: "-webkit-box",
          WebkitLineClamp: 3,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
        }}
      >
        {highlight(t.text, query, accent)}
      </div>
    </div>
  );
}

// —— 新增/编辑表单（共用） ——
function EditForm({
  title,
  cats,
  initial,
  presetCategory,
  ink,
  mute,
  border,
  cardBg,
  dark,
  onCancel,
  onSubmit,
}: {
  title: string;
  cats: string[];
  initial?: TemplateDto;
  presetCategory?: string;
  ink: string;
  mute: string;
  border: string;
  cardBg: string;
  dark: boolean;
  onCancel: () => void;
  onSubmit: (name: string, text: string, category: string) => void;
}) {
  const [name, setName] = useState(initial?.name ?? "");
  const [text, setText] = useState(initial?.text ?? "");
  const [category, setCategory] = useState(
    initial?.category ?? (presetCategory && presetCategory !== UNCATEGORIZED ? presetCategory : ""),
  );
  const inputStyle: CSSProperties = {
    width: "100%",
    padding: "9px 12px",
    borderRadius: 8,
    border: `1px solid ${border}`,
    background: dark ? "rgba(255,255,255,0.04)" : C.paper,
    color: ink,
    fontSize: 12.5,
    fontFamily: C.font,
    outline: "none",
    userSelect: "text",
    boxSizing: "border-box",
  };
  const listId = useId();
  return (
    <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 16 }}>
      <div style={{ fontSize: 12.5, fontWeight: 700, color: ink, marginBottom: 10 }}>{title}</div>
      <datalist id={listId}>
        {cats.filter((c) => c !== UNCATEGORIZED).map((c) => (
          <option key={c} value={c} />
        ))}
      </datalist>
      <div style={{ display: "flex", gap: 10 }}>
        <input
          style={{ ...inputStyle, flex: 2 }}
          placeholder="样板名称，如「标准售后承诺」"
          value={name}
          onChange={(e) => setName(e.currentTarget.value)}
        />
        <input
          style={{ ...inputStyle, flex: 1 }}
          list={listId}
          placeholder="分类（可选/新建）"
          value={category}
          onChange={(e) => setCategory(e.currentTarget.value)}
        />
      </div>
      <textarea
        style={{ ...inputStyle, marginTop: 10, minHeight: 84, resize: "vertical", lineHeight: 1.7 }}
        placeholder="粘贴样板文本……"
        value={text}
        onChange={(e) => setText(e.currentTarget.value)}
      />
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 12 }}>
        <Button kind="ghost" size="md" onClick={onCancel}>
          取消
        </Button>
        <Button
          kind="primary"
          size="md"
          icon="check"
          disabled={!name.trim() || !text.trim()}
          onClick={() => onSubmit(name, text, category)}
        >
          保存
        </Button>
      </div>
      <div style={{ fontSize: 10.5, color: mute, marginTop: 8 }}>提示：分类可直接输入新名称即创建。</div>
    </div>
  );
}

// —— 分类筛选 Pill（可点击） ——
function FilterPill({
  label,
  active,
  accent,
  dark,
  onClick,
}: {
  label: string;
  active: boolean;
  accent: string;
  dark: boolean;
  onClick: () => void;
}) {
  return (
    <span
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      style={{
        cursor: "pointer",
        fontSize: 11.5,
        fontWeight: active ? 700 : 500,
        padding: "3px 11px",
        borderRadius: 999,
        color: active ? "#fff" : dark ? "rgba(255,255,255,0.7)" : C.ink2,
        background: active ? accent : dark ? "rgba(255,255,255,0.05)" : C.paper2,
        border: `1px solid ${active ? accent : dark ? "rgba(255,255,255,0.08)" : C.line}`,
        userSelect: "none",
      }}
    >
      {label}
    </span>
  );
}
