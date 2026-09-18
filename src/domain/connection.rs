#![allow(dead_code)]

use std::collections::HashMap;

use super::{Error, Result};
use crate::domain::models::{Connection, DeletedItem, Endpoint, Project, ProjectData};

/// 新建连接的用户输入（TUI 表单 / 迁移）；秘密不在这里，走 `ConnectionPatch`。
pub struct ConnectionDraft {
    pub name: String,
    pub user: String,
    pub host: String,
    pub port: u16,
}

/// 编辑连接：None = 不改。秘密字段传 DPAPI 密文（加密由 cli/tui 层完成）。
#[derive(Default)]
pub struct ConnectionPatch {
    pub name: Option<String>,
    pub user: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub key: KeyChange,
    pub password_enc: Option<String>,
    pub key_pass_enc: Option<String>,
}

#[derive(Default)]
pub enum KeyChange {
    #[default]
    Keep,
    /// 移除 sidecar 引用 + 来源路径 + 私钥口令；旧路径记入 pendingKeyDeletes。
    Clear,
    /// 已由调用方写好 sidecar：`file` = `keys/<connectionId>.key`，`source` = 来源路径。
    Import { file: String, source: String },
}

/// 旧数据里一条项目级 SSH 记录（由 persist 从 JSON Value 采集）。
pub struct LegacySsh {
    pub locus: Locus,
    pub target: String,
    pub key_file: String,
    pub key_path: String,
    pub password_enc: String,
    pub key_pass_enc: String,
}

/// 记录位置：与 serde 解析出的结构按位置一一对应。
pub enum Locus {
    Live { gi: usize, pi: usize },
    Trashed { ti: usize },
    TrashedInGroup { ti: usize, pi: usize },
}

pub struct KeyRename {
    pub connection_id: String,
    pub from: String,
    pub to: String,
}

pub struct LegacyMigration {
    pub changed: bool,
    pub key_renames: Vec<KeyRename>,
}

impl ProjectData {
    /// 名称（大小写不敏感）或 `@<id>`（唯一前缀）。
    pub fn find_connection(&self, selector: &str) -> Result<usize> {
        let selector = selector.trim();
        if let Some(id) = selector.strip_prefix('@') {
            return find_connection_by_id(self, id);
        }
        let matches: Vec<usize> = self
            .connections
            .iter()
            .enumerate()
            .filter(|(_, conn)| conn.name.eq_ignore_ascii_case(selector))
            .map(|(index, _)| index)
            .collect();
        match matches.as_slice() {
            [index] => Ok(*index),
            [] => Err(Error::ConnectionNotFound {
                name: selector.to_string(),
            }),
            _ => Err(Error::ConnectionAmbiguous {
                name: selector.to_string(),
            }),
        }
    }

    /// 精确 id（大小写不敏感）。askpass / 启动用。
    pub fn connection(&self, id: &str) -> Option<&Connection> {
        let id = id.trim();
        if id.is_empty() {
            return None;
        }
        self.connections
            .iter()
            .find(|conn| conn.id.eq_ignore_ascii_case(id))
    }

    /// 项目引用的连接：空 id → Err(ProjectNotSsh)；有 id 但解析不到 → Err(ConnectionMissing)。
    pub fn connection_of(&self, project: &Project) -> Result<&Connection> {
        let Some(id) = project.connection_id_opt() else {
            return Err(Error::ProjectNotSsh {
                name: project.name.clone(),
            });
        };
        self.connection(id).ok_or_else(|| Error::ConnectionMissing {
            project: project.name.clone(),
        })
    }

    /// groups + trash（项目项 + 分组快照内项目）中引用该连接的项目数。
    pub fn connection_refs(&self, id: &str) -> usize {
        let id = id.trim();
        if id.is_empty() {
            return 0;
        }
        let live = self
            .groups
            .iter()
            .flat_map(|group| group.projects.iter())
            .filter(|project| project.connection_id.eq_ignore_ascii_case(id))
            .count();
        let trash = self
            .trash
            .iter()
            .map(|item| trash_refs(item, id))
            .sum::<usize>();
        live + trash
    }

