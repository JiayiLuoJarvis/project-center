use super::*;
use crate::domain::models::{
    DeletedItem, Group, Project, ProjectCommand, ProjectData, current_unix_ts,
};

fn data() -> ProjectData {
    ProjectData {
        groups: vec![
            Group {
                name: "Work".into(),
                alias: String::new(),
                projects: vec![Project::new("app", r"E:\dev\app", "")],
            },
            Group {
                name: "Personal".into(),
                alias: String::new(),
                projects: vec![Project::new("app", r"E:\personal\app", "")],
            },
        ],
        ..Default::default()
    }
}

#[test]
fn duplicate_project_requires_group() {
    let data = data();
    assert!(matches!(
        find_project(&data, "app", None),
        Err(Error::ProjectAmbiguous { .. })
    ));
    assert_eq!(find_project(&data, "app", Some("work")).unwrap(), (0, 0));
}

#[test]
fn find_by_alias() {
    let mut data = data();
    data.groups[0].alias = "wk".into();
    data.groups[0].projects[0].alias = "pcs".into();
    assert_eq!(find_group(&data, "wk").unwrap(), 0);
    assert_eq!(find_project(&data, "pcs", Some("wk")).unwrap(), (0, 0));
    assert_eq!(find_project(&data, "pcs", None).unwrap(), (0, 0));
    assert!(add_group_with_alias(&mut data, "X", "wk").is_err());
    assert!(
        add_project(
            &mut data,
            "Work",
            Project::new("other", r"E:\o", "").with_alias("pcs")
        )
        .is_err()
    );
}

#[test]
fn group_crud_rejects_duplicates() {
    let mut data = ProjectData::default();
    add_group_with_alias(&mut data, "Work", "").unwrap();
    assert!(add_group_with_alias(&mut data, "work", "").is_err());
    rename_group_with_alias(&mut data, "Work", "Personal", None).unwrap();
    assert_eq!(data.groups[0].name, "Personal");
    remove_group(&mut data, "Personal", false).unwrap();
    assert!(data.groups.is_empty());
}

#[test]
fn project_can_move_and_remove() {
    let mut data = data();
    move_project(&mut data, "app", Some("Work"), "Personal").unwrap_err();
    add_group_with_alias(&mut data, "Archive", "").unwrap();
    move_project(&mut data, "app", Some("Work"), "Archive").unwrap();
    assert_eq!(data.groups[0].projects.len(), 0);
    let removed = remove_project(&mut data, "app", Some("Archive"), false).unwrap();
    assert_eq!(removed.name, "app");
    // 软删除进回收站
    assert_eq!(data.trash.len(), 1);
    assert_eq!(data.trash[0].name, "app");
    assert_eq!(data.trash[0].group, "Archive");
}

fn id_data() -> ProjectData {
    ProjectData {
        groups: vec![Group {
            name: "Work".into(),
            alias: String::new(),
            projects: vec![
                Project {
                    id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
                    ..Project::new("app", r"E:\dev\app", "")
                },
                Project {
                    id: "aaaabbbb-0000-1111-2222-333333333333".into(),
                    ..Project::new("other", r"E:\dev\other", "")
                },
            ],
        }],
        ..Default::default()
    }
}

