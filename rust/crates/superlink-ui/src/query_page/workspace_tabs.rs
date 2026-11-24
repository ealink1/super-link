use super::*;

impl QueryPage {
    pub(crate) fn workspace_tabs(&self) -> Vec<(String, String, bool)> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| (tab.id.clone(), tab.title.clone(), index == self.active))
            .collect()
    }

    pub(crate) fn activate_workspace_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
            self.switch_tab(index, cx);
        }
    }

    pub(crate) fn close_workspace_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
            self.close_tab(index, cx);
        }
    }
}