    /// 端点复用查找（`Endpoint::key` 相等）。
    pub fn connection_by_endpoint(&self, endpoint: &Endpoint) -> Option<&Connection> {
        let key = endpoint.key();
        self.connections
            .iter()
            .find(|conn| conn.endpoint().key() == key)
    }

    /// 校验 name 非空唯一、host 非空且不含 `@`、user 不含 `@`；返回新 id。
    pub fn add_connection(&mut self, draft: ConnectionDraft, now: &str) -> Result<String> {
        self.validate_draft(&draft.name, &draft.user, &draft.host, draft.port, None)?;
        let endpoint = Endpoint {
            user: draft.user.trim().to_string(),
            host: draft.host.trim().to_string(),
            port: draft.port,
        };
        let conn = Connection::new(draft.name.trim(), &endpoint, now);
        let id = conn.id.clone();
        self.connections.push(conn);
        Ok(id)
    }

    /// 同校验（唯一性排除自身）；`KeyChange::Clear` 同时清空 key_pass_enc。
    pub fn edit_connection(&mut self, id: &str, patch: ConnectionPatch, now: &str) -> Result<()> {
        let idx = self
            .connections
            .iter()
            .position(|conn| conn.id.eq_ignore_ascii_case(id.trim()))
            .ok_or_else(|| Error::ConnectionNotFound {
                name: id.to_string(),
            })?;
        let current = &self.connections[idx];
        let name = patch.name.as_deref().unwrap_or(&current.name).to_string();
        let user = patch.user.as_deref().unwrap_or(&current.user).to_string();
        let host = patch.host.as_deref().unwrap_or(&current.host).to_string();
        let port = patch.port.unwrap_or(current.port);
        self.validate_draft(&name, &user, &host, port, Some(idx))?;
        let mut retire = None;
        {
            let conn = &mut self.connections[idx];
            conn.name = name.trim().to_string();
            conn.user = user.trim().to_string();
            conn.host = host.trim().to_string();
            conn.port = port;
            match patch.key {
                KeyChange::Keep => {}
                KeyChange::Clear => {
                    retire = Some(conn.ssh_key_file.clone());
                    conn.ssh_key_file.clear();
                    conn.ssh_key_path.clear();
                    conn.ssh_key_pass_enc.clear();
                }
                KeyChange::Import { file, source } => {
                    if conn.ssh_key_file != file {
                        retire = Some(conn.ssh_key_file.clone());
                    }
                    conn.ssh_key_file = file;
                    conn.ssh_key_path = source;
                }
            }
            if let Some(password_enc) = patch.password_enc {
                conn.ssh_password_enc = password_enc;
            }
            if let Some(key_pass_enc) = patch.key_pass_enc {
                conn.ssh_key_pass_enc = key_pass_enc;
            }
            conn.updated_at = now.to_string();
        }
        if let Some(old) = retire {
            queue_pending_key(self, old);
        }
        Ok(())
    }

    /// 有引用 → Err(ConnectionInUse)；成功则摘除连接，sidecar 由调用方退役。
    pub fn remove_connection(&mut self, id: &str) -> Result<Connection> {
        let idx = self
            .connections
            .iter()
            .position(|conn| conn.id.eq_ignore_ascii_case(id.trim()))
            .ok_or_else(|| Error::ConnectionNotFound {
                name: id.to_string(),
            })?;
        let refs = self.connection_refs(&self.connections[idx].id);
        if refs > 0 {
            return Err(Error::ConnectionInUse {
                name: self.connections[idx].name.clone(),
                count: refs,
            });
        }
        Ok(self.connections.remove(idx))
    }

    /// `--ssh` 与迁移共用：命中端点即复用，否则按 host / `host (user)` / 追加序号起名新建。
    pub fn find_or_create_connection(&mut self, endpoint: &Endpoint, now: &str) -> String {
        if let Some(conn) = self.connection_by_endpoint(endpoint) {
            return conn.id.clone();
        }
        let name = self.unique_connection_name(&endpoint.host, &endpoint.user);
        let conn = Connection::new(name, endpoint, now);
        let id = conn.id.clone();
        self.connections.push(conn);
        id
    }