#[test]
fn find_by_id_exact_and_prefix() {
    let data = id_data();
    assert_eq!(
        find_project_by_id(&data, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", None).unwrap(),
        (0, 0)
    );
    assert_eq!(find_project_by_id(&data, "aaaaaaaa", None).unwrap(), (0, 0));
    assert_eq!(find_project_by_id(&data, "AAAABBBB", None).unwrap(), (0, 1));
}

#[test]
fn find_by_id_rejects_unknown_and_ambiguous() {
    let data = id_data();
    assert!(matches!(
        find_project_by_id(&data, "zzzz", None),
        Err(Error::ProjectIdNotFound { .. })
    ));
    assert!(matches!(
        find_project_by_id(&data, "aaaa", None),
        Err(Error::ProjectIdAmbiguous { .. })
    ));
    assert!(matches!(
        find_project_by_id(&data, "", None),
        Err(Error::ProjectIdEmpty)
    ));
}

#[test]
fn remove_and_move_by_id() {
    let mut data = id_data();
    remove_project_by_id(&mut data, "aaaaaaaa", None, false).unwrap();
    assert_eq!(data.groups[0].projects.len(), 1);
    add_group_with_alias(&mut data, "Archive", "").unwrap();
    move_project_by_id(&mut data, "aaaabbbb", Some("Work"), "Archive").unwrap();
    assert_eq!(data.groups[0].projects.len(), 0);
    assert_eq!(data.groups[1].projects[0].name, "other");
}

#[test]
fn edit_project_updates_path_and_derived_wsl_path() {
    let mut data = data();
    edit_project_full(
        &mut data,
        "app",
        Some("Work"),
        Some("new-app"),
        None,
        Some(r"F:\dev\new-app"),
        None,
    )
    .unwrap();
    let project = &data.groups[0].projects[0];
    assert_eq!(project.name, "new-app");
    assert_eq!(project.wsl_path, "/mnt/f/dev/new-app");
}

#[test]
fn edit_project_can_clear_windows_path_keeping_wsl() {
    let mut data = data();
    let id = data.groups[0].projects[0].id.clone();
    data.groups[0].projects[0].wsl_path = "/mnt/e/dev/app".into();
    edit_project_full(&mut data, "app", Some("Work"), None, None, Some(""), None).unwrap();
    let project = &data.groups[0].projects[0];
    assert_eq!(project.id, id);
    assert_eq!(project.path, "");
    assert_eq!(project.wsl_path, "/mnt/e/dev/app");
    assert!(!project.has_windows_path());
}

#[test]
fn edit_project_can_clear_both_paths() {
    let mut data = data();
    edit_project_full(
        &mut data,
        "app",
        Some("Work"),
        None,
        None,
        Some(""),
        Some(""),
    )
    .unwrap();
    let project = &data.groups[0].projects[0];
    assert_eq!(project.path, "");
    assert_eq!(project.wsl_path, "");
}

#[test]
fn edit_project_derived_wsl_keeps_manual_override() {
    let mut data = data();
    edit_project_full(
        &mut data,
        "app",
        Some("Work"),
        None,
        None,
        Some(r"F:\dev\app"),
        Some("/custom/path"),
    )
    .unwrap();
    let project = &data.groups[0].projects[0];
    assert_eq!(project.path, r"F:\dev\app");
    assert_eq!(project.wsl_path, "/custom/path");
}

#[test]
fn set_default_tool_sets_and_clears() {
    let mut data = data();
    let id = data.groups[0].projects[0].id.clone();
    set_default_tool(&mut data, &id, Some("Work"), Some("opencode")).unwrap();
    assert_eq!(data.groups[0].projects[0].default_tool, "opencode");
    set_default_tool(&mut data, &id, Some("Work"), None).unwrap();
    assert_eq!(data.groups[0].projects[0].default_tool, "");
}

#[test]
fn add_project_command_stores_canonical_env() {
    let mut data = data();
    let id = data.groups[0].projects[0].id.clone();
    add_project_command(
        &mut data,
        &id,
        Some("Work"),
        "构建",
        "PowerShell",
        "make build",
    )
    .unwrap();
    let commands = &data.groups[0].projects[0].commands;
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].name, "构建");
    assert_eq!(commands[0].env, "powershell");
    assert_eq!(commands[0].command, "make build");
    // 非法 env 视为 ide
    add_project_command(&mut data, &id, Some("Work"), "检查", "bash", "echo hi").unwrap();
    assert_eq!(data.groups[0].projects[0].commands[1].env, "ide");
}

#[test]
fn add_project_command_rejects_duplicate_name() {
    let mut data = data();
    let id = data.groups[0].projects[0].id.clone();
    add_project_command(&mut data, &id, Some("Work"), "build", "wsl", "make").unwrap();
    assert!(add_project_command(&mut data, &id, Some("Work"), "build", "wsl", "make2").is_err());
    assert!(add_project_command(&mut data, &id, Some("Work"), "BUILD", "wsl", "make2").is_err());
    assert_eq!(data.groups[0].projects[0].commands.len(), 1);
}

