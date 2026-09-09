use anyhow::{Result, bail};

use crate::persist as secret;
use crate::persist::{Config, Store};

use super::args::SecretCommand;
use super::common::{read_hidden_line, select_project};

/// 从 argv 提取 askpass prompt。兼容两种调用：
/// - OpenSSH：`pcs.exe <prompt>`（prompt 可能含空格，已由系统按单参传入）
/// - 手工：`pcs.exe __askpass <prompt>`
pub(crate) fn askpass_prompt_from_args() -> String {
    askpass_prompt_from(std::env::args().skip(1))
}

/// 纯解析：供单元测试直接喂入参数列表，避免与生产逻辑分叉。
fn askpass_prompt_from(mut args: impl Iterator<Item = String>) -> String {
    match args.next() {
        Some(first) if first == "__askpass" => args.next().unwrap_or_default(),
        Some(first) => first,
        None => String::new(),
    }
}

pub(crate) fn cmd_secret(command: SecretCommand) -> Result<()> {
    let SecretCommand::Show(selector) = command;
    let data = Store::load();
    let (group_index, project_index) =
        select_project(&data, &selector.name, selector.group.as_deref())?;
    let project = &data.groups[group_index].projects[project_index];
    if !project.is_ssh_project() {
        bail!("项目 `{}` 不是 SSH 项目", project.name);
    }
    if project.ssh_password_enc.is_empty() && project.ssh_key_pass_enc.is_empty() {
        bail!("项目 `{}` 未保存密码或私钥口令", project.name);
    }
    let config = Config::load();
    let Some(record) = &config.pin else {
        bail!("尚未设置 PIN，先 `pcs pin set`");
    };
    let mut verified = false;
    for attempt in 1..=3 {
        let pin = read_hidden_line("PIN: ")?;
        if secret::verify_pin(&pin, record).map_err(anyhow::Error::msg)? {
            verified = true;
            break;
        }
        if attempt < 3 {
            eprintln!("PIN 不正确，还剩 {} 次机会。", 3 - attempt);
        }
    }
    if !verified {
        bail!("PIN 验证失败");
    }
    println!("项目: {}", project.name);
    if !project.ssh_password_enc.is_empty() {
        match secret::unprotect(&project.ssh_password_enc) {
            Ok(pw) => println!("登录密码: {pw}"),
            Err(e) => println!("登录密码: <解密失败: {e}>"),
        }
    }
    if !project.ssh_key_pass_enc.is_empty() {
        match secret::unprotect(&project.ssh_key_pass_enc) {
            Ok(kp) => println!("私钥口令: {kp}"),
            Err(e) => println!("私钥口令: <解密失败: {e}>"),
        }
    }
    Ok(())
}

/// askpass prompt 分派：只回答密码与私钥口令，其余（host key yes/no 等）忽略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AskpassKind {
    Password,
    KeyPass,
    Ignore,
}

pub(crate) fn askpass_kind(prompt: &str) -> AskpassKind {
    let lower = prompt.to_lowercase();
    if lower.contains("password") || prompt.contains("密码") {
        AskpassKind::Password
    } else if lower.contains("passphrase") {
        AskpassKind::KeyPass
    } else {
        AskpassKind::Ignore
    }
}

/// askpass token 校验：env 中 token 必须与父进程落在
/// `PCS_ASKPASS_TOKEN_FILE` 的内容一致（64 位 hex）。
/// 缺失/不匹配一律拒绝（设计 §3.5：防「直接敲一行 `pcs __askpass` 拿明文」；
/// 诚实边界：同 Windows 用户进程可读取文件内容，本就在 DPAPI 信任边界内）。
pub(crate) fn askpass_token_ok(token: &str, token_file: &str) -> bool {
    let token = token.trim();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    let path = token_file.trim();
    if path.is_empty() {
        return false;
    }
    match std::fs::read_to_string(path) {
        Ok(expected) => expected.trim().eq_ignore_ascii_case(token),
        Err(_) => false,
    }
}

