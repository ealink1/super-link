use super::*;
impl SuperLinkApp {
    pub(super) fn group_job(
        &mut self,
        job: impl FnOnce(&superlink_app::GroupService) -> Result<String, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.group_busy {
            return;
        }
        self.group_busy = true;
        let groups = self.groups.clone();
        let profiles = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            let notice = job(&groups)?;
            let profiles = profiles.list().map_err(|e| e.to_string())?;
            Ok::<_, String>((profiles, groups.declared(), notice))
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                this.group_busy = false;
                match outcome {
                    Ok((profiles, groups, notice)) => {
                        this.replace_profiles(profiles, cx);
                        this.declared_groups = groups;
                        this.group_note = Some(notice.into());
                        this.sync_selection(cx);
                    }
                    Err(error) => this.group_note = Some(error.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
