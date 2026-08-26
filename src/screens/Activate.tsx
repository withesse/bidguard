// 屏 · 授权激活（形态 A：离线签名许可）。首启即本地试用；到期/用尽后路由守卫拦到此页。
// 流程：复制本机机器码 → 发运营签发 .lic → 粘贴文本或选择文件 → 激活（Rust 层验签+绑定）。
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { C } from "../design/tokens";
import { Logo } from "../design/Icon";
import { Button } from "../components/primitives";
import { useTheme } from "../theme";
import { useToast } from "../components/Toast";
import { errMsg } from "../api/client";
import { useLicenseStatus, useImportLicense } from "../queries/data";
import type { LicenseStatusDto } from "../api/types";

function daysUntil(iso: string | null): number | null {
  if (!iso) return null;
  const ms = new Date(iso).getTime() - Date.now();
  return ms <= 0 ? 0 : Math.ceil(ms / 86_400_000);
}

function stateUi(s: string): { label: string; fg: string; bg: string } {
  switch (s) {
    case "licensed":
      return { label: "已授权", fg: C.ok, bg: C.okSoft };
    case "trial":
      return { label: "试用中", fg: C.warn, bg: C.warnSoft };
    case "grace":
      return { label: "宽限期", fg: C.warn, bg: C.warnSoft };
    case "exhausted":
      return { label: "次数已用尽", fg: C.danger, bg: C.dangerSoft };
    case "expired":
      return { label: "已到期", fg: C.danger, bg: C.dangerSoft };
    case "machineMismatch":
      return { label: "未绑定本机", fg: C.danger, bg: C.dangerSoft };
    default:
      return { label: "未激活", fg: C.danger, bg: C.dangerSoft };
  }
}

function summarize(st: LicenseStatusDto): string {
  if (st.state === "trial") {
    const d = daysUntil(st.trialExpiresAt);
    const parts = [];
    if (st.remainingUses != null) parts.push(`剩余 ${st.remainingUses} 次`);
    // 时钟自首次比对起算：未起算时没有到期日
    if (d != null) parts.push(`${d} 天后到期`);
    else parts.push("首次比对时开始计时");
    return `免费试用 · ${parts.join(" · ")}`;
  }
  if (st.state === "licensed" || st.state === "grace") {
    const parts = [];
    if (st.licenseeName) parts.push(st.licenseeName);
    if (st.remainingUses != null) parts.push(`剩余 ${st.remainingUses} 次`);
    else parts.push("不限次数");
    if (st.expiresAt) parts.push(`${daysUntil(st.expiresAt)} 天后到期`);
    return parts.join(" · ");
  }
  return st.message ?? "请激活后使用";
}

