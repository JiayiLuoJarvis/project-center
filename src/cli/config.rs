use anyhow::{Result, bail};

use crate::persist::{
    AppConfig, Config, ConfigEnv, add_tool, edit_tool, remove_tool, reset_config,
};

use super::args::ConfigCommand;

pub(crate) fn parse_config_env(name: &str) -> Result<ConfigEnv> {
    if name.eq_ignore_ascii_case("wsl") {
        Ok(ConfigEnv::Wsl)
    } else if name.eq_ignore_ascii_case("powershell") {
        Ok(ConfigEnv::PowerShell)
    } else if name.eq_ignore_ascii_case("ide") {
        Ok(ConfigEnv::Ide)
    } else {
        bail!("未知环境: {name}，可选 wsl/powershell/ide")
    }
}

/// 写盘失败仅警告（延续「仅警告」哲学），不中断操作。
pub(crate) fn save_config(config: &AppConfig) {
    if !Config::save(config) {
        eprintln!("警告：无法写入 config.json，本次修改未持久化。");
    }
}

pub(crate) fn cmd_config(command: ConfigCommand) -> Result<()> {
    let mut config = Config::load();
    match command {
        ConfigCommand::List { env } => {
            let envs: Vec<ConfigEnv> = match env {
                Some(name) => vec![parse_config_env(&name)?],
                None => vec![ConfigEnv::Wsl, ConfigEnv::PowerShell, ConfigEnv::Ide],
            };
            for env in envs {
                println!("[{}]", env.label());
                let tools = env.tools(&config);
                if tools.is_empty() {
                    println!("  （暂无工具）");
                } else {
                    for tool in tools {
                        println!("  {} ({})", tool.name, tool.command);
                    }
                }
            }
        }
        ConfigCommand::Add { env, name, command } => {
            add_tool(&mut config, parse_config_env(&env)?, &name, &command)
                .map_err(anyhow::Error::msg)?;
            save_config(&config);
            println!("工具已添加: {}", name.trim());
        }
        ConfigCommand::Edit {
            env,
            index,
            name,
            command,
        } => {
            let env = parse_config_env(&env)?;
            let tools = env.tools(&config);
            let current = tools
                .get(index)
                .ok_or_else(|| anyhow::anyhow!("工具索引无效"))?;
            let name = name.unwrap_or_else(|| current.name.clone());
            let command = command.unwrap_or_else(|| current.command.clone());
            edit_tool(&mut config, env, index, &name, &command).map_err(anyhow::Error::msg)?;
            save_config(&config);
            println!("工具已更新: {}", name.trim());
        }
        ConfigCommand::Rm { env, index } => {
            remove_tool(&mut config, parse_config_env(&env)?, index).map_err(anyhow::Error::msg)?;
            save_config(&config);
            println!("工具已删除。");
        }
        ConfigCommand::Reset => {
            reset_config(&mut config);
            save_config(&config);
            println!("配置已恢复默认。");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_env_case_insensitive() {
        assert_eq!(parse_config_env("WSL").unwrap(), ConfigEnv::Wsl);
        assert_eq!(
            parse_config_env("powershell").unwrap(),
            ConfigEnv::PowerShell
        );
        assert_eq!(parse_config_env("IDE").unwrap(), ConfigEnv::Ide);
        assert!(parse_config_env("bash").is_err());
    }
}