    /// 项目变为 SSH：connection_id + path=remote，wsl_path 清空。id 不存在 → Err。
    pub fn attach_connection(
        &mut self,
        gi: usize,
        pi: usize,
        connection_id: &str,
        remote_path: &str,
    ) -> Result<()> {
        let id = connection_id.trim();
        if self.connection(id).is_none() {
            return Err(Error::ConnectionNotFound {
                name: id.to_string(),
            });
        }
        let Some(project) = self
            .groups
            .get_mut(gi)
            .and_then(|group| group.projects.get_mut(pi))
        else {
            return Err(Error::ProjectNotFound {
                name: String::new(),
            });
        };
        project.connection_id = id.to_string();
        project.path = remote_path.trim().to_string();
        project.wsl_path.clear();
        Ok(())
    }

    /// 项目变回本地：清 connection_id；path 保留。连接不删。
    pub fn detach_connection(&mut self, gi: usize, pi: usize) {
        if let Some(project) = self
            .groups
            .get_mut(gi)
            .and_then(|group| group.projects.get_mut(pi))
        {
            project.connection_id.clear();
        }
    }

    /// `pin reset --force`：清所有连接的 key/来源/密码/口令与 pendingKeyDeletes。
    pub fn clear_all_connection_secrets(&mut self) {
        for conn in &mut self.connections {
            conn.ssh_key_file.clear();
            conn.ssh_key_path.clear();
            conn.ssh_password_enc.clear();
            conn.ssh_key_pass_enc.clear();
        }
        self.pending_key_deletes.clear();
    }

    /// 幂等：records 为空即 `changed=false`。
    pub fn absorb_legacy_ssh(&mut self, records: Vec<LegacySsh>, now: &str) -> LegacyMigration {
        if records.is_empty() {
            return LegacyMigration {
                changed: false,
                key_renames: Vec::new(),
            };
        }
        let mut buckets: HashMap<String, Vec<LegacySsh>> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for record in records {
            let Ok(endpoint) = Endpoint::parse(&record.target) else {
                continue;
            };
            let key = endpoint.key();
            if !buckets.contains_key(&key) {
                order.push(key.clone());
            }
            buckets.entry(key).or_default().push(record);
        }
        if buckets.is_empty() {
            return LegacyMigration {
                changed: false,
                key_renames: Vec::new(),
            };
        }
        let mut key_renames = Vec::new();
        for key in order {
            let recs = buckets.remove(&key).unwrap_or_default();
            let Some(first) = recs.first() else {
                continue;
            };
            let Ok(endpoint) = Endpoint::parse(&first.target) else {
                continue;
            };
            let cid = self.find_or_create_connection(&endpoint, now);
            let existing_key = self
                .connection(&cid)
                .map(|conn| conn.ssh_key_file.clone())
                .unwrap_or_default();
            let mut auth = AuthTuple::default();
            let mut seen_keys = Vec::new();
            if !existing_key.trim().is_empty() {
                seen_keys.push(existing_key);
            }
            for record in &recs {
                if !record.key_file.trim().is_empty() {
                    seen_keys.push(record.key_file.clone());
                }
                if has_any_auth(record) {
                    auth = AuthTuple {
                        key_file: record.key_file.clone(),
                        key_path: record.key_path.clone(),
                        password_enc: record.password_enc.clone(),
                        key_pass_enc: record.key_pass_enc.clone(),
                    };
                }
            }
            let winner_key = auth.key_file.trim().to_string();
            for old in seen_keys {
                if old != winner_key {
                    queue_pending_key(self, old);
                }
            }
            if let Some(conn) = self
                .connections
                .iter_mut()
                .find(|conn| conn.id.eq_ignore_ascii_case(&cid))
            {
                conn.ssh_key_file = auth.key_file;
                conn.ssh_key_path = auth.key_path;
                conn.ssh_password_enc = auth.password_enc;
                conn.ssh_key_pass_enc = auth.key_pass_enc;
                conn.updated_at = now.to_string();
                let expected = format!("keys/{}.key", conn.id);
                if !conn.ssh_key_file.trim().is_empty() && conn.ssh_key_file != expected {
                    key_renames.push(KeyRename {
                        connection_id: conn.id.clone(),
                        from: conn.ssh_key_file.clone(),
                        to: expected,
                    });
                }
            }
            for record in recs {
                attach_legacy(self, &record.locus, &cid);
            }
        }
        LegacyMigration {
            changed: true,
            key_renames,
        }
    }