#[test]
fn add_project_command_rejects_empty_fields() {
    let mut data = data();
    let id = data.groups[0].projects[0].id.clone();
    assert!(add_project_command(&mut data, &id, Some("Work"), "", "wsl", "make").is_err());
    assert!(add_project_command(&mut data, &id, Some("Work"), "构建", "wsl", "").is_err());
    assert!(add_project_command(&mut data, &id, Some("Work"), "  ", "wsl", "make").is_err());
    assert!(data.groups[0].projects[0].commands.is_empty());
}

#[test]
fn edit_project_command_updates_and_rejects_duplicate() {
    let mut data = data();
    let id = data.groups[0].projects[0].id.clone();
    add_project_command(&mut data, &id, Some("Work"), "build", "wsl", "make").unwrap();
    add_project_command(&mut data, &id, Some("Work"), "run", "wsl", "make run").unwrap();
    edit_project_command(
        &mut data,
        &id,
        Some("Work"),
        0,
        "build-new",
        "ide",
        "code .",
    )
    .unwrap();
    let commands = &data.groups[0].projects[0].commands;
    assert_eq!(commands[0].name, "build-new");
    assert_eq!(commands[0].env, "ide");
    assert_eq!(commands[0].command, "code .");
    // 改为已存在命令名 → 报错
    assert!(edit_project_command(&mut data, &id, Some("Work"), 0, "run", "wsl", "x").is_err());
    // 索引越界 → 报错
    assert!(edit_project_command(&mut data, &id, Some("Work"), 9, "x", "wsl", "x").is_err());
}

#[test]
fn remove_project_command_removes_at_index() {
    let mut data = data();
    let id = data.groups[0].projects[0].id.clone();
    add_project_command(&mut data, &id, Some("Work"), "build", "wsl", "make").unwrap();
    add_project_command(&mut data, &id, Some("Work"), "run", "wsl", "make run").unwrap();
    let removed = remove_project_command(&mut data, &id, Some("Work"), 0).unwrap();
    assert_eq!(removed.name, "build");
    assert_eq!(data.groups[0].projects[0].commands.len(), 1);
    assert_eq!(data.groups[0].projects[0].commands[0].name, "run");
    assert!(remove_project_command(&mut data, &id, Some("Work"), 5).is_err());
}

#[test]
fn commands_resolve_by_project_id_prefix() {
    let mut data = id_data();
    data.groups[0].projects[1]
        .commands
        .push(ProjectCommand::new("部署", "wsl", "deploy"));
    add_project_command(
        &mut data,
        "aaaabbbb",
        Some("Work"),
        "测试",
        "ide",
        "cargo test",
    )
    .unwrap();
    let commands = &data.groups[0].projects[1].commands;
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[1].name, "测试");
}

#[test]
fn soft_delete_and_restore_keeps_commands() {
    let mut data = data();
    let id = data.groups[0].projects[0].id.clone();
    add_project_command(&mut data, &id, Some("Work"), "构建", "wsl", "make").unwrap();
    remove_project(&mut data, "app", Some("Work"), false).unwrap();
    assert_eq!(data.trash[0].commands.len(), 1);
    assert_eq!(data.trash[0].commands[0].name, "构建");
    restore_item(&mut data, &id).unwrap();
    let project = &data.groups[0].projects[0];
    assert_eq!(project.commands.len(), 1);
    assert_eq!(project.commands[0].env, "wsl");
}

/// Work 组保留、Personal 组已删除；trash 含两个项目项和一个分组项。
fn trash_data() -> ProjectData {
    let mut data = data();
    remove_project(&mut data, "app", Some("Work"), false).unwrap();
    remove_project(&mut data, "app", Some("Personal"), false).unwrap();
    remove_group(&mut data, "Personal", false).unwrap();
    data
}

#[test]
fn soft_delete_snapshots_project_and_group() {
    let data = trash_data();
    assert_eq!(data.groups.len(), 1);
    assert_eq!(data.trash.len(), 3);
    let projects: Vec<&DeletedItem> = data.trash.iter().filter(|item| !item.is_group()).collect();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].group, "Work");
    assert!(projects[0].deleted_at > 0);
    let group = data.trash.iter().find(|item| item.is_group()).unwrap();
    assert_eq!(group.name, "Personal");
    assert!(group.projects.is_empty());
}

#[test]
fn force_delete_bypasses_trash() {
    let mut data = data();
    let removed = remove_project(&mut data, "app", Some("Work"), true).unwrap();
    assert_eq!(removed.name, "app");
    assert!(data.trash.is_empty());
    remove_group(&mut data, "Personal", true).unwrap();
    assert!(data.trash.is_empty());
    assert_eq!(data.groups.len(), 1);
}

