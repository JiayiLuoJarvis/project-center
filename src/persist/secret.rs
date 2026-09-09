use std::path::{Path, PathBuf};

use zeroize::Zeroize;

use crate::domain::models::ProjectData;

/// base64 标准编码（无依赖；秘密字段量小，性能无关紧要）。
pub fn b64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18 & 63) as usize] as char);
        out.push(TABLE[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// base64 标准解码；非法输入返回 None。
pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = text.trim().as_bytes();
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|&&c| c == b'=').count();
        if pad > 2 || chunk[..4 - pad].contains(&b'=') {
            return None;
        }
        let mut n = 0u32;
        for &c in &chunk[..4 - pad] {
            n = (n << 6) | val(c)?;
        }
        n <<= 6 * pad as u32;
        out.extend_from_slice(&n.to_be_bytes()[1..4][..3 - pad]);
    }
    Some(out)
}

/// DPAPI 加密（CryptProtectData），返回 base64 密文。
#[cfg(windows)]
pub fn protect(plain: &str) -> Result<String, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::CryptProtectData;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    let mut plain_bytes = plain.as_bytes().to_vec();
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain_bytes.len() as u32,
        pbData: plain_bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    plain_bytes.zeroize();
    if ok == 0 {
        return Err("DPAPI 加密失败".into());
    }
    let encrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(b64_encode(&encrypted))
}

/// DPAPI 解密（CryptUnprotectData），输入 base64 密文。
#[cfg(windows)]
pub fn unprotect(enc_b64: &str) -> Result<String, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::CryptUnprotectData;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    let mut encrypted = b64_decode(enc_b64).ok_or("密文不是合法 base64")?;
    if encrypted.is_empty() {
        return Err("密文为空".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len() as u32,
        pbData: encrypted.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    encrypted.zeroize();
    if ok == 0 {
        return Err("DPAPI 解密失败（密文损坏或来自其他用户/机器）".into());
    }
    let mut plain =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData.cast()) };
    let result = String::from_utf8(plain.clone()).map_err(|_| "解密结果不是合法 UTF-8".to_string());
    plain.zeroize();
    result
}

#[cfg(not(windows))]
pub fn protect(_plain: &str) -> Result<String, String> {
    Err("仅支持 Windows".into())
}

#[cfg(not(windows))]
pub fn unprotect(_enc_b64: &str) -> Result<String, String> {
    Err("仅支持 Windows".into())
}

/// 填充密码学随机字节（BCryptGenRandom）。
#[cfg(windows)]
pub fn fill_random(buf: &mut [u8]) -> Result<(), String> {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        return Err(format!("BCryptGenRandom 失败: 0x{status:08X}"));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn fill_random(_buf: &mut [u8]) -> Result<(), String> {
    Err("仅支持 Windows".into())
}

/// PBKDF2-HMAC-SHA256（BCryptDeriveKeyPBKDF2，Windows 自带）。
#[cfg(windows)]
fn pbkdf2_sha256(pin: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) -> Result<(), String> {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_ALG_HANDLE, BCRYPT_ALG_HANDLE_HMAC_FLAG, BCryptCloseAlgorithmProvider,
        BCryptDeriveKeyPBKDF2, BCryptOpenAlgorithmProvider,
    };

    const SHA256: &[u16] = &[
        b'S' as u16,
        b'H' as u16,
        b'A' as u16,
        b'2' as u16,
        b'5' as u16,
        b'6' as u16,
        0,
    ];
    unsafe {
        let mut alg: BCRYPT_ALG_HANDLE = std::ptr::null_mut();
        // PBKDF2 的 PRF 是 HMAC-SHA256，必须带 HMAC flag 打开。
        let status = BCryptOpenAlgorithmProvider(
            &mut alg,
            SHA256.as_ptr(),
            std::ptr::null(),
            BCRYPT_ALG_HANDLE_HMAC_FLAG,
        );
        if status != 0 {
            return Err(format!("BCryptOpenAlgorithmProvider 失败: 0x{status:08X}"));
        }
        let status = BCryptDeriveKeyPBKDF2(
            alg,
            pin.as_ptr(),
            pin.len() as u32,
            salt.as_ptr(),
            salt.len() as u32,
            iterations as u64,
            out.as_mut_ptr(),
            out.len() as u32,
            0,
        );
        BCryptCloseAlgorithmProvider(alg, 0);
        if status != 0 {
            return Err(format!("BCryptDeriveKeyPBKDF2 失败: 0x{status:08X}"));
        }
    }
    Ok(())
}

