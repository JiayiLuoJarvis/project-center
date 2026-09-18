use crate::domain::{LegacySsh, Locus};

/// 从原始 JSON 采集仍带非空 `sshTarget` 的项目级 SSH 记录。
/// 只认遗留键名；`from_value` 之后未知键会被丢掉，所以必须先走 Value。
pub(crate) fn harvest_legacy_ssh(root: &serde_json::Value) -> Vec<LegacySsh> {
    let mut out = Vec::new();
    if let Some(groups) = root.get("groups").and_then(serde_json::Value::as_array) {
        for (gi, group) in groups.iter().enumerate() {
            if let Some(projects) = group.get("projects").and_then(serde_json::Value::as_array) {
                for (pi, project) in projects.iter().enumerate() {
                    if let Some(record) = record_from(project, Locus::Live { gi, pi }) {
                        out.push(record);
                    }
                }
            }
        }
    }
    if let Some(trash) = root.get("trash").and_then(serde_json::Value::as_array) {
        for (ti, item) in trash.iter().enumerate() {
            if let Some(record) = record_from(item, Locus::Trashed { ti }) {
                out.push(record);
            }
            if let Some(projects) = item.get("projects").and_then(serde_json::Value::as_array) {
                for (pi, project) in projects.iter().enumerate() {
                    if let Some(record) = record_from(project, Locus::TrashedInGroup { ti, pi }) {
                        out.push(record);
                    }
                }
            }
        }
    }
    out
}

fn record_from(obj: &serde_json::Value, locus: Locus) -> Option<LegacySsh> {
    let target = json_str(obj, "sshTarget");
    if target.trim().is_empty() {
        return None;
    }
    Some(LegacySsh {
        locus,
        target,
        key_file: json_str(obj, "sshKeyFile"),
        key_path: json_str(obj, "sshKeyPath"),
        password_enc: json_str(obj, "sshPasswordEnc"),
        key_pass_enc: json_str(obj, "sshKeyPassEnc"),
    })
}

fn json_str(obj: &serde_json::Value, key: &str) -> String {
    obj.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harvests_groups_trash_and_nested_when_ssh_target_nonempty() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{
              "groups":[
                {"name":"G","projects":[
                  {"name":"live","sshTarget":"abc@h","sshKeyFile":"keys/a.key","sshPasswordEnc":"PW"},
                  {"name":"local","path":"E:\\x"}
                ]}
              ],
              "trash":[
                {"type":"project","name":"gone","sshTarget":"abc@h:22","sshKeyPath":"C:\\k","sshKeyPassEnc":"KP"},
                {"type":"group","name":"Old","projects":[
                  {"name":"inner","sshTarget":"  xyz@z  "}
                ]}
              ]
            }"#,
        )
        .unwrap();
        let records = harvest_legacy_ssh(&value);
        assert_eq!(records.len(), 3);
        assert!(matches!(records[0].locus, Locus::Live { gi: 0, pi: 0 }));
        assert_eq!(records[0].target, "abc@h");
        assert_eq!(records[0].key_file, "keys/a.key");
        assert_eq!(records[0].password_enc, "PW");
        assert!(matches!(records[1].locus, Locus::Trashed { ti: 0 }));
        assert_eq!(records[1].target, "abc@h:22");
        assert_eq!(records[1].key_path, r"C:\k");
        assert_eq!(records[1].key_pass_enc, "KP");
        assert!(matches!(
            records[2].locus,
            Locus::TrashedInGroup { ti: 1, pi: 0 }
        ));
        assert_eq!(records[2].target, "  xyz@z  ");
    }

    #[test]
    fn skips_empty_or_non_string_ssh_target() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"groups":[{"projects":[
              {"name":"blank","sshTarget":"   "},
              {"name":"num","sshTarget":1}
            ]}]}"#,
        )
        .unwrap();
        assert!(harvest_legacy_ssh(&value).is_empty());
    }
}
