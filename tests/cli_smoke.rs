use assert_cmd::Command;

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
