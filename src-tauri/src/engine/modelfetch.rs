// 模型按需下载的共用落盘器（embed / rerank 共用）：流式拉 .tar → 每个所需文件先写 <name>.part
// 再 rename，避免半截文件被后续 local_model_dir 当成「就位」。
//
// 【sha256 校验】是 W6-2 顺带补齐的取证短板：此前自托管/HF 下载都没有完整性校验，被劫持的
// 归档或被截断的传输会静默变成「模型已就绪」，而模型是判读结论的上游——查重结论不可举证。
// spec 声明期望摘要时，摘要不符即整目录丢弃并报错（宁可不可用，不可用错的模型出结论）。
// 未声明摘要（如内网自建归档）则只保证原子落盘，行为与本模块引入前一致。
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

/// 边读边算 sha256 的读取适配器。摘要必须覆盖【整个归档字节流】，故解包后要把剩余字节排空
/// 到 EOF 再取摘要（tar 取完所需条目就可能停读，不排空会算出半截摘要、把正常包误判成损坏）。
struct HashingReader<R> {
    inner: R,
    hasher: Sha256,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

impl<R: Read> HashingReader<R> {
    fn new(inner: R) -> Self {
        Self { inner, hasher: Sha256::new() }
    }

    /// 排空剩余字节后返回整流的十六进制摘要。
    /// 排空必须【经由本适配器】读（copy(&mut self, …) 而非 copy(&mut self.inner, …)），
    /// 否则剩余字节绕过 hasher，摘要只覆盖前半段，正常包会被误判成损坏。
    fn finish(mut self) -> std::io::Result<String> {
        std::io::copy(&mut self, &mut std::io::sink())?;
        Ok(self.hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }
}

/// 摘要比对：十六进制大小写不敏感、忽略首尾空白。此处是完整性校验（防截断/篡改的粗筛），
/// 不是认证，故不需要常数时间比较。
pub fn digest_matches(expected: &str, got: &str) -> bool {
    expected.trim().eq_ignore_ascii_case(got.trim())
}

/// 从 .tar 字节流中解出 `wanted` 列出的文件到 `dest`，并校验整流 sha256。
/// 返回写入字节数。校验失败 → 删除 `dest` 整目录（该目录按 `<base>/<model_id>/` 独占）并报错。
fn extract_verified<R: Read>(
    reader: R,
    expect_sha256: Option<&str>,
    dest: &Path,
    wanted: &[&str],
) -> Result<u64, String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let want: HashSet<&str> = wanted.iter().copied().collect();
    let mut archive = tar::Archive::new(HashingReader::new(reader));
    let mut written = 0u64;
    {
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut e = entry.map_err(|e| e.to_string())?;
            let name = e
                .path()
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
            let Some(name) = name else { continue };
            if !want.contains(name.as_str()) {
                continue;
            }
            let part = dest.join(format!("{name}.part"));
            let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
            written += std::io::copy(&mut e, &mut out).map_err(|e| e.to_string())?;
            std::fs::rename(&part, dest.join(&name)).map_err(|e| e.to_string())?;
        }
    }
    let digest = archive.into_inner().finish().map_err(|e| e.to_string())?;
    if let Some(exp) = expect_sha256 {
        if !digest_matches(exp, &digest) {
            let _ = std::fs::remove_dir_all(dest);
            return Err(format!(
                "模型归档校验失败：sha256 期望 {exp}，实得 {digest}；已丢弃本次下载内容"
            ));
        }
    }
    Ok(written)
}

/// 下载 `url` 指向的 .tar，解出 `wanted` 到 `dest`，按 `expect_sha256` 校验完整性。
/// 由工具屏显式发起 = 用户授权联网（比对路径一律不隐式下载）。
pub fn fetch_tar_into(
    url: &str,
    expect_sha256: Option<&str>,
    dest: &Path,
    wanted: &[&str],
) -> Result<u64, String> {
    let resp = ureq::get(url).call().map_err(|e| format!("下载失败：{e}"))?;
    extract_verified(resp.into_body().into_reader(), expect_sha256, dest, wanted)
}

/// 下载 .tar 并解出其中【第一个】.onnx 到 `dest` 文件（OCR 档位专用：det/rec 归档内条目
/// 同名 inference.onnx，按名匹配会撞名，故按扩展名取首个并落为目标文件名）。
/// 与 fetch_tar_into 的两点差异：dest 所在缓存目录为多档位共享，校验失败只删本次 .part、
/// 不清目录；rename 发生在【整流摘要通过之后】——失败时磁盘上不会出现「看似就位」的目标文件。
pub fn fetch_tar_first_onnx(
    url: &str,
    expect_sha256: Option<&str>,
    dest: &Path,
) -> Result<u64, String> {
    let resp = ureq::get(url).call().map_err(|e| format!("下载失败：{e}"))?;
    extract_first_onnx_verified(resp.into_body().into_reader(), expect_sha256, dest)
}