#[test]
fn group_soft_delete_includes_projects() {
    let mut data = data();
    remove_group(&mut data, "Work", false).unwrap();
    assert_eq!(data.groups.len(), 1);
    assert_eq!(data.trash.len(), 1);
    let item = &data.trash[0];
    assert!(item.is_group());
    assert_eq!(item.name, "Work");
    assert_eq!(item.projects.len(), 1);
    assert_eq!(item.projects[0].name, "app");
}

#[test]
fn restore_project_to_original_group_keeps_id() {
    let mut data = trash_data();
    let project_id = data.trash[0].id.clone();
    let message = restore_item(&mut data, &project_id).unwrap();
    assert_eq!(message, "已恢复项目: app");
    let group = &data.groups[0];
    assert_eq!(group.name, "Work");
    assert_eq!(group.projects[0].id, project_id);
    assert_eq!(group.projects[0].path, r"E:\dev\app");
    assert!(!data.trash.iter().any(|item| item.id == project_id));
}

#[test]
fn restore_project_to_first_group_when_original_missing() {
    let mut data = trash_data();
    let project_id = data.trash[1].id.clone();
    let message = restore_item(&mut data, &project_id).unwrap();
    // 原分组 Personal 已删除，恢复到第一个分组 Work（Work 自身项目已软删，故为 1 个）
    assert!(message.starts_with("已恢复项目: app（原分组 `Personal` 不存在"));
    assert_eq!(data.groups[0].projects.len(), 1);
    assert!(
        data.groups[0]
            .projects
            .iter()
            .any(|project| project.id == project_id)
    );
}

#[test]
fn restore_project_without_groups_fails() {
    let mut data = ProjectData::default();
    let mut project = Project::new("orphan", r"E:\o", "");
    let id = project.id.clone();
    data.trash
        .push(DeletedItem::from_project(&project, "Gone", 1));
    project.id = id.clone();
    assert!(restore_item(&mut data, &id).is_err());
}

#[test]
fn restore_project_regenerates_conflicting_id() {
    let mut data = ProjectData::default();
    add_group_with_alias(&mut data, "Work", "").unwrap();
    let mut project = Project::new("app", r"E:\dev\app", "");
    let id = project.id.clone();
    data.trash
        .push(DeletedItem::from_project(&project, "Work", 1));
    project.id = id.clone();
    // 同名同 id 项目已存在（如删除后又手工重建）
    data.groups[0].projects.push(project);
    restore_item(&mut data, &id).unwrap();
    let projects = &data.groups[0].projects;
    assert_eq!(projects.len(), 2);
    assert_ne!(projects[0].id, projects[1].id);
}

#[test]
fn restore_group_keeps_project_ids_and_renames_on_conflict() {
    let mut data = ProjectData::default();
    add_group_with_alias(&mut data, "Existing", "").unwrap();
    data.groups[0]
        .projects
        .push(Project::new("live", r"E:\live", ""));
    let inner = Project::new("inner", r"E:\inner", "");
    let inner_id = inner.id.clone();
    let mut group = Group::new("Archive");
    group.projects.push(inner);
    let item = DeletedItem::from_group(&group, 1);
    let group_id = item.id.clone();
    data.trash.push(item);
    // 恢复前手工创建同名分组
    add_group_with_alias(&mut data, "Archive", "").unwrap();
    let message = restore_item(&mut data, &group_id).unwrap();
    assert_eq!(
        message,
        "已恢复分组: Archive（原名已被占用，恢复为 Archive(恢复)）"
    );
    assert_eq!(data.groups.len(), 3);
    let restored = &data.groups[2];
    assert_eq!(restored.name, "Archive(恢复)");
    assert_eq!(restored.projects[0].id, inner_id);
}

