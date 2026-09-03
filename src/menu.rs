use crate::config::AppConfig;
use crate::launcher::LaunchEnv;
use crate::models::Project;

/// 单个可启动选项：环境 + 工具名 + 启动命令（终端命令为空串）。
#[derive(Clone, Debug)]
pub struct LaunchOption {
    pub env: LaunchEnv,
    pub tool_name: String,
    pub command: String,
    /// 是否为项目自定义命令（显示 `⚙ 名称 (env)` 风格标签）。
    pub is_custom: bool,
}

impl LaunchOption {
    pub fn label(&self) -> String {
        match self.env {
            LaunchEnv::Explorer => "文件夹".to_string(),
            _ if self.is_custom => {
                format!("⚙ {} ({})", self.tool_name, self.env.short_label())
            }
            _ => format!("{} · {}", self.env.label(), self.tool_name),
        }
    }
}

fn push_terminal(options: &mut Vec<LaunchOption>, env: LaunchEnv) {
    options.push(LaunchOption {
        env,
        tool_name: "终端".into(),
        command: String::new(),
        is_custom: false,
    });
}

/// 构建项目的全部启动选项：WSL（终端 + config.wsl）；
/// 有 Windows 路径时追加 PowerShell（终端 + config.powershell）、IDE（config.ide）、文件夹；
/// 尾部追加项目自定义命令段。
pub fn build_launch_options(project: &Project, config: &AppConfig) -> Vec<LaunchOption> {
    let mut options = Vec::new();
    push_terminal(&mut options, LaunchEnv::Wsl);
    options.extend(config.wsl.iter().map(|tool| LaunchOption {
        env: LaunchEnv::Wsl,
        tool_name: tool.name.clone(),
        command: tool.command.clone(),
        is_custom: false,
    }));
    if project.has_windows_path() {
        push_terminal(&mut options, LaunchEnv::PowerShell);
        options.extend(config.powershell.iter().map(|tool| LaunchOption {
            env: LaunchEnv::PowerShell,
            tool_name: tool.name.clone(),
            command: tool.command.clone(),
            is_custom: false,
        }));
        options.extend(config.ide.iter().map(|tool| LaunchOption {
            env: LaunchEnv::Ide,
            tool_name: tool.name.clone(),
            command: tool.command.clone(),
            is_custom: false,
        }));
        options.push(LaunchOption {
            env: LaunchEnv::Explorer,
            tool_name: "文件夹".into(),
            command: String::new(),
            is_custom: false,
        });
    }
    options.extend(
        project
            .commands
            .iter()
            .filter(|command| {
                let env = LaunchEnv::from_command_env(&command.env);
                project.has_windows_path() || env == LaunchEnv::Wsl
            })
            .map(|command| LaunchOption {
                env: LaunchEnv::from_command_env(&command.env),
                tool_name: command.name.clone(),
                command: command.command.clone(),
                is_custom: true,
            }),
    );
    options
}

/// 项目默认工具命中的选项索引（大小写不敏感）；默认工具可指向自定义命令名。
pub fn default_option_index(options: &[LaunchOption], project: &Project) -> Option<usize> {
    if !project.has_default_tool() {
        return None;
    }
    options
        .iter()
        .position(|option| option.tool_name.eq_ignore_ascii_case(&project.default_tool))
}

