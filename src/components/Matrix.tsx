// 相似度矩阵可视化 —— 移植自 app-design/project/src/c/bid-a.jsx
import { C, severityColor } from "../design/tokens";
import { useTheme } from "../theme";

// 列表里的迷你矩阵缩略图
export function MiniMatrix({ m, size = 90 }: { m: number[][]; size?: number }) {
  const { dark } = useTheme();
  const n = m.length;
  const fallback = dark ? "rgba(255,255,255,0.08)" : C.paper3;
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: `repeat(${n}, 1fr)`,
        gap: 2,
        width: size,
        height: size,
        padding: 4,
        borderRadius: 6,
        background: dark ? "rgba(255,255,255,0.025)" : C.paper2,
      }}
    >
      {m.map((row, r) =>
        row.map((v, c) => (
          <div
            key={`${r}-${c}`}
            style={{
              aspectRatio: "1/1",
              borderRadius: 2,
              background: r === c ? (dark ? "rgba(255,255,255,0.10)" : C.ink5) : severityColor(v, fallback),
            }}
          />
        )),
      )}
    </div>
  );
}

// 检测中：边算边填的矩阵（null 表示该对尚未完成）
export function BuildingMatrix({
  m,
  tags = ["甲", "乙", "丙", "丁", "戊"],
}: {
  m: (number | null)[][];
  tags?: string[];
}) {
  const { dark } = useTheme();
  const ink = dark ? "#fff" : C.ink;
  const n = m.length;
  return (
    <div style={{ display: "grid", gridTemplateColumns: `32px repeat(${n}, 1fr)`, gap: 4 }}>
      <div />
      {tags.slice(0, n).map((t) => (
        <div
          key={t}
          style={{ textAlign: "center", fontSize: 11, fontWeight: 700, color: ink, fontFamily: C.serif }}
        >
          {t}
        </div>
      ))}
      {m.map((row, r) => (
        <div key={r} style={{ display: "contents" }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "flex-end",
              fontSize: 11,
              fontWeight: 700,
              color: ink,
              fontFamily: C.serif,
              paddingRight: 4,
            }}
          >
            {tags[r]}
          </div>
          {row.map((v, c) => {
            const diag = r === c;
            const computed = v != null;
            return (
              <div
                key={c}
                style={{
                  aspectRatio: "1.2 / 1",
                  borderRadius: 6,
                  background: diag
                    ? dark
                      ? "rgba(255,255,255,0.04)"
                      : C.paper2
                    : computed
                      ? severityColor(v as number, C.okSoft)
                      : dark
                        ? "rgba(255,255,255,0.02)"
                        : "#fff",
                  border: !computed && !diag ? `1px dashed ${dark ? "rgba(255,255,255,0.1)" : C.ink5}` : "none",
                  color: computed && (v as number) >= 0.7 ? "#fff" : ink,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 12,
                  fontWeight: 700,
                  fontFamily: C.mono,
                  animation: !computed && !diag ? "cpulse 1.8s ease-in-out infinite" : "none",
                }}
              >
                {diag ? "—" : computed ? `${((v as number) * 100).toFixed(0)}` : "·"}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
