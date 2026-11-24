use super::*;
fn snapshot() -> DesignSnapshot {
    DesignSnapshot {
        columns: vec![
            ColumnDefinition {
                name: "id".into(),
                data_type: "bigint".into(),
                nullable: "NO".into(),
                key: "PRI".into(),
                extra: "auto_increment".into(),
                ..Default::default()
            },
            ColumnDefinition {
                name: "title".into(),
                data_type: "varchar(24)".into(),
                nullable: "NO".into(),
                default: Some("''".into()),
                has_default: true,
                comment: "原注释".into(),
                charset: "utf8mb4".into(),
                collation: "utf8mb4_general_ci".into(),
                ..Default::default()
            },
        ],
        comment: "原表备注".into(),
    }
}
#[test]
fn mysql_edits_preserve_defaults_charset_and_comment_in_one_alter() {
    let before = snapshot();
    let mut draft = DesignDraft::from(&before);
    draft.fields[1].column.name = "title'`new".into();
    draft.fields[1].column.data_type = "varchar(64)".into();
    draft.fields.swap(0, 1);
    draft.comment = "新'备注\\x".into();
    let sql = design_sql(
        "mysql",
        &Object {
            name: "items".into(),
            ..Default::default()
        },
        &before,
        &draft,
    )
    .unwrap();
    assert_eq!(sql.len(), 1);
    assert!(sql[0].contains("CHANGE COLUMN `title` `title'``new` varchar(64) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL DEFAULT '' COMMENT"));
    assert!(sql[0].contains(" FIRST"));
    assert!(sql[0].contains("AFTER `title'``new`"));
    assert!(sql[0].contains("COMMENT = "));
    assert!(sql[0].contains("COMMENT = '新''备注\\\\x'"));
    assert!(!sql[0].contains("CONVERT("));
}

#[test]
fn mysql_ddl_strings_preserve_quotes_and_backslashes_for_existing_and_copied_fields() {
    let mut before = snapshot();
    before.columns[1].default = Some("'目录\\it''s'".into());
    let mut draft = DesignDraft::from(&before);
    draft.fields[1].column.data_type = "varchar(64)".into();
    let object = Object {
        name: "items".into(),
        ..Default::default()
    };
    let alter = design_sql("mysql", &object, &before, &draft).unwrap();
    let create = crate::build_create_table("mysql", &object, &before.columns).unwrap();
    for sql in [&alter[0], &create] {
        assert!(sql.contains("DEFAULT '目录\\\\it''s'"), "{sql}");
        assert!(!sql.contains("CONVERT("));
    }
}
#[test]
fn mysql_primary_and_identity_changes_are_one_atomic_statement() {
    let before = snapshot();
    let mut draft = DesignDraft::from(&before);
    draft.fields[0].column.extra.clear();
    draft.fields[0].column.key.clear();
    draft.fields[1].column.key = "PRI".into();
    let sql = design_sql(
        "mysql",
        &Object {
            name: "items".into(),
            ..Default::default()
        },
        &before,
        &draft,
    )
    .unwrap();
    assert_eq!(sql.len(), 1);
    assert!(sql[0].contains("DROP PRIMARY KEY"));
    assert!(sql[0].contains("ADD PRIMARY KEY (`title`)"));
    draft.fields[0].column.extra = "auto_increment".into();
    assert!(
        design_sql(
            "mysql",
            &Object {
                name: "items".into(),
                ..Default::default()
            },
            &before,
            &draft
        )
        .is_err()
    );
}

#[test]
fn mysql_reorder_and_rename_preserve_existing_composite_primary_key_order() {
    let mut before = snapshot();
    before.columns[0].extra.clear();
    before.columns[1].key = "PRI".into();
    let mut draft = DesignDraft::from(&before);
    draft.fields[0].column.name = "renamed_id".into();
    draft.fields.swap(0, 1);
    let sql = design_sql(
        "mysql",
        &Object {
            name: "items".into(),
            ..Default::default()
        },
        &before,
        &draft,
    )
    .unwrap();
    assert!(sql[0].contains("CHANGE COLUMN `id` `renamed_id`"));
    assert!(!sql[0].contains("PRIMARY KEY"));
}
#[test]
fn invalid_types_duplicate_names_and_unknown_originals_never_produce_sql() {
    let before = snapshot();
    let object = Object {
        name: "items".into(),
        ..Default::default()
    };
    for bad in ["TEXT; DROP TABLE items", "INT) /*", "varchar(10) DEFAULT 1"] {
        let mut draft = DesignDraft::from(&before);
        draft.fields[1].column.data_type = bad.into();
        assert!(design_sql("mysql", &object, &before, &draft).is_err());
    }
    let mut draft = DesignDraft::from(&before);
    draft.fields[1].column.name = "ID".into();
    assert!(design_sql("mysql", &object, &before, &draft).is_err());
    let mut draft = DesignDraft::from(&before);
    draft.fields[0].original = Some("gone".into());
    assert!(design_sql("mysql", &object, &before, &draft).is_err());
}
#[test]
fn unchanged_snapshot_is_noop_and_portable_draft_refuses_silent_reordering() {
    let before = snapshot();
    let object = Object {
        name: "items".into(),
        ..Default::default()
    };
    let mut draft = DesignDraft::from(&before);
    assert!(
        design_sql("mysql", &object, &before, &draft)
            .unwrap()
            .is_empty()
    );
    draft.fields.swap(0, 1);
    assert!(design_sql("sqlite", &object, &before, &draft).is_err());
}