/// PIN 校验记录：只存 PBKDF2 哈希，绝不存 PIN 本身。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PinRecord {
    /// base64 编码的随机盐。
    pub salt: String,
    /// PBKDF2 迭代次数。
    pub iterations: u32,
    /// base64 编码的 32 字节派生哈希。
    pub hash: String,
}

pub const PIN_ITERATIONS: u32 = 600_000;
/// `verify_pin` 接受的最大迭代数：config.json 可被手工篡改成超大值导致每次
/// 校验挂死（本机 DoS），钳制上限；超过上限的记录校验必败（视为损坏记录）。
const MAX_VERIFY_ITERATIONS: u32 = 2_000_000;
const SALT_LEN: usize = 16;
const HASH_LEN: usize = 32;

/// 生成新的 PIN 校验记录（随机盐 + PBKDF2）。
pub fn pin_record_from(pin: &str) -> Result<PinRecord, String> {
    let pin = pin.trim();
    if !pin.bytes().all(|b| b.is_ascii_digit()) || !(4..=12).contains(&pin.len()) {
        return Err("PIN 必须是 4-12 位数字".into());
    }
    let mut salt = [0u8; SALT_LEN];
    fill_random(&mut salt)?;
    let mut hash = [0u8; HASH_LEN];
    pbkdf2_sha256(pin.as_bytes(), &salt, PIN_ITERATIONS, &mut hash)?;
    let record = PinRecord {
        salt: b64_encode(&salt),
        iterations: PIN_ITERATIONS,
        hash: b64_encode(&hash),
    };
    hash.zeroize();
    Ok(record)
}

/// 验证 PIN 是否匹配记录（常数时间比较）。
pub fn verify_pin(pin: &str, record: &PinRecord) -> Result<bool, String> {
    let pin = pin.trim();
    let salt = b64_decode(&record.salt).ok_or("PIN 盐损坏")?;
    let expected = b64_decode(&record.hash).ok_or("PIN 哈希损坏")?;
    if expected.len() != HASH_LEN {
        return Err("PIN 哈希长度异常".into());
    }
    let mut actual = [0u8; HASH_LEN];
    pbkdf2_sha256(
        pin.as_bytes(),
        &salt,
        record.iterations.min(MAX_VERIFY_ITERATIONS),
        &mut actual,
    )?;
    let mut diff = 0u8;
    for (a, b) in actual.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    let ok = diff == 0;
    actual.zeroize();
    Ok(ok)
}