/// 默认工具命中时置顶并标 `[默认] ` 前缀；返回排序后的选项与显示标签。
pub fn default_first(
    options: Vec<LaunchOption>,
    project: &Project,
) -> (Vec<LaunchOption>, Vec<String>) {
    let Some(index) = default_option_index(&options, project) else {
        let labels = options.iter().map(|option| option.label()).collect();
        return (options, labels);
    };
    let mut items = options;
    let item = items.remove(index);
    let label = format!("[默认] {}", item.label());
    items.insert(0, item);
    let mut labels = vec![label];
    labels.extend(items.iter().skip(1).map(|option| option.label()));
    (items, labels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::models::ProjectCommand;

    fn cfg() -> AppConfig {
        AppConfig::defaults()
    }

    #[test]
    fn build_options_windows_project() {
        let p = Project::new("a", r"E:\a", "");
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts.len(), 9);
        assert_eq!(opts[0].tool_name, "终端");
        assert_eq!(opts[1].tool_name, "opencode");
        assert_eq!(opts[3].tool_name, "终端");
        assert_eq!(opts[6].tool_name, "VS Code");
        assert_eq!(opts[8].env, LaunchEnv::Explorer);
        assert_eq!(opts[8].label(), "文件夹");
    }

    #[test]
    fn build_options_wsl_only() {
        let p = Project::new("a", "", "/mnt/e/a");
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts.len(), 3);
    }

    #[test]
    fn build_options_appends_custom_commands() {
        let p = Project {
            commands: vec![
                ProjectCommand::new("构建", "wsl", "make build"),
                ProjectCommand::new("运行脚本", "powershell", "npm run dev"),
                ProjectCommand::new("检查", "ide", "code --reuse-window"),
            ],
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts.len(), 12);
        assert_eq!(opts[9].label(), "⚙ 构建 (wsl)");
        assert_eq!(opts[10].label(), "⚙ 运行脚本 (ps)");
        assert_eq!(opts[11].label(), "⚙ 检查 (ide)");
    }

    #[test]
    fn build_options_custom_commands_wsl_only_project() {
        let p = Project {
            commands: vec![
                ProjectCommand::new("构建", "wsl", "make"),
                ProjectCommand::new("运行脚本", "powershell", "npm run dev"),
                ProjectCommand::new("检查", "ide", "code --reuse-window"),
            ],
            ..Project::new("a", "", "/mnt/e/a")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts.len(), 4);
        assert_eq!(opts[3].label(), "⚙ 构建 (wsl)");
        assert!(
            !opts
                .iter()
                .any(|opt| opt.is_custom && opt.env != LaunchEnv::Wsl),
            "WSL-only 项目不应出现 ps/ide 自定义命令"
        );
    }

    #[test]
    fn build_options_invalid_custom_env_falls_back_to_ide() {
        let p = Project {
            commands: vec![ProjectCommand::new("自检", "bash", "check")],
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts[9].label(), "⚙ 自检 (ide)");
    }

    #[test]
    fn default_option_matches_custom_command() {
        let p = Project {
            default_tool: "构建".into(),
            commands: vec![ProjectCommand::new("构建", "wsl", "make")],
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(default_option_index(&opts, &p), Some(9));
    }

    #[test]
    fn default_first_marks_custom_command_at_top() {
        let p = Project {
            default_tool: "构建".into(),
            commands: vec![ProjectCommand::new("构建", "wsl", "make")],
            ..Project::new("a", r"E:\a", "")
        };
        let (items, labels) = default_first(build_launch_options(&p, &cfg()), &p);
        assert_eq!(items[0].tool_name, "构建");
        assert_eq!(labels[0], "[默认] ⚙ 构建 (wsl)");
        assert_eq!(items.len(), labels.len());
        assert_eq!(items[9].tool_name, "文件夹");
    }

    #[test]
    fn default_option_match_case_insensitive() {
        let p = Project {
            default_tool: "OPENCODE".into(),
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(default_option_index(&opts, &p), Some(1));
    }

    #[test]
    fn default_option_missing_returns_none() {
        let p = Project {
            default_tool: "ghost".into(),
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(default_option_index(&opts, &p), None);
    }

    #[test]
    fn default_option_empty_returns_none() {
        let p = Project::new("a", r"E:\a", "");
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(default_option_index(&opts, &p), None);
    }

    #[test]
    fn default_first_puts_marked_item_at_top() {
        let p = Project {
            default_tool: "Cursor".into(),
            ..Project::new("a", r"E:\a", "")
        };
        let options = build_launch_options(&p, &cfg());
        let (items, labels) = default_first(options, &p);
        assert_eq!(items[0].tool_name, "Cursor");
        assert_eq!(labels[0], "[默认] IDE · Cursor");
        assert_eq!(items.len(), labels.len());
        assert_eq!(items[8].tool_name, "文件夹");
    }

    #[test]
    fn default_first_without_match_keeps_order() {
        let p = Project {
            default_tool: "ghost".into(),
            ..Project::new("a", r"E:\a", "")
        };
        let options = build_launch_options(&p, &cfg());
        let (items, labels) = default_first(options, &p);
        assert_eq!(items[0].tool_name, "终端");
        assert_eq!(labels[0], "WSL · 终端");
    }
}
