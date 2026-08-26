fn main() {
    // 构建 SHA 注入：支持工单里「用户报告对不上代码版本」的定位（About 面板展示）。
    // 非 git 环境（源码包构建）取不到即空——消费端 option_env! 回落 "unknown"，不 fail 构建。
    if let Ok(out) = std::process::Command::new("git").args(["rev-parse", "--short=12", "HEAD"]).output() {
        if out.status.success() {
            if let Ok(sha) = String::from_utf8(out.stdout) {
                println!("cargo:rustc-env=BIDGUARD_BUILD_SHA={}", sha.trim());
            }
        }
    }
    // HEAD 变化（切分支/新提交）时重跑本脚本，避免 sha 粘在旧值上
    println!("cargo:rerun-if-changed=../.git/HEAD");
    tauri_build::build()
}