#[test]
fn find_deleted_by_id_exact_prefix_and_at() {
    let mut data = data();
    let mut project = Project::new("app", r"E:\dev\app", "");
    project.id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into();
    data.trash
        .push(DeletedItem::from_project(&project, "Work", 1));
    project.id = "aaaabbbb-0000-1111-2222-333333333333".into();
    data.trash
        .push(DeletedItem::from_project(&project, "Work", 2));
    assert_eq!(find_deleted_by_id(&data, "@aaaaaaaa").unwrap(), 0);
    assert_eq!(find_deleted_by_id(&data, "AAAABBBB").unwrap(), 1);
    assert!(find_deleted_by_id(&data, "aaaa").is_err());
    assert!(find_deleted_by_id(&data, "zzzz").is_err());
    assert!(find_deleted_by_id(&data, "").is_err());
}

#[test]
fn delete_trash_item_and_empty() {
    let mut data = trash_data();
    let id = data.trash[0].id.clone();
    let removed = delete_trash_item(&mut data, &id).unwrap();
    assert_eq!(removed.id, id);
    assert_eq!(data.trash.len(), 2);
    empty_trash(&mut data);
    assert!(data.trash.is_empty());
}

#[test]
fn purge_expired_trash_removes_only_expired() {
    let mut data = trash_data();
    let now = current_unix_ts();
    let retention_secs = TRASH_RETENTION_DAYS * 86_400;
    // [0] 刚好超过保留期 -> 清理；[1] 未过期 -> 保留；[2] 旧数据(0) -> 保留
    data.trash[0].deleted_at = now - retention_secs - 1;
    data.trash[1].deleted_at = now - 86_400;
    data.trash[2].deleted_at = 0;
    assert!(purge_expired_trash(&mut data));
    assert_eq!(data.trash.len(), 2);
    assert!(
        !data
            .trash
            .iter()
            .any(|item| item.deleted_at == now - retention_secs - 1)
    );
    // 恰好在边界上 -> 保留
    data.trash[0].deleted_at = now - retention_secs;
    assert!(!purge_expired_trash(&mut data));
    assert_eq!(data.trash.len(), 2);
}

fn ssh_trash_snapshot(key: &str) -> (String, DeletedItem) {
    let mut project = Project::new("srv", "/opt/x", "");
    project.ssh_target = "abc@h".into();
    project.ssh_key_file = key.into();
    let id = project.id.clone();
    (id, DeletedItem::from_project(&project, "Work", 1))
}

#[test]
fn restore_with_id_conflict_keeps_key_reference_for_sweep() {
    // 设计 §11 必测：regenerate_id 后 JSON 中的 sshKeyFile 路径不变，
    // 引用驱动清理仍能覆盖该密钥文件，不会因 id 与文件名失配而误删。
    let mut data = ProjectData::default();
    add_group_with_alias(&mut data, "Work", "").unwrap();
    let (id, snapshot) = ssh_trash_snapshot("keys/srv.key");
    data.trash.push(snapshot);
    // 同 id 项目已存在（如删除后又手工重建）
    let mut live = Project::new("srv-live", "/opt/live", "");
    live.id = id.clone();
    data.groups[0].projects.push(live);

    restore_item(&mut data, &id).unwrap();
    let restored = data
        .groups
        .iter()
        .flat_map(|group| &group.projects)
        .find(|project| project.name == "srv")
        .unwrap();
    assert_ne!(restored.id, id, "id 冲突应已重生成");
    assert_eq!(restored.ssh_key_file, "keys/srv.key", "key 引用必须保留");
    let refs = crate::persist::referenced_key_files(&data);
    assert!(
        refs.contains(&"keys/srv.key".to_string()),
        "regenerate_id 后引用清理不得误删该密钥"
    );
}

