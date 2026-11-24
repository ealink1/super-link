//! Creates only the explicitly supplied, empty development fixture directory.
use std::{path::PathBuf, sync::Arc};
use superlink_app::{Credentials, NotebookService, ProfileService, QueryService};
use superlink_domain::{ConnectionConfig, Note, NoteGroup, Notebook, Profile};
use superlink_infra::{LocalVault, Store};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("supply an absolute empty fixture directory")?,
    );
    if !root.is_absolute() || root.join("state.db").exists() {
        return Err("fixture must be an absolute new workspace".into());
    }
    std::fs::create_dir_all(&root)?;
    let store = Arc::new(Store::open(root.join("state.db"))?);
    let vault = Arc::new(LocalVault::new(&root));
    let service = ProfileService::new(store.clone(), vault.clone());
    let profile = service.save(
        Profile {
            id: "parity-sqlite".into(),
            name: "SQLite 对齐验收".into(),
            config: ConnectionConfig {
                kind: "sqlite".into(),
                database: root.join("demo.db").to_string_lossy().into(),
                ..Default::default()
            },
            ..Default::default()
        },
        &Credentials::default(),
    )?;
    QueryService::new(root.join("drivers")).execute(&profile,&Credentials::default(),"CREATE TABLE demo(id INTEGER PRIMARY KEY, name TEXT NOT NULL, amount DECIMAL(12,2), note TEXT); INSERT INTO demo VALUES(1,'中文输入与选择',12.50,'第一行'),(2,'分页和编辑',32.25,NULL),(3,'精确整数',99.99,'第三行');",100)?;
    NotebookService::new(store,vault).save(&Notebook{groups:vec![NoteGroup{id:"qa".into(),name:"迁移验收".into()}],notes:vec![Note{id:"parity-note".into(),title:"SuperLink 功能对齐".into(),body:"# 原生工作区\n\n这是一篇独立测试笔记。\n\n- 中文输入\n- 格式与分屏\n- 加密保存\n\n```sql\nSELECT * FROM demo;\n```".into(),group_id:"qa".into(),tags:"验收,测试".into(),..Default::default()}],..Default::default()})?;
    println!("Created isolated SuperLink fixture at {}", root.display());
    Ok(())
}
