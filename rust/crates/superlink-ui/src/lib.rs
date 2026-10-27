//! Native GPUI shell. Owns rendering and input only; all I/O goes through
//! application services.

mod shell;
mod theme;

pub use shell::{SuperLinkApp, run};
pub use theme::Theme;
