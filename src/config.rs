use std::path::{Path, PathBuf};

/// 保留名：WSL / PowerShell 环境固定的「终端」项，配置工具不得使用。
pub const RESERVED_TERMINAL: &str = "终端";

/// 单个可配置启动工具。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Tool {
    pub name: String,
    pub command: String,
}

impl Tool {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
        }
    }
}

/// 各环境的启动工具列表；顶层缺失键视为空列表。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AppConfig {
    pub wsl: Vec<Tool>,
    pub powershell: Vec<Tool>,
    pub ide: Vec<Tool>,
    /// PIN 校验记录（PBKDF2 哈希，非 PIN 本身）；未设置时为 None 且不写盘。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<crate::secret::PinRecord>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::defaults()
    }
}

impl AppConfig {
    /// 内置默认工具，与 `DEFAULT_CONFIG` 保持一致（有测试保证）。
    pub fn defaults() -> Self {
        Self {
            wsl: vec![
                Tool::new("opencode", "opencode"),
                Tool::new("cursor-agent", "cursor-agent"),
            ],
            powershell: vec![
                Tool::new("opencode", "opencode"),
                Tool::new("cursor-agent", "cursor-agent"),
            ],
            ide: vec![Tool::new("VS Code", "code"), Tool::new("Cursor", "cursor")],
            pin: None,
        }
    }
}

/// 可配置工具的环境，与 config.json 顶层键一一对应。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigEnv {
    Wsl,
    PowerShell,
    Ide,
}

impl ConfigEnv {
    #[allow(dead_code)]
    pub fn key(self) -> &'static str {
        match self {
            Self::Wsl => "wsl",
            Self::PowerShell => "powershell",
            Self::Ide => "ide",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Wsl => "WSL",
            Self::PowerShell => "PowerShell",
            Self::Ide => "IDE",
        }
    }

    pub fn tools(self, config: &AppConfig) -> &[Tool] {
        match self {
            Self::Wsl => &config.wsl,
            Self::PowerShell => &config.powershell,
            Self::Ide => &config.ide,
        }
    }

    pub fn tools_mut(self, config: &mut AppConfig) -> &mut Vec<Tool> {
        match self {
            Self::Wsl => &mut config.wsl,
            Self::PowerShell => &mut config.powershell,
            Self::Ide => &mut config.ide,
        }
    }
}

/// 首次启动生成的默认配置内容，与 `AppConfig::defaults()` 保持一致。
const DEFAULT_CONFIG: &str = r#"{
  "wsl": [
    { "name": "opencode", "command": "opencode" },
    { "name": "cursor-agent", "command": "cursor-agent" }
  ],
  "powershell": [
    { "name": "opencode", "command": "opencode" },
    { "name": "cursor-agent", "command": "cursor-agent" }
  ],
  "ide": [
    { "name": "VS Code", "command": "code" },
    { "name": "Cursor", "command": "cursor" }
  ]
}
"#;

pub struct Config;

impl Config {
    pub fn load() -> AppConfig {
        Self::load_from_dir(&Self::exe_dir())
    }

    /// 从指定目录加载 `config.json`；缺失或损坏时自愈并返回默认配置。
    pub(crate) fn load_from_dir(dir: &Path) -> AppConfig {
        let path = dir.join("config.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            // 首次启动：生成默认配置供用户查看与修改。
            if std::fs::write(&path, DEFAULT_CONFIG).is_err() {
                eprintln!("无法写入默认 config.json，本次使用内置默认配置。");
            }
            return AppConfig::defaults();
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            Self::backup_and_reset(dir);
            return AppConfig::defaults();
        };
        if !value.is_object() {
            Self::backup_and_reset(dir);
            return AppConfig::defaults();
        }
        parse_config(&value)
    }

    pub fn save(config: &AppConfig) -> bool {
        Self::save_to_dir(&Self::exe_dir(), config)
    }

