//! Native GPUI shell. Owns rendering and input only; all I/O goes through
//! application services.

mod chat_panel;
mod connection_form;
pub mod editor;
pub mod icons;
pub mod multiline_editor;
pub mod note_page;
mod query_page;
mod schema_designer;
mod shell;
mod shell_style;
mod shell_workspace;
mod sql_highlight;
pub mod terminal_page;
mod text;
pub mod theme;
pub mod vt;
pub mod workbench;

pub use note_page::NotePage;
pub use query_page::QueryPage;
pub use shell::{SuperLinkApp, run};
pub use terminal_page::TerminalPage;
pub use theme::Theme;