/// 数据根目录（projects.json 所在目录）。
pub fn data_root() -> PathBuf {
    crate::persist::Store::file_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// key 相对路径 -> 绝对路径（相对数据根解析）。
pub fn key_file_path_in(root: &Path, relative: &str) -> PathBuf {
    root.join(relative)
}

/// 原子写 DPAPI 加密的私钥 sidecar，返回相对路径（`keys/<uuid>.key`）。
pub fn write_key_file_in(root: &Path, project_id: &str, plain_key: &str) -> Result<String, String> {
    let id = project_id.trim();
    if id.is_empty() {
        return Err("项目缺少 id，无法保存密钥文件".into());
    }
    if plain_key.is_empty() {
        return Err("密钥内容为空".into());
    }
    let encrypted = protect(plain_key)?;
    let dir = root.join("keys");
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建密钥目录: {e}"))?;
    let tmp = dir.join(format!("{}.key.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, &encrypted).map_err(|e| format!("写入密钥临时文件失败: {e}"))?;
    let target = dir.join(format!("{id}.key"));
    // Windows 的 rename 不覆盖目标：先删旧文件。写失败不破坏旧文件由 tmp 中转保证。
    let _ = std::fs::remove_file(&target);
    std::fs::rename(&tmp, &target).map_err(|e| format!("密钥文件落位失败: {e}"))?;
    Ok(format!("keys/{id}.key"))
}

/// 读取并解密私钥 sidecar。
pub fn read_key_file_in(root: &Path, relative: &str) -> Result<String, String> {
    let path = key_file_path_in(root, relative);
    let encrypted = std::fs::read_to_string(&path).map_err(|e| format!("读取密钥文件失败: {e}"))?;
    unprotect(encrypted.trim())
}

/// 删除私钥 sidecar；不存在视为成功。失败返回 Err 供调用方记录延迟重试。
pub fn delete_key_file_in(root: &Path, relative: &str) -> Result<(), String> {
    if relative.trim().is_empty() {
        return Ok(());
    }
    match std::fs::remove_file(key_file_path_in(root, relative)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("删除密钥文件失败: {e}")),
    }
}

/// 生产路径包装（真实数据根）。
pub fn write_key_file(project_id: &str, plain_key: &str) -> Result<String, String> {
    write_key_file_in(&data_root(), project_id, plain_key)
}

pub fn read_key_file(relative: &str) -> Result<String, String> {
    read_key_file_in(&data_root(), relative)
}

pub fn delete_key_file(relative: &str) -> Result<(), String> {
    delete_key_file_in(&data_root(), relative)
}

/// 尽力覆写并删除文件（临时密钥清理用）；失败不报错（下次启动兜底）。
pub fn shred_and_remove(path: &Path) {
    if let Ok(mut data) = std::fs::read(path) {
        for b in data.iter_mut() {
            *b = 0;
        }
        let _ = std::fs::write(path, &data);
        data.zeroize();
    }
    let _ = std::fs::remove_file(path);
}

/// 收集 `data` 引用的全部 key 文件相对路径（groups + trash 快照）。
pub fn referenced_key_files(data: &ProjectData) -> Vec<String> {
    let mut refs = Vec::new();
    for project in data.groups.iter().flat_map(|g| &g.projects) {
        if !project.ssh_key_file.trim().is_empty() {
            refs.push(project.ssh_key_file.clone());
        }
    }
    for item in &data.trash {
        if !item.ssh_key_file.trim().is_empty() {
            refs.push(item.ssh_key_file.clone());
        }
        for project in &item.projects {
            if !project.ssh_key_file.trim().is_empty() {
                refs.push(project.ssh_key_file.clone());
            }
        }
    }
    refs
}

/// 收集 `keys\` 目录下的全部 key 相对路径。
fn existing_key_files_in(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root.join("keys")) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(".key").then_some(format!("keys/{name}"))
        })
        .collect()
}

/// `keys_tmp\` 会话文件保留阈值：存活会话（ssh 进行中）的临时密钥与
/// askpass token 校验文件可能被其他 pcs 进程的启动维护扫到，
/// 按修改时间跳过新于阈值的文件；崩溃残留超过阈值由下次维护兜底。
const KEYS_TMP_KEEP_SECS: u64 = 3600;

/// 覆写清空 `keys_tmp\` 中早于 `cutoff` 的残留文件
///（`modified > cutoff` 视为活跃会话文件，跳过）。
fn shred_keys_tmp_in(root: &Path, cutoff: std::time::SystemTime) {
    let Ok(entries) = std::fs::read_dir(root.join("keys_tmp")) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let recent = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|modified| modified > cutoff)
            .unwrap_or(false);
        if recent {
            continue;
        }
        shred_and_remove(&entry.path());
    }
}