export function Activate() {
  const { dark, accent } = useTheme();
  const nav = useNavigate();
  const toast = useToast();
  const { data: st, isLoading } = useLicenseStatus();
  const importMut = useImportLicense();
  const [pasted, setPasted] = useState("");

  const ink = dark ? "#fff" : C.ink;
  const mute = dark ? "rgba(255,255,255,0.55)" : C.ink3;
  const bg = dark ? "#15151B" : C.paper;
  const cardBg = dark ? "rgba(255,255,255,0.04)" : C.white;
  const border = dark ? "rgba(255,255,255,0.08)" : C.line;
  const codeBg = dark ? "rgba(255,255,255,0.06)" : C.paper2;

  const copyCode = async () => {
    if (!st?.machineCode) return;
    try {
      await navigator.clipboard.writeText(st.machineCode);
      toast.show("机器码已复制", "success");
    } catch {
      toast.show("复制失败，请手动选择文本", "error");
    }
  };

  const doImport = (input: string) => {
    importMut.mutate(input, {
      onSuccess: (next) => {
        if (next.state === "licensed" || next.state === "grace") {
          toast.show("激活成功", "success");
          nav("/");
        } else {
          toast.show("已导入，但当前不可用：" + (next.message ?? next.state), "error");
        }
      },
      onError: (e) => toast.show("激活失败：" + errMsg(e), "error"),
    });
  };

  const pickFile = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const sel = await open({
        multiple: false,
        filters: [{ name: "许可文件", extensions: ["lic", "txt"] }],
      });
      if (typeof sel === "string") doImport(sel);
    } catch (e) {
      toast.show("读取许可文件失败：" + errMsg(e), "error");
    }
  };

  const ui = st ? stateUi(st.state) : null;

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: bg, overflow: "auto" }}>
      <div style={{ maxWidth: 560, margin: "0 auto", padding: "56px 24px 48px", width: "100%" }}>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, marginBottom: 28 }}>
          <Logo size={40} color={accent} />
          <div style={{ fontSize: 18, fontWeight: 700, color: ink, letterSpacing: "-0.01em" }}>激活原本 · 标书查重</div>
          <div style={{ fontSize: 12.5, color: mute }}>离线授权 · 全程不联网</div>
        </div>

        {/* 当前状态 */}
        {st && ui && (
          <div
            style={{
              background: cardBg,
              border: `1px solid ${border}`,
              borderRadius: 12,
              padding: "16px 18px",
              marginBottom: 18,
              display: "flex",
              alignItems: "center",
              gap: 12,
            }}
          >
            <span
              style={{
                fontSize: 11,
                fontWeight: 700,
                color: ui.fg,
                background: ui.bg,
                padding: "3px 9px",
                borderRadius: 999,
                whiteSpace: "nowrap",
              }}
            >
              {ui.label}
            </span>
            <span style={{ fontSize: 12.5, color: ink, flex: 1 }}>{summarize(st)}</span>
            {st.active && (
              <Button kind="secondary" size="sm" onClick={() => nav("/")}>
                进入应用
              </Button>
            )}
          </div>
        )}

        {st?.clockTamper && (
          <div
            style={{
              fontSize: 11.5,
              color: C.warn,
              background: C.warnSoft,
              border: `1px solid ${C.warn}33`,
              borderRadius: 8,
              padding: "8px 12px",
              marginBottom: 18,
            }}
          >
            检测到系统时间异常，请校正系统时钟后重试。
          </div>
        )}

        {st?.tamper && (
          <div
            style={{
              fontSize: 11.5,
              color: C.danger,
              background: C.dangerSoft,
              border: `1px solid ${C.danger}33`,
              borderRadius: 8,
              padding: "8px 12px",
              marginBottom: 18,
            }}
          >
            授权状态文件校验异常（曾被修改、删除或自他机复制），既往用量已按本机审计记录从严恢复。如属误判（如恢复系统备份后触发），请联系支持处理。
          </div>
        )}

        {/* 机器码 */}
        <div style={{ marginBottom: 18 }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: mute, marginBottom: 8 }}>
            第一步 · 复制本机机器码，发送给供应商申请许可
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "stretch",
              gap: 8,
            }}
          >
            <div
              style={{
                flex: 1,
                fontFamily: C.mono,
                fontSize: 11.5,
                color: ink,
                background: codeBg,
                border: `1px solid ${border}`,
                borderRadius: 8,
                padding: "10px 12px",
                wordBreak: "break-all",
                lineHeight: 1.5,
                userSelect: "all",
                maxHeight: 96,
                overflow: "auto",
              }}
            >
              {isLoading ? "读取中…" : st?.machineCode ?? "—"}
            </div>
            <Button kind="secondary" size="md" icon="paperclip" onClick={copyCode} style={{ alignSelf: "flex-start" }}>
              复制
            </Button>
          </div>
        </div>

        {/* 导入许可 */}
        <div style={{ marginBottom: 18 }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: mute, marginBottom: 8 }}>
            第二步 · 收到许可后，粘贴内容或选择 .lic 文件激活
          </div>
          <textarea
            value={pasted}
            onChange={(e) => setPasted(e.target.value)}
            placeholder={"-----BEGIN BIDGUARD LICENSE-----\n…\n-----END BIDGUARD LICENSE-----"}
            spellCheck={false}
            style={{
              width: "100%",
              boxSizing: "border-box",
              height: 96,
              fontFamily: C.mono,
              fontSize: 11,
              color: ink,
              background: cardBg,
              border: `1px solid ${border}`,
              borderRadius: 8,
              padding: "10px 12px",
              resize: "vertical",
              lineHeight: 1.5,
            }}
          />
          <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
            <Button
              kind="primary"
              size="md"
              icon="check"
              disabled={!pasted.trim() || importMut.isPending}
              onClick={() => doImport(pasted.trim())}
            >
              {importMut.isPending ? "激活中…" : "粘贴激活"}
            </Button>
            <Button kind="secondary" size="md" icon="folder" disabled={importMut.isPending} onClick={pickFile}>
              选择许可文件
            </Button>
          </div>
        </div>

        {/* 隐私声明（PIPL） */}
        <div
          style={{
            fontSize: 11,
            color: mute,
            lineHeight: 1.7,
            background: dark ? "rgba(255,255,255,0.03)" : C.paper2,
            border: `1px solid ${border}`,
            borderRadius: 8,
            padding: "12px 14px",
          }}
        >
          <div style={{ fontWeight: 600, color: ink, marginBottom: 4 }}>关于隐私</div>
          机器码仅包含本机硬件标识的<strong>加盐哈希值</strong>（不可反推原始硬件信息），用于将许可绑定到本设备、防止一份授权多机共享。
          形态 A 全程离线：机器码由您自行复制发送，应用不联网、不上传任何数据。
        </div>
      </div>
    </div>
  );
}
