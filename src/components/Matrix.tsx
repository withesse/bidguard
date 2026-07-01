// 相似度矩阵可视化 —— 移植自 app-design/project/src/c/bid-a.jsx
import { C, severityColor } from "../design/tokens";
import { useTheme } from "../theme";

// 列表里的迷你矩阵缩略图
export function MiniMatrix({ m, size = 90 }: { m: number[][]; size?: number }) {
  const { dark } = useTheme();
  const n = m.length;
  const fallback = dark ? "rgba(255,255,255,0.08)" : C.paper3;
  // 空/非方阵守卫：n=0 会产生非法 grid 模板 repeat(0,1fr)
  if (n === 0 || m.some((row) => row.length !== n)) return null;
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