    /// persist 改名成功后回写；失败则保持旧路径。
    pub(crate) fn relink_connection_key(&mut self, connection_id: &str, relative: &str) {
        if let Some(conn) = self
            .connections
            .iter_mut()
            .find(|conn| conn.id.eq_ignore_ascii_case(connection_id))
        {
            conn.ssh_key_file = relative.to_string();
        }
    }

    fn unique_connection_name(&self, host: &str, user: &str) -> String {
        if !self.connection_name_taken(host, None) {
            return host.to_string();
        }
        let with_user = if user.trim().is_empty() {
            None
        } else {
            Some(format!("{host} ({user})"))
        };
        if let Some(name) = &with_user {
            if !self.connection_name_taken(name, None) {
                return name.clone();
            }
        }
        let base = with_user.unwrap_or_else(|| host.to_string());
        let mut n = 2u32;
        loop {
            let candidate = format!("{base} {n}");
            if !self.connection_name_taken(&candidate, None) {
                return candidate;
            }
            n += 1;
        }
    }

    fn connection_name_taken(&self, name: &str, skip: Option<usize>) -> bool {
        self.connections
            .iter()
            .enumerate()
            .any(|(index, conn)| skip != Some(index) && conn.name.eq_ignore_ascii_case(name))
    }

    fn validate_draft(
        &self,
        name: &str,
        user: &str,
        host: &str,
        port: u16,
        skip: Option<usize>,
    ) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::ConnectionNameEmpty);
        }
        if self.connection_name_taken(name, skip) {
            return Err(Error::ConnectionExists {
                name: name.to_string(),
            });
        }
        let host = host.trim();
        if host.is_empty() {
            return Err(Error::ConnectionHostEmpty);
        }
        if host.contains('@') || user.contains('@') {
            return Err(Error::ConnectionAtSign);
        }
        if host.contains(':') {
            return Err(Error::ConnectionIpv6);
        }
        if port == 0 {
            return Err(Error::ConnectionPortInvalid);
        }
        Ok(())
    }
}

#[derive(Default)]
struct AuthTuple {
    key_file: String,
    key_path: String,
    password_enc: String,
    key_pass_enc: String,
}

fn find_connection_by_id(data: &ProjectData, id: &str) -> Result<usize> {
    let id = id.trim();
    if id.is_empty() {
        return Err(Error::ConnectionNotFound {
            name: String::new(),
        });
    }
    let matches: Vec<usize> = data
        .connections
        .iter()
        .enumerate()
        .filter(|(_, conn)| {
            conn.id.eq_ignore_ascii_case(id)
                || conn
                    .id
                    .get(..id.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(id))
        })
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => Err(Error::ConnectionNotFound {
            name: id.to_string(),
        }),
        _ => Err(Error::ConnectionAmbiguous {
            name: id.to_string(),
        }),
    }
}

fn trash_refs(item: &DeletedItem, id: &str) -> usize {
    if item.is_group() {
        item.projects
            .iter()
            .filter(|project| project.connection_id.eq_ignore_ascii_case(id))
            .count()
    } else if item.connection_id.eq_ignore_ascii_case(id) {
        1
    } else {
        0
    }
}

fn has_any_auth(record: &LegacySsh) -> bool {
    !record.key_file.trim().is_empty()
        || !record.key_path.trim().is_empty()
        || !record.password_enc.trim().is_empty()
        || !record.key_pass_enc.trim().is_empty()
}

fn queue_pending_key(data: &mut ProjectData, path: String) {
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    if data
        .pending_key_deletes
        .iter()
        .any(|existing| existing == path)
    {
        return;
    }
    data.pending_key_deletes.push(path.to_string());
}

