//! Native GPUI shell. Owns rendering and input only; all I/O goes through
//! application services.

pub mod editor;
pub mod note_page;
pub mod terminal_page;
pub mod vt;
mod query_page;
mod shell;
mod theme;

pub use note_page::NotePage;
pub use query_page::QueryPage;
pub use terminal_page::TerminalPage;
pub use shell::{SuperLinkApp, run};
pub use theme::Theme;
