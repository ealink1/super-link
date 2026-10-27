//! Native GPUI shell. Owns rendering and input only; all I/O goes through
//! application services.

pub mod editor;
pub mod note_page;
mod query_page;
mod shell;
pub mod terminal_page;
mod theme;
pub mod vt;

pub use note_page::NotePage;
pub use query_page::QueryPage;
pub use shell::{SuperLinkApp, run};
pub use terminal_page::TerminalPage;
pub use theme::Theme;