    /// 原子写：写 `config.json.tmp` -> 删除旧文件 -> rename。
    pub(crate) fn save_to_dir(dir: &Path, config: &AppConfig) -> bool {
        let path = dir.join("config.json");
        let Ok(json) = serde_json::to_string_pretty(config) else {
            return false;
        };
        let tmp = dir.join("config.json.tmp");
        if std::fs::write(&tmp, json).is_err() {
            return false;
        }
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&tmp, &path).is_ok()
    }

    fn exe_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// 备份损坏的配置并重建默认；备份或写入失败仅警告，不影响使用。
    fn backup_and_reset(dir: &Path) {
        let path = dir.join("config.json");
        let backup = dir.join("config.json.bak");
        let _ = std::fs::remove_file(&backup);
        let backed_up = std::fs::rename(&path, &backup).is_ok();
        if !backed_up {
            let _ = std::fs::remove_file(&path);
        }
        if std::fs::write(&path, DEFAULT_CONFIG).is_err() {
            eprintln!("config.json 解析失败，且无法写入默认配置，本次使用内置默认。");
            return;
        }
        if backed_up {
            eprintln!("config.json 解析失败，已备份到 config.json.bak 并恢复默认配置。");
        } else {
            eprintln!("config.json 解析失败，已恢复默认配置（原文件备份失败）。");
        }
    }
}

fn parse_config(value: &serde_json::Value) -> AppConfig {
    AppConfig {
        wsl: parse_tools(value, "wsl"),
        powershell: parse_tools(value, "powershell"),
        ide: parse_tools(value, "ide"),
        pin: parse_pin(value),
    }
}

/// 解析顶层 `pin` 校验记录；缺失或损坏返回 None（仅警告，不影响工具配置）。
fn parse_pin(value: &serde_json::Value) -> Option<crate::secret::PinRecord> {
    let pin_value = value.get("pin")?;
    match serde_json::from_value::<crate::secret::PinRecord>(pin_value.clone()) {
        Ok(record) => Some(record),
        Err(_) => {
            eprintln!("config.json: `pin` 字段损坏，已忽略（需重新 pcs pin set）。");
            None
        }
    }
}

pub fn add_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    name: &str,
    command: &str,
) -> Result<(), String> {
    let tool = validate_tool_fields(name.trim(), command.trim())?;
    let tools = env.tools_mut(config);
    if tools
        .iter()
        .any(|t| t.name.eq_ignore_ascii_case(&tool.name))
    {
        return Err(format!("名称「{}」已存在", tool.name));
    }
    tools.push(tool);
    Ok(())
}

pub fn edit_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    index: usize,
    name: &str,
    command: &str,
) -> Result<(), String> {
    let tool = validate_tool_fields(name.trim(), command.trim())?;
    let tools = env.tools_mut(config);
    if index >= tools.len() {
        return Err("工具索引无效".into());
    }
    if tools
        .iter()
        .enumerate()
        .any(|(i, t)| i != index && t.name.eq_ignore_ascii_case(&tool.name))
    {
        return Err(format!("名称「{}」已存在", tool.name));
    }
    tools[index] = tool;
    Ok(())
}

pub fn remove_tool(config: &mut AppConfig, env: ConfigEnv, index: usize) -> Result<(), String> {
    let tools = env.tools_mut(config);
    if index >= tools.len() {
        return Err("工具索引无效".into());
    }
    tools.remove(index);
    Ok(())
}

pub fn reset_config(config: &mut AppConfig) {
    *config = AppConfig::defaults();
}

fn parse_tools(value: &serde_json::Value, key: &str) -> Vec<Tool> {
    let Some(items) = value.get(key).and_then(|v| v.as_array()) else {
        if value.get(key).is_some() {
            eprintln!("config.json: `{key}` 应为数组，该环境配置已被忽略。");
        }
        return Vec::new();
    };
    let mut tools: Vec<Tool> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match validate_tool(item) {
            Ok(tool) => {
                if tools.iter().any(|existing| existing.name == tool.name) {
                    eprintln!(
                        "config.json: {key}[{index}] 名称「{}」重复，已跳过。",
                        tool.name
                    );
                } else {
                    tools.push(tool);
                }
            }
            Err(reason) => eprintln!("config.json: {key}[{index}] 无效（{reason}），已跳过。"),
        }
    }
    tools
}

/// 校验 name / command 组合并构造 Tool；调用方需自行 trim。
fn validate_tool_fields(name: &str, command: &str) -> Result<Tool, String> {
    if name.is_empty() {
        return Err("缺少 name 或 name 为空".into());
    }
    if name == RESERVED_TERMINAL {
        return Err(format!("name 与保留名「{RESERVED_TERMINAL}」冲突"));
    }
    if command.is_empty() {
        return Err("缺少 command 或 command 为空".into());
    }
    if command.chars().any(|c| c.is_whitespace()) {
        return Err("command 含空白字符".into());
    }
    Ok(Tool::new(name, command))
}

