use super::*;

fn visible(file: &RemoteFile, filter: &str, show_hidden: bool) -> bool {
    (show_hidden || !file.name.starts_with('.'))
        && file
            .name
            .to_lowercase()
            .contains(&filter.trim().to_lowercase())
}

impl TerminalPage {
    pub(in crate::terminal_page) fn visible_files(&self) -> Vec<&RemoteFile> {
        self.auxiliary
            .files
            .iter()
            .filter(|file| {
                visible(
                    file,
                    &self.auxiliary.filter_text,
                    self.auxiliary.show_hidden,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filtering_keeps_file_identity_and_respects_hidden_entries() {
        let file = RemoteFile {
            name: ".Env.PROD".into(),
            mode: "-rw-------".into(),
            size: 1,
            modified: None,
            directory: false,
        };
        assert!(visible(&file, "prod", true));
        assert!(!visible(&file, "prod", false));
        assert!(!visible(&file, "missing", true));
        assert!(visible(&file, " env ", true));
    }
}