/// 启动维护（幂等，每次数据加载后调用一次）：
/// 1. 重试 `pendingKeyDeletes`（删除失败延迟重试）；
/// 2. 以 JSON 引用为准清理 `keys\` 下孤儿 key 文件（仅 `allow_orphan_sweep` 时执行）；
/// 3. 覆写清空 `keys_tmp\` 残留与 `keys\*.key.tmp` 写失败残留。
///
/// `allow_orphan_sweep=false` 用于数据退化场景（JSON 损坏且无备份、备份恢复）：
/// 引用集不可信，绝不能清理，否则会把全部密文 sidecar 当孤儿销毁。
///
/// 返回数据是否被修改（`pendingKeyDeletes` 摘除）。
pub fn startup_maintenance_in(
    root: &Path,
    data: &mut ProjectData,
    allow_orphan_sweep: bool,
) -> bool {
    let mut changed = false;

    // 1. 重试待删除列表
    if !data.pending_key_deletes.is_empty() {
        data.pending_key_deletes
            .retain(|relative| delete_key_file_in(root, relative).is_err());
        changed = true;
    }

    // 2. 孤儿 key 清理（以 JSON 引用为准，不按 id 推导）
    if allow_orphan_sweep {
        let refs = referenced_key_files(data);
        for relative in existing_key_files_in(root) {
            if !refs.contains(&relative) {
                let _ = delete_key_file_in(root, &relative);
            }
        }
    }

    // 3. `keys\*.key.tmp` 写失败残留：永不被引用，无条件覆写删除
    if let Ok(entries) = std::fs::read_dir(root.join("keys")) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".key.tmp") {
                shred_and_remove(&entry.path());
            }
        }
    }

    // 4. keys_tmp 残留清理（跳过阈值内的活跃会话文件）
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(KEYS_TMP_KEEP_SECS);
    shred_keys_tmp_in(root, cutoff);

    changed
}