/// 校验单个工具项；返回错误原因字符串。
fn validate_tool(value: &serde_json::Value) -> Result<Tool, String> {
    let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let command = value.get("command").and_then(|v| v.as_str()).unwrap_or("");
    validate_tool_fields(name.trim(), command.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pcs_cfg_test_{stamp}"));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn write_config(dir: &Path, content: &str) {
        std::fs::write(dir.join("config.json"), content).unwrap();
    }

    fn read_config(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("config.json")).unwrap()
    }

    #[test]
    fn missing_file_generates_default() {
        let dir = temp_dir();
        let config = Config::load_from_dir(&dir);
        assert_eq!(config, AppConfig::defaults());
        assert_eq!(read_config(&dir), DEFAULT_CONFIG);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_valid_config() {
        let dir = temp_dir();
        write_config(
            &dir,
            r#"{"wsl":[{"name":"opencode","command":"opencode"}],"ide":[{"name":"CodeBuddy","command":"codebuddy"}]}"#,
        );
        let config = Config::load_from_dir(&dir);
        assert_eq!(config.wsl.len(), 1);
        assert_eq!(config.wsl[0].name, "opencode");
        assert_eq!(config.wsl[0].command, "opencode");
        assert_eq!(config.ide[0].command, "codebuddy");
        assert!(config.powershell.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_key_is_empty_list() {
        let dir = temp_dir();
        write_config(&dir, r#"{"ide":[{"name":"VS Code","command":"code"}]}"#);
        let config = Config::load_from_dir(&dir);
        assert!(config.wsl.is_empty());
        assert!(config.powershell.is_empty());
        assert_eq!(config.ide.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_keys_ignored() {
        let dir = temp_dir();
        write_config(
            &dir,
            r#"{"extra":[{"name":"x","command":"y"}],"wsl":[{"name":"a","command":"b"}]}"#,
        );
        let config = Config::load_from_dir(&dir);
        assert_eq!(config.wsl.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_json_backs_up_and_resets() {
        let dir = temp_dir();
        write_config(&dir, "{ not valid json");
        let config = Config::load_from_dir(&dir);
        assert_eq!(config, AppConfig::defaults());
        assert_eq!(
            std::fs::read_to_string(dir.join("config.json.bak")).unwrap(),
            "{ not valid json"
        );
        assert_eq!(read_config(&dir), DEFAULT_CONFIG);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_object_resets() {
        let dir = temp_dir();
        write_config(&dir, "\"hello\"");
        let config = Config::load_from_dir(&dir);
        assert_eq!(config, AppConfig::defaults());
        assert!(dir.join("config.json.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_tools_skipped() {
        let dir = temp_dir();
        write_config(
            &dir,
            r#"{"wsl":[
                {"name":"ok","command":"good"},
                {"name":"","command":"x"},
                {"name":"x","command":""},
                {"command":"no-name"},
                {"name":"no-cmd"},
                {"name":"sp","command":"two words"},
                {"name":"终端","command":"t"},
                {"name":42,"command":"n"}
            ]}"#,
        );
        let config = Config::load_from_dir(&dir);
        assert_eq!(config.wsl.len(), 1);
        assert_eq!(config.wsl[0].name, "ok");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_tool_names_skipped() {
        let dir = temp_dir();
        write_config(
            &dir,
            r#"{"ide":[
                {"name":"VS Code","command":"code"},
                {"name":"VS Code","command":"code-insiders"}
            ]}"#,
        );
        let config = Config::load_from_dir(&dir);
        assert_eq!(config.ide.len(), 1);
        assert_eq!(config.ide[0].command, "code");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_array_key_ignored() {
        let dir = temp_dir();
        write_config(&dir, r#"{"wsl":"opencode"}"#);
        let config = Config::load_from_dir(&dir);
        assert!(config.wsl.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_config_matches_defaults() {
        let value: serde_json::Value = serde_json::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(parse_config(&value), AppConfig::defaults());
    }

    #[test]
    fn valid_file_not_rewritten() {
        let dir = temp_dir();
        let content = r#"{"ide":[{"name":"CodeBuddy","command":"codebuddy"}]}"#;
        write_config(&dir, content);
        let _ = Config::load_from_dir(&dir);
        assert!(!dir.join("config.json.bak").exists());
        assert_eq!(read_config(&dir), content);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn base_config() -> AppConfig {
        AppConfig::defaults()
    }

    #[test]
    fn add_tool_appends_to_env() {
        let mut config = base_config();
        add_tool(&mut config, ConfigEnv::Ide, "CodeBuddy", "codebuddy").unwrap();
        assert_eq!(config.ide.last().unwrap().name, "CodeBuddy");
        assert_eq!(config.ide.last().unwrap().command, "codebuddy");
    }

    #[test]
    fn add_tool_rejects_duplicate_name() {
        let mut config = base_config();
        assert!(add_tool(&mut config, ConfigEnv::Ide, "VS Code", "other").is_err());
        assert_eq!(config.ide.len(), 2);
    }

    #[test]
    fn add_tool_rejects_invalid_fields() {
        let mut config = base_config();
        assert!(add_tool(&mut config, ConfigEnv::Wsl, "", "x").is_err());
        assert!(add_tool(&mut config, ConfigEnv::Wsl, "终端", "x").is_err());
        assert!(add_tool(&mut config, ConfigEnv::Wsl, "x", "").is_err());
        assert!(add_tool(&mut config, ConfigEnv::Wsl, "x", "two words").is_err());
        assert_eq!(config.wsl.len(), 2);
    }

    #[test]
    fn edit_tool_updates_in_place() {
        let mut config = base_config();
        edit_tool(&mut config, ConfigEnv::Ide, 0, "CodeBuddy", "codebuddy").unwrap();
        assert_eq!(config.ide[0].name, "CodeBuddy");
        assert_eq!(config.ide[0].command, "codebuddy");
        assert_eq!(config.ide.len(), 2);
    }

    #[test]
    fn edit_tool_rejects_duplicate_and_oob() {
        let mut config = base_config();
        assert!(edit_tool(&mut config, ConfigEnv::Ide, 0, "Cursor", "x").is_err());
        assert!(edit_tool(&mut config, ConfigEnv::Ide, 5, "x", "x").is_err());
    }

    #[test]
    fn remove_tool_removes_at_index() {
        let mut config = base_config();
        remove_tool(&mut config, ConfigEnv::Ide, 0).unwrap();
        assert_eq!(config.ide.len(), 1);
        assert_eq!(config.ide[0].name, "Cursor");
        assert!(remove_tool(&mut config, ConfigEnv::Ide, 5).is_err());
    }

    #[test]
    fn reset_config_restores_defaults() {
        let mut config = base_config();
        add_tool(&mut config, ConfigEnv::Wsl, "extra", "extra").unwrap();
        remove_tool(&mut config, ConfigEnv::Ide, 0).unwrap();
        reset_config(&mut config);
        assert_eq!(config, AppConfig::defaults());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir();
        let mut config = AppConfig::defaults();
        add_tool(&mut config, ConfigEnv::Ide, "CodeBuddy", "codebuddy").unwrap();
        assert!(Config::save_to_dir(&dir, &config));
        let loaded = Config::load_from_dir(&dir);
        assert_eq!(loaded, config);
        assert!(!dir.join("config.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_serializes_all_env_keys() {
        let dir = temp_dir();
        let config = AppConfig::defaults();
        assert!(Config::save_to_dir(&dir, &config));
        let text = std::fs::read_to_string(dir.join("config.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(value.get("wsl").is_some());
        assert!(value.get("powershell").is_some());
        assert!(value.get("ide").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pin_round_trip_and_absent_skipped() {
        let dir = temp_dir();
        // 未设 PIN：config.json 不含 pin 键
        let config = AppConfig::defaults();
        assert!(config.pin.is_none());
        assert!(Config::save_to_dir(&dir, &config));
        let text = std::fs::read_to_string(dir.join("config.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(value.get("pin").is_none(), "None 时不应写出 pin 键");
        // 设置 PIN：round trip 保留
        let mut config = AppConfig::defaults();
        config.pin = Some(crate::secret::PinRecord {
            salt: "AAAA".into(),
            iterations: 600_000,
            hash: "BBBB".into(),
        });
        assert!(Config::save_to_dir(&dir, &config));
        let loaded = Config::load_from_dir(&dir);
        assert_eq!(loaded.pin.as_ref().unwrap().hash, "BBBB");
        assert_eq!(loaded.pin.as_ref().unwrap().iterations, 600_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_pin_ignored() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.json"),
            r#"{"wsl":[],"powershell":[],"ide":[],"pin":{"bad":"shape"}}"#,
        )
        .unwrap();
        let config = Config::load_from_dir(&dir);
        assert!(config.pin.is_none());
        assert!(config.ide.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_empty_envs() {
        let dir = temp_dir();
        let config = AppConfig {
            wsl: Vec::new(),
            powershell: Vec::new(),
            ide: Vec::new(),
            pin: None,
        };
        assert!(Config::save_to_dir(&dir, &config));
        assert_eq!(Config::load_from_dir(&dir), config);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
