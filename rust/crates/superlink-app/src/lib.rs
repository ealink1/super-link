//! Application services. No UI toolkit, no driver protocol details.
//!
//! Services own the invariants that used to live in the Go application layer:
//! credential separation, revision checks and bounded input.

pub mod history;
pub mod notebook;
pub mod profiles;
pub mod queries;
pub mod query;
pub mod settings;
pub mod shell_hosts;

pub use history::{HistoryService, redact};
pub use notebook::{NotebookError, NotebookService};
pub use profiles::{Credentials, ProfileService};
pub use queries::{DraftService, QueryLibrary};
pub use query::{QueryError, QueryService};
pub use settings::SettingsService;
pub use shell_hosts::{HostCredentials, HostError, HostService};
