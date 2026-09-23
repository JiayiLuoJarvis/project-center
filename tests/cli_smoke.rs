use assert_cmd::Command;
use serde_json::Value;

fn pcs() -> Command {
    Command::cargo_bin("pcs").expect("pcs binary")
}

#[test]
fn help_exits_zero() {
    pcs().arg("--help").assert().success();
}

#[test]
fn ls_empty_data_dir_succeeds() {
    let dir = std::env::temp_dir().join(format!(
        "pcs_smoke_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    pcs()
        .env("PCS_DATA_DIR", &dir)
        .env("PCS_CONFIG_PATH", dir.join("config.json"))
        .arg("ls")
        .assert()
        .success();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_git_remote_and_notes_persist() {
    let dir = std::env::temp_dir().join(format!(
        "pcs_smoke_meta_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let env = |cmd: &mut Command| {
        cmd.env("PCS_DATA_DIR", &dir)
            .env("PCS_CONFIG_PATH", dir.join("config.json"));
    };
    let mut g = pcs();
    env(&mut g);
    g.args(["group", "add", "Work"]).assert().success();
    let mut add = pcs();
    env(&mut add);
    add.args([
        "add",
        "meta-app",
        "--group",
        "Work",
        "--wsl-path",
        "/mnt/e/meta-app",
        "--git-remote",
        "https://example.com/meta.git",
        "--notes",
        "初值",
    ])
    .assert()
    .success();
    let mut edit = pcs();
    env(&mut edit);
    edit.args([
        "edit",
        "meta-app",
        "--group",
        "Work",
        "--git-remote",
        "https://example.com/meta2.git",
        "--notes",
        "更新备注",
    ])
    .assert()
    .success();
    let json = std::fs::read_to_string(dir.join("projects.json")).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    let p = &v["groups"][0]["projects"][0];
    assert_eq!(p["gitRemote"], "https://example.com/meta2.git");
    assert_eq!(p["notes"], "更新备注");
    let _ = std::fs::remove_dir_all(&dir);
}