/// SSH_ASKPASS 回调：校验 token -> 按 prompt 分派 -> 解密 -> stdout。
/// 只读加载（不写回、不维护），避免与父进程的 projects.json 写竞态。
/// 非密码/口令类 prompt（host key 的 yes/no 等）一律输出空，绝不代答。
pub(crate) fn cmd_askpass(prompt: &str) -> Result<()> {
    let token = std::env::var("PCS_ASKPASS_TOKEN").unwrap_or_default();
    let token_file = std::env::var("PCS_ASKPASS_TOKEN_FILE").unwrap_or_default();
    if !askpass_token_ok(&token, &token_file) {
        // token 缺失/不匹配：拒绝输出。
        return Ok(());
    }
    let id = std::env::var("PCS_ASKPASS_ID").unwrap_or_default();
    let data = Store::load_readonly();
    let Some(project) = data
        .groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .find(|p| p.id.eq_ignore_ascii_case(&id))
    else {
        return Ok(());
    };
    let enc = match askpass_kind(prompt) {
        AskpassKind::Password => &project.ssh_password_enc,
        AskpassKind::KeyPass => &project.ssh_key_pass_enc,
        AskpassKind::Ignore => return Ok(()),
    };
    if enc.trim().is_empty() {
        return Ok(());
    }
    if let Ok(plain) = secret::unprotect(enc) {
        println!("{plain}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn askpass_prompt_from_args_shapes() {
        // 直接测生产解析：OpenSSH 单参 / 手工 __askpass / 空参。
        let from = |args: &[&str]| -> String {
            askpass_prompt_from(args.iter().map(|s| (*s).to_string()))
        };
        assert_eq!(
            from(&["abc@172.16.14.10's password: "]),
            "abc@172.16.14.10's password: "
        );
        assert_eq!(
            from(&["__askpass", "Enter passphrase for key:"]),
            "Enter passphrase for key:"
        );
        assert_eq!(from(&["__askpass"]), "");
        assert_eq!(from(&[]), "");
    }

    #[test]
    fn askpass_kind_dispatch() {
        assert_eq!(
            askpass_kind("abc@172.16.14.10's password: "),
            AskpassKind::Password
        );
        assert_eq!(askpass_kind("请输入密码："), AskpassKind::Password);
        assert_eq!(
            askpass_kind("Enter passphrase for key 'C:\\keys\\id_ed25519': "),
            AskpassKind::KeyPass
        );
        // host key 的 yes/no 等绝不代答
        assert_eq!(
            askpass_kind("Are you sure you want to continue connecting (yes/no/[fingerprint])? "),
            AskpassKind::Ignore
        );
        assert_eq!(askpass_kind("Verification code: "), AskpassKind::Ignore);
    }

    fn temp_token_file(content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pcs_askpass_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("askpass.token");
        std::fs::write(&path, content).unwrap();
        path
    }

    fn cleanup_token_file(path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn askpass_token_ok_requires_matching_file() {
        let token = "a1B2c3D4e5F6a1B2c3D4e5F6a1B2c3D4e5F6a1B2c3D4e5F6a1B2c3D4e5F6a1B2";
        let path = temp_token_file(token);
        // 匹配（大小写不敏感）
        assert!(askpass_token_ok(token, &path.to_string_lossy()));
        assert!(askpass_token_ok(
            &token.to_uppercase(),
            &path.to_string_lossy()
        ));
        // 不匹配 / 缺失 / 空
        assert!(!askpass_token_ok("ff", &path.to_string_lossy()));
        assert!(!askpass_token_ok("", &path.to_string_lossy()));
        assert!(!askpass_token_ok(token, ""));
        assert!(!askpass_token_ok(token, "Z:\\no\\such\\file.token"));
        // env 伪造 token 但文件缺失 -> 拒绝
        assert!(!askpass_token_ok(token, "  "));
        cleanup_token_file(&path);
        // 文件被删除后（ssh 已退出的正常清理）即拒绝
        assert!(!askpass_token_ok(token, &path.to_string_lossy()));
    }

    #[test]
    fn askpass_token_ok_rejects_malformed_token() {
        let path = temp_token_file("x");
        // 非 hex / 长度不对的 token 一律拒绝，无论文件内容
        assert!(!askpass_token_ok("xyz", &path.to_string_lossy()));
        assert!(!askpass_token_ok("ff", &path.to_string_lossy()));
        cleanup_token_file(&path);
    }
}
