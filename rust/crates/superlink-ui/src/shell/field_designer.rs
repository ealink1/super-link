use super::*;
use crate::{
    schema_designer::{DesignEvent, SchemaDesigner},
    workbench::{TableSection, WorkbenchTab},
};
impl SuperLinkApp {
    pub(super) fn open_field_designer(&mut self, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|e| e.busy || !e.pending.is_empty())
        {
            self.error = Some("请先完成结构变更".into());
            cx.notify();
            return;
        }
        if self.workbench.as_ref().is_some_and(|b| !b.edits.is_empty()) {
            self.error = Some("请先提交或撤销数据编辑，再修改表结构".into());
            cx.notify();
            return;
        }
        self.structure_edit = None;
        if let Some(b) = self.workbench.as_mut() {
            b.tab = WorkbenchTab::Structure;
            b.section = TableSection::Columns;
        }
        self.ensure_field_designer(cx);
        cx.notify();
    }
    pub(super) fn ensure_field_designer(&mut self, cx: &mut Context<Self>) {
        let Some(b) = self.workbench.as_ref() else {
            return;
        };
        if b.stale || b.designer.is_some() {
            return;
        }
        let Some(profile) = self.profiles.get(b.conn).cloned() else {
            return;
        };
        let conn = b.conn;
        let db = b.database.clone();
        let table = b.table.clone();
        let id = b.id.clone();
        let identity = (profile.id.clone(), profile.revision);
        let event_id = id.clone();
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let theme = self.theme;
        let editor = cx.new(|cx| {
            SchemaDesigner::new(
                profile,
                db.clone(),
                table.clone(),
                queries,
                profiles,
                theme,
                cx,
            )
        });
        cx.subscribe(&editor, move |this, _, event, cx| {
            if this.profiles.get(conn).map(|p| (&p.id, p.revision))
                != Some((&identity.0, identity.1))
            {
                return;
            }
            match event {
                DesignEvent::Changed => {
                    if let Some(bench) = this.workbench.as_mut().filter(|b| b.id == event_id) {
                        bench.error = None;
                    }
                }
                DesignEvent::SelectSection(section) => {
                    this.select_table_section(*section, cx);
                }
                DesignEvent::Saved(target) => {
                    for bench in this
                        .workbench
                        .iter_mut()
                        .chain(this.parked_workbenches.iter_mut())
                    {
                        if bench.conn == conn && bench.database == db && bench.table == *target {
                            if bench.edits.is_empty() {
                                bench.result = None;
                            } else {
                                bench.stale = true;
                                bench.error =
                                    Some("表结构已变化，已保留数据编辑；请撤销后重新打开".into());
                            }
                            bench.ddl_source = None;
                        }
                    }
                    this.table_structures
                        .remove(&(conn, db.clone(), target.clone()));
                    this.fetch_tables(conn, db.clone(), cx);
                    this.fetch_structure(conn, db.clone(), target.clone(), cx);
                }
            }
            cx.notify();
        })
        .detach();
        if let Some(b) = self.workbench.as_mut()
            && b.id == id
        {
            b.designer = Some(editor);
        }
    }
}
