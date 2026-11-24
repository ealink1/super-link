use super::*;
pub(super) fn read_credentials(
    service: &ProfileService,
    profile: &Profile,
) -> Result<Credentials, superlink_app::QueryError> {
    let current = service
        .get(&profile.id)
        .map_err(|e| superlink_app::QueryError::Policy(e.to_string()))?;
    if current.revision != profile.revision {
        return Err(superlink_app::QueryError::Policy(
            "连接配置已修改，请刷新后重试".into(),
        ));
    }
    service
        .credentials(&current)
        .map(|c| c.unwrap_or_default())
        .map_err(|e| superlink_app::QueryError::Policy(e.to_string()))
}
impl SuperLinkApp {
    pub(super) fn replace_profiles(&mut self, profiles: Vec<Profile>, cx: &mut Context<Self>) {
        let identity = |items: &[Profile]| {
            items
                .iter()
                .map(|p| (p.id.clone(), p.revision))
                .collect::<Vec<_>>()
        };
        if identity(&self.profiles) != identity(&profiles) {
            let selected = self.profiles.get(self.selected).map(|p| p.id.clone());
            for bench in self
                .workbench
                .iter_mut()
                .chain(self.parked_workbenches.iter_mut())
            {
                if let Some(index) = self
                    .profiles
                    .get(bench.conn)
                    .and_then(|old| matching_profile(&profiles, old))
                {
                    bench.conn = index;
                } else {
                    bench.stale = true;
                    if let Some(editor) = &bench.designer {
                        editor.update(cx, |editor, cx| editor.mark_stale(cx));
                    }
                    bench.request = bench.request.wrapping_add(1);
                    bench.loading = false;
                    bench.preview = None;
                    bench.error = Some(
                        "连接配置已变化；请撤销暂存变更并重新打开表，之前写入结果需刷新确认".into(),
                    );
                    if let Some(cancel) = bench.operation.take() {
                        cx.background_executor()
                            .spawn(async move {
                                cancel.cancel();
                            })
                            .detach();
                    }
                }
            }
            if let Some(edit) = self.structure_edit.as_mut() {
                if let Some(index) = self
                    .profiles
                    .get(edit.conn)
                    .and_then(|old| matching_profile(&profiles, old))
                {
                    edit.conn = index;
                } else {
                    edit.stale = true;
                    edit.busy = false;
                    edit.preview = None;
                    edit.error =
                        Some("连接配置已变化，请关闭面板后重新预览；之前执行结果需刷新确认".into());
                }
            }
            self.selected = selected
                .and_then(|id| profiles.iter().position(|p| p.id == id))
                .unwrap_or(0);
            self.expanded_conns.clear();
            self.expanded_dbs.clear();
            self.expanded_tables.clear();
            self.conn_dbs.clear();
            self.conn_db_note.clear();
            self.db_tables.clear();
            self.table_structures.clear();
            self.table_counts.clear();
            self.column_errors.clear();
            self.prefetching.clear();
            self.connection_actions = None;
        }
        self.profiles = profiles;
        self.sync_catalog(cx);
    }

    pub(super) fn refresh_profiles(&mut self, selected: Option<String>, cx: &mut Context<Self>) {
        let service = self.service.clone();
        let selected = selected.or_else(|| self.profiles.get(self.selected).map(|p| p.id.clone()));
        let task = cx
            .background_executor()
            .spawn(async move { service.list() });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    Ok(profiles) => {
                        this.replace_profiles(profiles, cx);
                        this.selected = selected
                            .and_then(|id| this.profiles.iter().position(|p| p.id == id))
                            .unwrap_or(0);
                        this.error = None;
                        this.sync_selection(cx);
                    }
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn matching_profile(items: &[Profile], previous: &Profile) -> Option<usize> {
    items
        .iter()
        .position(|p| p.id == previous.id && p.revision == previous.revision)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_follows_identity_through_reordering_and_refuses_changed_connections() {
        let a = Profile {
            id: "a".into(),
            revision: 2,
            ..Default::default()
        };
        let b = Profile {
            id: "b".into(),
            revision: 3,
            ..Default::default()
        };
        assert_eq!(matching_profile(&[b.clone(), a.clone()], &a), Some(1));
        let mut changed = a.clone();
        changed.revision += 1;
        assert_eq!(matching_profile(&[changed, b], &a), None);
    }
}
