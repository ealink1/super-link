use super::*;
use superlink_domain::ConnectionConfig;
use superlink_drivers::NativeSqlite;
use superlink_sql::design::DesignField;
struct Fixture {
    profile: Profile,
    path: std::path::PathBuf,
    queries: QueryService,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn fixture() -> Fixture {
    let path = std::env::temp_dir().join(format!(
        "superlink-field-design-{}.db",
        uuid::Uuid::new_v4()
    ));
    let profile = Profile {
        revision: 1,
        config: ConnectionConfig {
            kind: "sqlite".into(),
            database: path.to_string_lossy().into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let driver = NativeSqlite::open(&profile.config).unwrap();
    driver
        .execute_batch(&[
            "CREATE TABLE items(id INTEGER PRIMARY KEY,title TEXT NOT NULL)".into(),
            "INSERT INTO items VALUES(1,'保留原数据')".into(),
        ])
        .unwrap();
    Fixture {
        profile,
        path,
        queries: QueryService::new(std::path::PathBuf::new()),
    }
}
#[test]
fn save_renames_adds_and_rereads_real_structure_without_losing_rows() {
    let f = fixture();
    let credentials = Credentials::default();
    let cancel = QueryCancel::new();
    let before = f
        .queries
        .design_snapshot(&f.profile, &credentials, "", "items", &cancel)
        .unwrap();
    let mut draft = DesignDraft::from(&before);
    draft.fields[1].column.name = "label".into();
    draft.fields.push(DesignField {
        original: None,
        column: ColumnDefinition {
            name: "note".into(),
            data_type: "TEXT".into(),
            nullable: "YES".into(),
            ..Default::default()
        },
    });
    let saved = f
        .queries
        .save_design(
            &f.profile,
            &credentials,
            "",
            DesignEdit {
                table: "items",
                before: &before,
                draft: &draft,
            },
            &cancel,
        )
        .unwrap();
    assert_eq!(
        saved
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "label", "note"]
    );
    let driver = NativeSqlite::open(&f.profile.config).unwrap();
    let data = driver
        .query("SELECT label,note FROM items", &[], 10)
        .unwrap();
    assert_eq!(data.rows[0][0], "保留原数据");
    assert!(data.rows[0][1].is_null());
}
#[test]
fn external_schema_change_and_read_only_profile_block_save() {
    let f = fixture();
    let credentials = Credentials::default();
    let cancel = QueryCancel::new();
    let before = f
        .queries
        .design_snapshot(&f.profile, &credentials, "", "items", &cancel)
        .unwrap();
    let mut draft = DesignDraft::from(&before);
    draft.fields[1].column.name = "label".into();
    let driver = NativeSqlite::open(&f.profile.config).unwrap();
    driver
        .execute_batch(&["ALTER TABLE items ADD other TEXT".into()])
        .unwrap();
    assert!(
        f.queries
            .save_design(
                &f.profile,
                &credentials,
                "",
                DesignEdit {
                    table: "items",
                    before: &before,
                    draft: &draft
                },
                &cancel
            )
            .unwrap_err()
            .to_string()
            .contains("已被修改")
    );
    let fresh = f
        .queries
        .design_snapshot(&f.profile, &credentials, "", "items", &cancel)
        .unwrap();
    let mut restricted = f.profile.clone();
    restricted.read_only = true;
    assert!(
        f.queries
            .save_design(
                &restricted,
                &credentials,
                "",
                DesignEdit {
                    table: "items",
                    before: &fresh,
                    draft: &DesignDraft::from(&fresh)
                },
                &cancel
            )
            .is_err()
    );
    assert_eq!(driver.structure("items").unwrap().columns[1].name, "title");
}
#[test]
fn later_ddl_failure_rolls_back_earlier_rename() {
    let f = fixture();
    let credentials = Credentials::default();
    let cancel = QueryCancel::new();
    let before = f
        .queries
        .design_snapshot(&f.profile, &credentials, "", "items", &cancel)
        .unwrap();
    let mut draft = DesignDraft::from(&before);
    draft.fields[1].column.name = "label".into();
    draft.fields.push(DesignField {
        original: None,
        column: ColumnDefinition {
            name: "required".into(),
            data_type: "INTEGER".into(),
            nullable: "NO".into(),
            ..Default::default()
        },
    });
    assert!(
        f.queries
            .save_design(
                &f.profile,
                &credentials,
                "",
                DesignEdit {
                    table: "items",
                    before: &before,
                    draft: &draft
                },
                &cancel
            )
            .is_err()
    );
    let snapshot = f
        .queries
        .design_snapshot(&f.profile, &credentials, "", "items", &QueryCancel::new())
        .unwrap();
    assert_eq!(snapshot, before);
}
