//! Run only against a disposable, loopback MySQL datadir inside the supplied QA root.
use std::{path::PathBuf, sync::Arc};
use superlink_app::{
    Credentials, ProfileService, QueryCancel, QueryService, table_design::DesignEdit,
};
use superlink_domain::{ConnectionConfig, Profile};
use superlink_infra::{LocalVault, Store};
use superlink_sql::{
    ColumnDefinition,
    design::{DesignDraft, DesignField},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("QA root required")?).canonicalize()?;
    if !root.to_string_lossy().contains("superlink-sql-gonavi-qa-") {
        return Err("isolated QA workspace required".into());
    }
    let port: u16 = std::env::args()
        .nth(2)
        .ok_or("loopback port required")?
        .parse()?;
    let service = QueryService::new(PathBuf::new());
    let credentials = Credentials::default();
    let mut profile = Profile {
        id: "field-designer-mysql-qa".into(),
        name: "MySQL 字段验收".into(),
        group: "隔离验收".into(),
        revision: 1,
        config: ConnectionConfig {
            kind: "mysql".into(),
            host: "127.0.0.1".into(),
            port,
            user: "root".into(),
            query_timeout: 10,
            ..Default::default()
        },
        ..Default::default()
    };
    let directory = service.execute(&profile, &credentials, "SELECT @@datadir", 1)?;
    let actual =
        PathBuf::from(directory.rows[0][0].as_str().ok_or("datadir missing")?).canonicalize()?;
    if actual != root.join("mysql-data").canonicalize()? {
        return Err("server does not belong to disposable QA root".into());
    }
    service.execute(
        &profile,
        &credentials,
        "CREATE DATABASE superlink_field_qa",
        1,
    )?;
    profile.config.database = "superlink_field_qa".into();
    service.execute(&profile, &credentials, "CREATE TABLE casbin_rules(id BIGINT PRIMARY KEY AUTO_INCREMENT,ptype VARCHAR(24) NOT NULL DEFAULT '' COMMENT '规则类型',v0 VARCHAR(100) NOT NULL DEFAULT '',v1 VARCHAR(100) NOT NULL DEFAULT X'785c79',v2 VARCHAR(100) NOT NULL DEFAULT '',v3 VARCHAR(100) NOT NULL DEFAULT '',v4 VARCHAR(100) NOT NULL DEFAULT '',v5 VARCHAR(100) NOT NULL DEFAULT '') CHARACTER SET utf8mb4 COLLATE utf8mb4_bin COMMENT='初始备注'; INSERT INTO casbin_rules(ptype,v0) VALUES('p','保留中文')", 1)?;
    let cancel = QueryCancel::new();
    let before = service.design_snapshot(
        &profile,
        &credentials,
        "superlink_field_qa",
        "casbin_rules",
        &cancel,
    )?;
    assert_eq!(before.columns.len(), 8);
    assert_eq!(before.comment, "初始备注");
    assert_eq!(before.columns[1].comment, "规则类型");
    let mut draft = DesignDraft::from(&before);
    draft.comment = "字段设计器验收\\目录'备注".into();
    draft.fields[1].column.name = "rule_type".into();
    draft.fields[1].column.data_type = "varchar(64)".into();
    draft.fields[1].column.comment = "更新注释".into();
    draft.fields[0].column.data_type = "bigint unsigned".into();
    draft.fields[0].column.extra.clear();
    let moved = draft.fields.remove(7);
    draft.fields.insert(2, moved);
    draft.fields.push(DesignField {
        original: None,
        column: ColumnDefinition {
            name: "enabled".into(),
            data_type: "tinyint unsigned".into(),
            nullable: "NO".into(),
            has_default: true,
            default: Some("1".into()),
            comment: "新字段".into(),
            ..Default::default()
        },
    });
    let saved = service.save_design(
        &profile,
        &credentials,
        "superlink_field_qa",
        DesignEdit {
            table: "casbin_rules",
            before: &before,
            draft: &draft,
        },
        &cancel,
    )?;
    assert_eq!(saved.columns[1].name, "rule_type");
    assert_eq!(saved.columns[1].data_type, "varchar(64)");
    assert_eq!(saved.columns[1].charset, before.columns[1].charset);
    assert_eq!(saved.columns[1].collation, before.columns[1].collation);
    assert_eq!(saved.columns[1].default, before.columns[1].default);
    assert_eq!(saved.columns[1].comment, "更新注释");
    assert_eq!(saved.columns[2].name, "v5");
    assert_eq!(saved.comment, draft.comment);
    assert!(saved.columns[0].data_type.contains("unsigned"));
    assert!(!saved.columns[0].extra.contains("auto_increment"));
    let mut enable = DesignDraft::from(&saved);
    enable.fields[0].column.extra = "auto_increment".into();
    let enabled = service.save_design(
        &profile,
        &credentials,
        "superlink_field_qa",
        DesignEdit {
            table: "casbin_rules",
            before: &saved,
            draft: &enable,
        },
        &cancel,
    )?;
    assert!(enabled.columns[0].extra.contains("auto_increment"));
    let mut switch = DesignDraft::from(&enabled);
    switch.fields[0].column.extra.clear();
    switch.fields[0].column.key.clear();
    switch.fields.last_mut().unwrap().column.key = "PRI".into();
    let switched = service.save_design(
        &profile,
        &credentials,
        "superlink_field_qa",
        DesignEdit {
            table: "casbin_rules",
            before: &enabled,
            draft: &switch,
        },
        &cancel,
    )?;
    assert_eq!(switched.columns.last().unwrap().key, "PRI");
    let restore = DesignDraft::from(&enabled);
    let enabled = service.save_design(
        &profile,
        &credentials,
        "superlink_field_qa",
        DesignEdit {
            table: "casbin_rules",
            before: &switched,
            draft: &restore,
        },
        &cancel,
    )?;
    let copy_object =
        superlink_app::table_design::design_object("mysql", "superlink_field_qa", "copied_fields");
    let copy_sql = superlink_sql::build_create_table("mysql", &copy_object, &enabled.columns)?;
    service.execute_design_sql(
        &profile,
        &credentials,
        "superlink_field_qa",
        &[copy_sql],
        &cancel,
    )?;
    let copied = service.design_snapshot(
        &profile,
        &credentials,
        "superlink_field_qa",
        "copied_fields",
        &cancel,
    )?;
    assert_eq!(copied.columns, enabled.columns);
    let mut invalid = DesignDraft::from(&enabled);
    invalid.fields[1].column.name = "should_rollback".into();
    invalid.fields[1].column.data_type = "int".into();
    assert!(
        service
            .save_design(
                &profile,
                &credentials,
                "superlink_field_qa",
                DesignEdit {
                    table: "casbin_rules",
                    before: &enabled,
                    draft: &invalid
                },
                &cancel
            )
            .is_err()
    );
    assert_eq!(
        service.design_snapshot(
            &profile,
            &credentials,
            "superlink_field_qa",
            "casbin_rules",
            &cancel
        )?,
        enabled
    );
    let data = service.execute(
        &profile,
        &credentials,
        "SELECT COUNT(*),MIN(v0),MIN(enabled) FROM casbin_rules",
        1,
    )?;
    assert_eq!(data.rows[0][0].as_i64(), Some(1));
    assert_eq!(data.rows[0][1], "保留中文");
    assert_eq!(data.rows[0][2].as_i64(), Some(1));
    service.execute(
        &profile,
        &credentials,
        "CREATE TABLE key_order(b INT NOT NULL,a INT NOT NULL,PRIMARY KEY(a,b))",
        1,
    )?;
    let key_before = service.design_snapshot(
        &profile,
        &credentials,
        "superlink_field_qa",
        "key_order",
        &cancel,
    )?;
    let mut key_draft = DesignDraft::from(&key_before);
    key_draft.fields[1].column.name = "a2".into();
    key_draft.fields.swap(0, 1);
    service.save_design(
        &profile,
        &credentials,
        "superlink_field_qa",
        DesignEdit {
            table: "key_order",
            before: &key_before,
            draft: &key_draft,
        },
        &cancel,
    )?;
    let key_order = service.execute(&profile, &credentials, "SELECT COLUMN_NAME FROM information_schema.STATISTICS WHERE TABLE_SCHEMA='superlink_field_qa' AND TABLE_NAME='key_order' AND INDEX_NAME='PRIMARY' ORDER BY SEQ_IN_INDEX", 2)?;
    assert_eq!(key_order.rows[0][0], "a2");
    assert_eq!(key_order.rows[1][0], "b");
    let data_root = root.join("data");
    let store = Arc::new(Store::open(data_root.join("state.db"))?);
    let profiles = ProfileService::new(store, Arc::new(LocalVault::new(&data_root)));
    profile.revision = profiles.get(&profile.id).map(|p| p.revision).unwrap_or(0);
    profiles.save(profile, &credentials)?;
    println!(
        "MySQL designer: attributes, order, comments, defaults, charset, auto increment, atomic failure and original data verified."
    );
    Ok(())
}