#[test]
fn empty_trash_and_delete_trash_item_remove_key_files_on_disk() {
    // drop_key_file 走真实数据根：串行 + 临时 APPDATA，验证磁盘行为。
    let _lock = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_ops_test_{}", uuid::Uuid::new_v4()));
    let root = temp_appdata.join("project_center_dev");
    std::fs::create_dir_all(root.join("keys")).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let (id_a, snapshot_a) = ssh_trash_snapshot("keys/a.key");
    let (_id_b, snapshot_b) = ssh_trash_snapshot("keys/b.key");
    std::fs::write(root.join("keys").join("a.key"), "enc-a").unwrap();
    std::fs::write(root.join("keys").join("b.key"), "enc-b").unwrap();
    let mut data = ProjectData::default();
    add_group_with_alias(&mut data, "Work", "").unwrap();
    data.trash.push(snapshot_a);
    data.trash.push(snapshot_b);

    // 单项彻底删除：磁盘文件真删，无 pending 残留
    let removed = delete_trash_item(&mut data, &id_a).unwrap();
    assert_eq!(removed.name, "srv");
    assert!(!root.join("keys").join("a.key").exists());
    assert!(data.pending_key_deletes.is_empty());
    assert!(root.join("keys").join("b.key").exists());

    // 清空回收站：剩余 key 文件也真删
    empty_trash(&mut data);
    assert!(!root.join("keys").join("b.key").exists());
    assert!(data.pending_key_deletes.is_empty());

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn soft_delete_keeps_key_reference_and_purge_defers_failed_deletes() {
    let mut data = trash_data();
    // 快照带 SSH 秘密字段
    let mut ssh = Project::new("srv", "/opt/x", "");
    ssh.ssh_target = "abc@h".into();
    ssh.ssh_key_file = "keys/srv.key".into();
    ssh.ssh_password_enc = "PWENC".into();
    data.groups[0].projects.push(ssh);
    // 软删：key 引用随快照保留（可恢复），pending 不变
    let removed = remove_project(&mut data, "srv", None, false).unwrap();
    assert_eq!(data.trash.last().unwrap().ssh_key_file, "keys/srv.key");
    assert!(data.pending_key_deletes.is_empty());
    // drop_key_file 对不存在文件视为成功；对删除失败路径去重
    drop_key_file(&mut data, &removed.ssh_key_file);
    assert!(data.pending_key_deletes.is_empty(), "不存在的文件删除成功");
    data.pending_key_deletes.push("keys/srv.key".into());
    drop_key_file(&mut data, "keys/srv.key");
    assert_eq!(data.pending_key_deletes.len(), 1, "pending 去重");
}

#[test]
fn restore_project_renames_on_name_conflict() {
    let mut data = ProjectData::default();
    add_group_with_alias(&mut data, "Work", "").unwrap();
    add_project(&mut data, "Work", Project::new("app", r"E:\app", "")).unwrap();
    let snapshot = DeletedItem::from_project(&data.groups[0].projects[0], "Work", 1);
    data.trash.push(snapshot);
    let id = data.trash[0].id.clone();
    let message = restore_item(&mut data, &id).unwrap();
    assert!(message.contains("app(恢复)"));
    assert_eq!(data.groups[0].projects.len(), 2);
    assert_eq!(data.groups[0].projects[1].name, "app(恢复)");
    assert!(data.trash.is_empty());
}

#[test]
fn restore_group_suffix_increments_on_repeated_conflict() {
    let mut data = ProjectData::default();
    add_group_with_alias(&mut data, "Archive", "").unwrap();
    add_group_with_alias(&mut data, "Archive(恢复)", "").unwrap();
    let group = Group::new("Archive");
    data.trash.push(DeletedItem::from_group(&group, 1));
    let id = data.trash[0].id.clone();
    restore_item(&mut data, &id).unwrap();
    assert_eq!(data.groups[2].name, "Archive(恢复)2");
}

#[test]
fn restore_item_failure_keeps_trash_item() {
    let mut data = ProjectData::default();
    let project = Project::new("x", r"E:\x", "");
    data.trash
        .push(DeletedItem::from_project(&project, "Work", 1));
    let id = data.trash[0].id.clone();
    assert!(restore_item(&mut data, &id).is_err());
    assert_eq!(data.trash.len(), 1, "恢复失败时回收站项应保持原样");
}

#[test]
fn restore_group_ids_conflict_regenerated() {
    let mut data = ProjectData::default();
    add_group_with_alias(&mut data, "Work", "").unwrap();
    let existing = Project::new("live", r"E:\live", "");
    data.groups[0].projects.push(existing);
    // 回收站分组里包含与现有项目同 id 的项目
    let mut group = Group::new("Archive");
    group.projects.push(data.groups[0].projects[0].clone());
    let item = DeletedItem::from_group(&group, 1);
    let group_id = item.id.clone();
    data.trash.push(item);
    restore_item(&mut data, &group_id).unwrap();
    let restored = &data.groups[1];
    assert_eq!(restored.projects.len(), 1);
    assert_ne!(restored.projects[0].id, data.groups[0].projects[0].id);
}