fn extract_first_onnx_verified<R: Read>(
    reader: R,
    expect_sha256: Option<&str>,
    dest: &Path,
) -> Result<u64, String> {
    let part = dest.with_extension("part");
    let mut archive = tar::Archive::new(HashingReader::new(reader));
    let mut written: Option<u64> = None;
    {
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut e = entry.map_err(|e| e.to_string())?;
            let is_onnx = e
                .path()
                .ok()
                .and_then(|p| p.extension().map(|x| x.eq_ignore_ascii_case("onnx")))
                .unwrap_or(false);
            if is_onnx {
                let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
                written = Some(std::io::copy(&mut e, &mut out).map_err(|e| e.to_string())?);
                break; // 剩余字节由 finish() 排空计入摘要
            }
        }
    }
    let digest = archive.into_inner().finish().map_err(|e| e.to_string())?;
    let Some(n) = written else {
        let _ = std::fs::remove_file(&part);
        return Err("tar 包内未找到 .onnx".to_string());
    };
    if let Some(exp) = expect_sha256 {
        if !digest_matches(exp, &digest) {
            let _ = std::fs::remove_file(&part);
            return Err(format!(
                "模型归档校验失败：sha256 期望 {exp}，实得 {digest}；已丢弃本次下载内容"
            ));
        }
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tar_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        for (name, data) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, *data).unwrap();
        }
        b.into_inner().unwrap()
    }

    fn sha256_hex(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn digest_matches_is_case_and_space_insensitive() {
        assert!(digest_matches("ABCD", "abcd"));
        assert!(digest_matches("  abcd\n", "abcd"));
        assert!(!digest_matches("abcd", "abce"));
    }

    // 摘要必须覆盖整个归档：所需文件取完后 tar 可能没读到 EOF，
    // 排空剩余字节前取摘要会得到半截值 → 正常包被误判为损坏。
    #[test]
    fn extract_verified_accepts_matching_digest() {
        let root = std::env::temp_dir().join(format!("bg_mf_ok_{}", uuid::Uuid::new_v4()));
        let bytes = tar_bytes(&[("model.onnx", b"onnx-bytes"), ("junk.txt", b"ignored")]);
        let n = extract_verified(
            &bytes[..],
            Some(&sha256_hex(&bytes)),
            &root,
            &["model.onnx"],
        )
        .expect("摘要一致应接受");
        assert_eq!(n, "onnx-bytes".len() as u64);
        assert_eq!(std::fs::read(root.join("model.onnx")).unwrap(), b"onnx-bytes");
        assert!(!root.join("junk.txt").exists(), "未列入 wanted 的条目不落盘");
        assert!(!root.join("model.onnx.part").exists(), "临时 .part 必须已 rename");
        let _ = std::fs::remove_dir_all(&root);
    }

    // 摘要不符即拒收：目标目录整体丢弃，绝不留下「看起来就位」的半套文件。
    #[test]
    fn extract_verified_rejects_wrong_digest_and_wipes_dir() {
        let root = std::env::temp_dir().join(format!("bg_mf_bad_{}", uuid::Uuid::new_v4()));
        let bytes = tar_bytes(&[("model.onnx", b"onnx-bytes")]);
        let err = extract_verified(&bytes[..], Some(&"0".repeat(64)), &root, &["model.onnx"])
            .expect_err("摘要不符必须拒收");
        assert!(err.contains("sha256"), "错误信息应点明校验失败：{err}");
        assert!(!root.exists(), "校验失败后目标目录必须清空");
    }

    // 首个 .onnx 变体：改名发生在摘要通过之后；子目录内的 inference.onnx 也能按扩展名命中。
    #[test]
    fn first_onnx_renames_only_after_digest_ok() {
        let root = std::env::temp_dir().join(format!("bg_mf_onnx_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let bytes = tar_bytes(&[("PP-OCRv6_x_det_onnx_infer/inference.onnx", b"onnx-bytes"), ("x/inference.yml", b"cfg")]);
        let dest = root.join("pp-ocrv6_x_det.onnx");
        let n = extract_first_onnx_verified(&bytes[..], Some(&sha256_hex(&bytes)), &dest)
            .expect("摘要一致应接受");
        assert_eq!(n, "onnx-bytes".len() as u64);
        assert_eq!(std::fs::read(&dest).unwrap(), b"onnx-bytes");
        assert!(!dest.with_extension("part").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn first_onnx_rejects_wrong_digest_without_touching_dest() {
        let root = std::env::temp_dir().join(format!("bg_mf_onnxbad_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let bytes = tar_bytes(&[("a/inference.onnx", b"onnx-bytes")]);
        let dest = root.join("target.onnx");
        let err = extract_first_onnx_verified(&bytes[..], Some(&"0".repeat(64)), &dest)
            .expect_err("摘要不符必须拒收");
        assert!(err.contains("sha256"), "{err}");
        assert!(!dest.exists(), "校验失败后目标文件不得存在");
        assert!(!dest.with_extension("part").exists(), "半成品 .part 必须清除");
        let _ = std::fs::remove_dir_all(&root);
    }

    // 未声明期望摘要（内网自建归档）时行为不变：正常落盘，不因缺摘要而拒绝。
    #[test]
    fn extract_verified_without_expectation_still_writes() {
        let root = std::env::temp_dir().join(format!("bg_mf_none_{}", uuid::Uuid::new_v4()));
        let bytes = tar_bytes(&[("tokenizer.json", b"{}")]);
        extract_verified(&bytes[..], None, &root, &["tokenizer.json"]).expect("无期望摘要应放行");
        assert_eq!(std::fs::read(root.join("tokenizer.json")).unwrap(), b"{}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