pub fn startup_maintenance(data: &mut ProjectData, allow_orphan_sweep: bool) -> bool {
    startup_maintenance_in(&data_root(), data, allow_orphan_sweep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{DeletedItem, Group, Project, ProjectData};

    fn temp_root() -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pcs_secret_test_{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ssh_project(relative: &str) -> Project {
        let mut p = Project::new("srv", "/opt/x", "");
        p.ssh_target = "abc@h".into();
        p.ssh_key_file = relative.into();
        p
    }

    #[test]
    fn b64_round_trip() {
        for len in 0..70usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 % 251) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&data)).unwrap(), data, "len={len}");
        }
        assert!(b64_decode("A").is_none());
        assert!(b64_decode("AB!C").is_none());
        assert_eq!(b64_decode("").unwrap(), Vec::<u8>::new());
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"foob"), "Zm9vYg==");
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_round_trip_and_reject() {
        let secret = "L6p^BwgHny.sVqFLn3RG 密码测试";
        let enc = protect(secret).unwrap();
        assert_ne!(enc, secret);
        assert_eq!(unprotect(&enc).unwrap(), secret);
        assert!(unprotect("not-base64!!").is_err());
        assert!(unprotect(&b64_encode(b"garbage-not-dpapi")).is_err());
        // 密文每次加密都不同（DPAPI 含随机成分）
        assert_ne!(enc, protect(secret).unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn pin_record_verify_and_reject() {
        let record = pin_record_from("246813").unwrap();
        assert_ne!(
            record.salt,
            pin_record_from("246813").unwrap().salt,
            "盐必须随机"
        );
        assert!(matches!(verify_pin("246813", &record), Ok(true)));
        assert!(matches!(verify_pin(" 246813 ", &record), Ok(true)));
        assert!(matches!(verify_pin("000000", &record), Ok(false)));
        assert!(pin_record_from("123").is_err(), "过短拒绝");
        assert!(pin_record_from("12a4").is_err(), "非数字拒绝");
        assert!(pin_record_from("").is_err());
        assert!(pin_record_from("1234567890123").is_err(), "过长拒绝");
    }

    #[cfg(windows)]
    #[test]
    fn key_file_round_trip_overwrite_and_delete() {
        let root = temp_root();
        let relative =
            write_key_file_in(&root, "id-1", "-----BEGIN OPENSSH PRIVATE KEY-----").unwrap();
        assert_eq!(relative, "keys/id-1.key");
        assert!(root.join(&relative).exists());
        assert_eq!(
            read_key_file_in(&root, &relative).unwrap(),
            "-----BEGIN OPENSSH PRIVATE KEY-----"
        );
        // 覆盖写入
        write_key_file_in(&root, "id-1", "new-key-content").unwrap();
        assert_eq!(
            read_key_file_in(&root, &relative).unwrap(),
            "new-key-content"
        );
        // 无 tmp 残留
        assert!(root.join("keys").read_dir().unwrap().count() == 1);
        // 删除（含幂等）
        assert!(delete_key_file_in(&root, &relative).is_ok());
        assert!(!root.join(&relative).exists());
        assert!(delete_key_file_in(&root, &relative).is_ok());
        // 空内容 / 空 id 拒绝
        assert!(write_key_file_in(&root, "id-1", "").is_err());
        assert!(write_key_file_in(&root, "  ", "x").is_err());
        // 磁盘上不是合法密文 -> 解密报错
        std::fs::create_dir_all(root.join("keys")).unwrap();
        std::fs::write(root.join("keys").join("bad.key"), "not-dpapi").unwrap();
        assert!(read_key_file_in(&root, "keys/bad.key").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn referenced_key_files_covers_groups_and_trash() {
        let mut data = ProjectData::default();
        data.groups.push(Group {
            name: "G".into(),
            alias: String::new(),
            projects: vec![ssh_project("keys/a.key")],
        });
        let mut trashed = ssh_project("keys/b.key");
        trashed.ssh_key_file = String::new();
        let mut item = DeletedItem::from_project(&trashed, "G", 1);
        item.ssh_key_file = "keys/c.key".into();
        let mut group_item = DeletedItem::from_group(&Group::new("H"), 2);
        group_item.projects.push(ssh_project("keys/d.key"));
        data.trash.push(item);
        data.trash.push(group_item);
        let refs = referenced_key_files(&data);
        assert!(refs.contains(&"keys/a.key".to_string()));
        assert!(refs.contains(&"keys/c.key".to_string()));
        assert!(refs.contains(&"keys/d.key".to_string()));
        assert!(!refs.contains(&"keys/b.key".to_string()));
    }

    #[cfg(windows)]
    #[test]
    fn startup_maintenance_pends_orphans_and_tmp() {
        let root = temp_root();
        let mut data = ProjectData::default();
        data.groups.push(Group {
            name: "G".into(),
            alias: String::new(),
            projects: vec![ssh_project("keys/keep.key")],
        });
        std::fs::create_dir_all(root.join("keys")).unwrap();
        std::fs::create_dir_all(root.join("keys_tmp")).unwrap();
        std::fs::write(root.join("keys").join("keep.key"), "enc").unwrap();
        std::fs::write(root.join("keys").join("orphan.key"), "enc").unwrap();
        std::fs::write(root.join("keys").join("leftover.key.tmp"), "enc").unwrap();
        std::fs::write(root.join("keys_tmp").join("leftover"), "plain").unwrap();

        // pending 指向一个删除会成功的文件 -> 摘除并标记 changed
        std::fs::write(root.join("keys").join("gone.key"), "enc").unwrap();
        data.pending_key_deletes.push("keys/gone.key".into());
        assert!(startup_maintenance_in(&root, &mut data, true));
        assert!(data.pending_key_deletes.is_empty());

        // 孤儿与 tmp 残留被清理，被引用的保留；
        // keys_tmp 会话文件新于阈值 -> 启动维护跳过（活跃会话保护）
        assert!(root.join("keys").join("keep.key").exists());
        assert!(!root.join("keys").join("orphan.key").exists());
        assert!(!root.join("keys").join("leftover.key.tmp").exists());
        assert!(root.join("keys_tmp").join("leftover").exists());

        // 再跑一次：无 pending、无变化 -> 返回 false（幂等）
        assert!(!startup_maintenance_in(&root, &mut data, true));

        // busy.key 是目录，删除失败 -> 留在待删列表，等待下次重试
        std::fs::create_dir_all(root.join("keys").join("busy.key")).unwrap();
        data.pending_key_deletes.push("keys/busy.key".into());
        let _ = startup_maintenance_in(&root, &mut data, true);
        assert_eq!(data.pending_key_deletes, vec!["keys/busy.key".to_string()]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn keys_tmp_cleanup_respects_cutoff() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("keys_tmp")).unwrap();
        let file = root.join("keys_tmp").join("session");
        std::fs::write(&file, "data").unwrap();
        let now = std::time::SystemTime::now();

        // cutoff 在过去：文件新于阈值 -> 视为活跃会话，跳过
        shred_keys_tmp_in(
            &root,
            now - std::time::Duration::from_secs(KEYS_TMP_KEEP_SECS),
        );
        assert!(file.exists(), "新于阈值的会话文件必须保留");

        // cutoff 在未来：文件旧于阈值 -> 覆写删除（崩溃残留兜底路径）
        shred_keys_tmp_in(
            &root,
            now + std::time::Duration::from_secs(KEYS_TMP_KEEP_SECS),
        );
        assert!(!file.exists());

        // 目录缺失 / 非 keys_tmp 目录：静默无操作
        shred_keys_tmp_in(&root, now);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn orphan_sweep_disabled_keeps_all_key_files() {
        let root = temp_root();
        let mut data = ProjectData::default();
        // 数据退化场景：引用集为空（groups/trash 均空）
        std::fs::create_dir_all(root.join("keys")).unwrap();
        std::fs::write(root.join("keys").join("precious.key"), "enc").unwrap();
        std::fs::write(root.join("keys").join("orphan.key"), "enc").unwrap();

        // 不允许清理：密钥文件全部保留（pending 重试与 tmp 清理仍执行）
        assert!(!startup_maintenance_in(&root, &mut data, false));
        assert!(root.join("keys").join("precious.key").exists());
        assert!(root.join("keys").join("orphan.key").exists());

        // 允许清理后：无引用的文件按孤儿删除
        let _ = startup_maintenance_in(&root, &mut data, true);
        assert!(!root.join("keys").join("precious.key").exists());
        assert!(!root.join("keys").join("orphan.key").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn orphan_sweep_gating_covers_trash_references() {
        let root = temp_root();
        let mut data = ProjectData::default();
        // groups 为空但 trash 快照引用 key：清理仍应安全（引用可信）
        let mut trashed = ssh_project("keys/in-trash.key");
        trashed.ssh_key_file = "keys/in-trash.key".into();
        data.trash.push(DeletedItem::from_project(&trashed, "G", 1));
        std::fs::create_dir_all(root.join("keys")).unwrap();
        std::fs::write(root.join("keys").join("in-trash.key"), "enc").unwrap();
        std::fs::write(root.join("keys").join("orphan.key"), "enc").unwrap();
        let _ = startup_maintenance_in(&root, &mut data, true);
        assert!(root.join("keys").join("in-trash.key").exists());
        assert!(!root.join("keys").join("orphan.key").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn shred_and_remove_zeroes_and_removes() {
        let root = temp_root();
        let path = root.join("plain.txt");
        std::fs::write(&path, "super-secret").unwrap();
        shred_and_remove(&path);
        assert!(!path.exists());
        // 不存在的文件也不报错
        shred_and_remove(&path);
        let _ = std::fs::remove_dir_all(&root);
    }
}
