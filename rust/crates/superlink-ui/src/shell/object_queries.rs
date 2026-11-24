use super::*;
impl SuperLinkApp {
    /// Non-SQL keys, collections, indexes and topics open their own read command.
    pub(super) fn open_protocol_object(
        &mut self,
        conn: usize,
        database: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(profile) = self.profiles.get(conn).cloned() else {
            return false;
        };
        let Some(command) = superlink_domain::object_query::object_read_command(
            &profile.config.kind,
            database,
            name,
        ) else {
            return false;
        };
        if self.workbench.as_ref().is_some_and(|b| !b.edits.is_empty())
            || self
                .structure_edit
                .as_ref()
                .is_some_and(|e| !e.pending.is_empty())
        {
            self.error = Some("请先提交或撤销当前变更".into());
            cx.notify();
            return true;
        }
        self.workspace = Workspace::Sql;
        if !self.park_workbench(cx) {
            return true;
        }
        self.structure_edit = None;
        self.query_page.update(cx, |page, cx| {
            page.load_table_query(profile, database.into(), command, cx);
            page.set_protocol_target(name, cx);
        });
        cx.notify();
        true
    }
}
