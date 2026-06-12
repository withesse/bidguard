// 新通路 invoke 包装：统一把 Rust AppError（{code,message,detail}）还原为类型化错误。
import { invoke } from "@tauri-apps/api/core";

export interface ApiError {
  code: string;
  message: string;
  detail?: string;
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function asApiError(e: unknown): ApiError {
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    return e as ApiError;
  }
  return { code: "unknown", message: String(e) };
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw { code: "unknown", message: "桌面功能仅在应用内可用" } satisfies ApiError;
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw asApiError(e);
  }
}

/** 面向用户的错误文案（详情供「复制反馈」场景，不默认展示）。 */
export function errMsg(e: unknown): string {
  return asApiError(e).message;
}
