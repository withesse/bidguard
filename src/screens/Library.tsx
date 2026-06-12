// 屏 · 查重源 —— 通用模板 / 基准库管理（落库：导入分块时标记命中样板）。
import { useState, type CSSProperties } from "react";
import { C } from "../design/tokens";
import { Icon } from "../design/Icon";
import { Topbar } from "../components/Topbar";
import { Button } from "../components/primitives";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import { useDeleteTemplate, useSaveTemplate, useTemplates } from "../queries/data";

export function Library() {
  const { dark, accent } = useTheme();
  const toast = useToast();
  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;

  const { data } = useTemplates();
  const items = data ?? [];
  const save = useSaveTemplate();
  const delTpl = useDeleteTemplate();
  const [name, setName] = useState("");
  const [text, setText] = useState("");

  const add = () => {
    if (!name.trim() || !text.trim()) return;
    save.mutate(
      { name: name.trim(), text: text.trim() },
      {
        onSuccess: () => {
          setName("");
          setText("");
        },
        onError: (e) => toast.show("保存失败：" + errMsg(e), "error"),
      },
    );
  };
  const remove = (id: string) =>
    delTpl.mutate(id, { onError: (e) => toast.show("删除失败：" + errMsg(e), "error") });

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
  };

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: bg, minWidth: 0 }}>
      <Topbar title="查重源" sub={`本地通用模板 / 基准库 · 共 ${items.length} 条`} />
      <div style={{ flex: 1, overflow: "auto", padding: "28px 48px 40px" }}>
        <div style={{ maxWidth: 760, margin: "0 auto", display: "flex", flexDirection: "column", gap: 16 }}>
          {/* 说明 */}
          <div
            style={{
              background: dark ? "rgba(79,88,168,0.10)" : `${accent}08`,
              border: `1px solid ${accent}33`,
              borderRadius: 12,
              padding: "14px 18px",
              display: "flex",
              gap: 12,
            }}
          >
            <Icon name="book" size={18} style={{ color: accent, marginTop: 2 }} />
            <div style={{ fontSize: 12.5, color: mute, lineHeight: 1.7 }}>
              这里维护行业通用样板（法律法规、资质目录、标准承诺等）。导入标书时，
              <b style={{ color: ink }}>与这些样板高度相似的段落会被标记为模板、不计入雷同与围标判定</b>
              ，从而减少误报。全部存储在你本地。修改模板后需重新导入文档才会生效。
            </div>
          </div>

          {/* 新增 */}
          <div style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: 18 }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: ink, marginBottom: 12 }}>新增模板</div>
            <input
              style={inputStyle}
              placeholder="模板名称，如「标准售后承诺」"
              value={name}
              onChange={(e) => setName(e.currentTarget.value)}
            />
            <textarea
              style={{ ...inputStyle, marginTop: 10, minHeight: 88, resize: "vertical", lineHeight: 1.7 }}
              placeholder="粘贴样板文本……"
              value={text}
              onChange={(e) => setText(e.currentTarget.value)}
            />
            <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 12 }}>
              <Button kind="primary" size="md" icon="plus" onClick={add}>
                添加到查重源
              </Button>
            </div>
          </div>

          {/* 列表 */}
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            {items.length === 0 ? (
              <div style={{ color: mute, fontSize: 12.5, textAlign: "center", padding: "30px 0" }}>
                查重源为空，新查重将不剔除任何样板。
              </div>
            ) : (
              items.map((t) => (
                <div
                  key={t.id}
                  style={{ background: cardBg, border: `1px solid ${border}`, borderRadius: 12, padding: "14px 18px" }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <Icon name="book" size={13} style={{ color: accent }} />
                    <span style={{ fontSize: 12.5, fontWeight: 700, color: ink, flex: 1 }}>{t.name}</span>
                    <Icon name="x" size={13} style={{ color: mute, cursor: "pointer" }} onClick={() => remove(t.id)} />
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
                    {t.text}
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