fn clear_project_legacy_ssh(project: &mut Project) {
    project.ssh_target.clear();
    project.ssh_key_file.clear();
    project.ssh_key_path.clear();
    project.ssh_password_enc.clear();
    project.ssh_key_pass_enc.clear();
}

fn clear_deleted_legacy_ssh(item: &mut DeletedItem) {
    item.ssh_target.clear();
    item.ssh_key_file.clear();
    item.ssh_key_path.clear();
    item.ssh_password_enc.clear();
    item.ssh_key_pass_enc.clear();
}

fn attach_legacy(data: &mut ProjectData, locus: &Locus, connection_id: &str) {
    match locus {
        Locus::Live { gi, pi } => {
            if let Some(project) = data
                .groups
                .get_mut(*gi)
                .and_then(|group| group.projects.get_mut(*pi))
            {
                project.connection_id = connection_id.to_string();
                clear_project_legacy_ssh(project);
            }
        }
        Locus::Trashed { ti } => {
            if let Some(item) = data.trash.get_mut(*ti) {
                item.connection_id = connection_id.to_string();
                clear_deleted_legacy_ssh(item);
            }
        }
        Locus::TrashedInGroup { ti, pi } => {
            if let Some(project) = data
                .trash
                .get_mut(*ti)
                .and_then(|item| item.projects.get_mut(*pi))
            {
                project.connection_id = connection_id.to_string();
                clear_project_legacy_ssh(project);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{DeletedItem, Group, Project};

    fn now() -> &'static str {
        "2026-09-18T00:00:00Z"
    }

    fn endpoint(target: &str) -> Endpoint {
        Endpoint::parse(target).unwrap()
    }

    fn live_record(gi: usize, pi: usize, target: &str) -> LegacySsh {
        LegacySsh {
            locus: Locus::Live { gi, pi },
            target: target.into(),
            key_file: String::new(),
            key_path: String::new(),
            password_enc: String::new(),
            key_pass_enc: String::new(),
        }
    }

    #[test]
    fn find_or_create_dedupes_same_endpoint() {
        let mut data = ProjectData::default();
        let a = data.find_or_create_connection(&endpoint("abc@Host:22"), now());
        let b = data.find_or_create_connection(&endpoint("ABC@host"), now());
        assert_eq!(a, b);
        assert_eq!(data.connections.len(), 1);
        assert_eq!(data.connections[0].name, "Host");
        assert_eq!(data.connections[0].label(), "abc@Host:22");
        assert!(!data.connections[0].has_saved_secrets());
        assert!(!data.connections[0].has_key());
    }

    #[test]
    fn connection_crud_lookup_and_detach() {
        let mut data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![Project::new("app", "/opt/a", "")],
            }],
            ..Default::default()
        };
        let id = data
            .add_connection(
                ConnectionDraft {
                    name: "box".into(),
                    user: "abc".into(),
                    host: "h".into(),
                    port: 22,
                },
                now(),
            )
            .unwrap();
        assert_eq!(data.find_connection("BOX").unwrap(), 0);
        assert_eq!(data.find_connection(&format!("@{id}")).unwrap(), 0);
        assert_eq!(data.connection(&id).unwrap().userhost(), "abc@h");
        data.attach_connection(0, 0, &id, "/opt/remote").unwrap();
        assert_eq!(
            data.connection_of(&data.groups[0].projects[0]).unwrap().id,
            id
        );
        data.edit_connection(
            &id,
            ConnectionPatch {
                name: Some("box-2".into()),
                key: KeyChange::Import {
                    file: "keys/x.key".into(),
                    source: r"C:\k".into(),
                },
                password_enc: Some("PW".into()),
                ..Default::default()
            },
            now(),
        )
        .unwrap();
        assert_eq!(data.connections[0].name, "box-2");
        assert!(data.connections[0].has_key());
        assert!(data.connections[0].has_saved_secrets());
        data.edit_connection(
            &id,
            ConnectionPatch {
                key: KeyChange::Clear,
                ..Default::default()
            },
            now(),
        )
        .unwrap();
        assert!(!data.connections[0].has_key());
        assert_eq!(data.pending_key_deletes, vec!["keys/x.key"]);
        data.detach_connection(0, 0);
        assert!(data.groups[0].projects[0].connection_id.is_empty());
        assert!(matches!(
            data.connection_of(&data.groups[0].projects[0]),
            Err(Error::ProjectNotSsh { .. })
        ));
        data.clear_all_connection_secrets();
        assert!(data.pending_key_deletes.is_empty());
        assert!(data.connections[0].ssh_password_enc.is_empty());
    }

    #[test]
    fn absorb_legacy_merges_two_projects_same_endpoint() {
        let mut data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![
                    Project::new("one", "/opt/a", "").with_ssh_target("abc@h:22"),
                    Project::new("two", "/opt/b", "").with_ssh_target("ABC@H"),
                ],
            }],
            ..Default::default()
        };
        let mig = data.absorb_legacy_ssh(
            vec![live_record(0, 0, "abc@h:22"), live_record(0, 1, "ABC@H")],
            now(),
        );
        assert!(mig.changed);
        assert_eq!(data.connections.len(), 1);
        let cid = data.connections[0].id.clone();
        assert_eq!(data.groups[0].projects[0].connection_id, cid);
        assert_eq!(data.groups[0].projects[1].connection_id, cid);
        assert!(data.groups[0].projects[0].ssh_target.is_empty());
        assert!(data.groups[0].projects[1].ssh_target.is_empty());
        assert_eq!(data.connections[0].name, "h");
    }

    #[test]
    fn absorb_legacy_later_auth_wins_whole_tuple() {
        let mut data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![
                    {
                        let mut p = Project::new("one", "/opt/a", "").with_ssh_target("abc@h");
                        p.ssh_key_file = "keys/old.key".into();
                        p.ssh_password_enc = "PW1".into();
                        p
                    },
                    {
                        let mut p = Project::new("two", "/opt/b", "").with_ssh_target("abc@h:22");
                        p.ssh_password_enc = "PW2".into();
                        p
                    },
                ],
            }],
            ..Default::default()
        };
        let mut first = live_record(0, 0, "abc@h");
        first.key_file = "keys/old.key".into();
        first.password_enc = "PW1".into();
        let mut second = live_record(0, 1, "abc@h:22");
        second.password_enc = "PW2".into();
        let mig = data.absorb_legacy_ssh(vec![first, second], now());
        assert!(mig.changed);
        let conn = &data.connections[0];
        assert_eq!(conn.ssh_password_enc, "PW2");
        assert!(conn.ssh_key_file.is_empty());
        assert_eq!(data.pending_key_deletes, vec!["keys/old.key"]);
        assert!(mig.key_renames.is_empty());
    }

    #[test]
    fn remove_connection_refuses_when_groups_or_trash_reference() {
        let mut data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![Project::new("app", "/opt/a", "")],
            }],
            ..Default::default()
        };
        let cid = data.find_or_create_connection(&endpoint("abc@h"), now());
        data.attach_connection(0, 0, &cid, "/opt/a").unwrap();
        let err = data.remove_connection(&cid).unwrap_err();
        assert!(matches!(err, Error::ConnectionInUse { count: 1, .. }));

        crate::domain::remove_project(&mut data, "app", Some("G"), false).unwrap();
        let err = data.remove_connection(&cid).unwrap_err();
        assert!(matches!(err, Error::ConnectionInUse { count: 1, .. }));

        data.trash.clear();
        let removed = data.remove_connection(&cid).unwrap();
        assert_eq!(removed.id, cid);
        assert!(data.connections.is_empty());
    }

    #[test]
    fn connection_refs_counts_trash_snapshots() {
        let mut live = Project::new("live", "/opt/a", "");
        let mut trashed = Project::new("gone", "/opt/b", "");
        let mut inner = Project::new("inner", "/opt/c", "");
        live.connection_id = "CID".into();
        trashed.connection_id = "cid".into();
        inner.connection_id = "Cid".into();
        let data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![live],
            }],
            trash: vec![
                DeletedItem::from_project(&trashed, "G", 1),
                DeletedItem::from_group(
                    &Group {
                        name: "Old".into(),
                        alias: String::new(),
                        projects: vec![inner],
                    },
                    2,
                ),
            ],
            ..Default::default()
        };
        assert_eq!(data.connection_refs("cid"), 3);
    }

    #[test]
    fn absorb_legacy_attaches_trash_loci() {
        let live = Project::new("live", "/opt/a", "").with_ssh_target("abc@h");
        let gone = Project::new("gone", "/opt/b", "").with_ssh_target("abc@h");
        let inner = Project::new("inner", "/opt/c", "").with_ssh_target("abc@h");
        let mut data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![live],
            }],
            trash: vec![
                DeletedItem::from_project(&gone, "G", 1),
                DeletedItem::from_group(
                    &Group {
                        name: "Old".into(),
                        alias: String::new(),
                        projects: vec![inner],
                    },
                    2,
                ),
            ],
            ..Default::default()
        };
        let mig = data.absorb_legacy_ssh(
            vec![
                live_record(0, 0, "abc@h"),
                LegacySsh {
                    locus: Locus::Trashed { ti: 0 },
                    target: "abc@h".into(),
                    key_file: String::new(),
                    key_path: String::new(),
                    password_enc: String::new(),
                    key_pass_enc: String::new(),
                },
                LegacySsh {
                    locus: Locus::TrashedInGroup { ti: 1, pi: 0 },
                    target: "abc@h".into(),
                    key_file: String::new(),
                    key_path: String::new(),
                    password_enc: String::new(),
                    key_pass_enc: String::new(),
                },
            ],
            now(),
        );
        assert!(mig.changed);
        let cid = data.connections[0].id.clone();
        assert_eq!(data.groups[0].projects[0].connection_id, cid);
        assert_eq!(data.trash[0].connection_id, cid);
        assert_eq!(data.trash[1].projects[0].connection_id, cid);
        assert!(data.trash[0].ssh_target.is_empty());
        assert!(data.trash[1].projects[0].ssh_target.is_empty());
    }

    #[test]
    fn endpoint_parse_rejects_ipv6_at_and_bad_port() {
        assert!(matches!(
            Endpoint::parse("abc@::1"),
            Err(Error::ConnectionIpv6)
        ));
        assert!(matches!(
            Endpoint::parse("a@b@h"),
            Err(Error::ConnectionAtSign)
        ));
        assert!(matches!(
            Endpoint::parse("h:99999"),
            Err(Error::ConnectionPortInvalid)
        ));
        assert!(matches!(
            Endpoint::parse(""),
            Err(Error::ConnectionHostEmpty)
        ));
        let ep = Endpoint::parse("abc@h:2222").unwrap();
        assert_eq!(ep.user, "abc");
        assert_eq!(ep.host, "h");
        assert_eq!(ep.port, 2222);
        assert_eq!(ep.key(), "abc@h:2222");
    }

    #[test]
    fn absorb_plans_key_rename_then_relink() {
        let mut data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![{
                    let mut p = Project::new("one", "/opt/a", "").with_ssh_target("abc@h");
                    p.ssh_key_file = "keys/old-project.key".into();
                    p
                }],
            }],
            ..Default::default()
        };
        let mut record = live_record(0, 0, "abc@h");
        record.key_file = "keys/old-project.key".into();
        let mig = data.absorb_legacy_ssh(vec![record], now());
        assert_eq!(mig.key_renames.len(), 1);
        let cid = data.connections[0].id.clone();
        assert_eq!(mig.key_renames[0].connection_id, cid);
        assert_eq!(mig.key_renames[0].from, "keys/old-project.key");
        assert_eq!(mig.key_renames[0].to, format!("keys/{cid}.key"));
        assert_eq!(data.connections[0].ssh_key_file, "keys/old-project.key");
        data.relink_connection_key(&cid, &mig.key_renames[0].to);
        assert_eq!(
            data.connection(&cid).unwrap().ssh_key_file,
            format!("keys/{cid}.key")
        );
    }
}
