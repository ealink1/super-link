//! Application services. No UI toolkit, no driver protocol details.
//!
//! Services own the invariants that used to live in the Go application layer:
//! credential separation, revision checks and bounded input.

pub mod catalog;
pub mod chat;
pub mod data_changes;
pub mod documents;
pub mod drafts;
pub mod driver_install;
pub mod execution;
pub mod execution_policy;
pub mod groups;
pub mod history;
pub mod imports;
pub mod notebook;
pub mod profiles;
pub mod queries;
pub mod query;
pub mod saved_queries;
mod scope;
pub mod settings;
pub mod shell_hosts;
pub mod structure;
pub mod table_design;
pub mod transfer;
pub mod updates;

pub use drafts::DraftService;
pub use groups::{GroupService, GroupSummary};
pub use history::{HistoryService, placeholder_count, redact};
pub use notebook::{NotebookError, NotebookService};
pub use profiles::{Credentials, ProfileService};
pub use queries::QueryLibrary;
pub use query::{QueryError, QueryService};
pub use saved_queries::SavedQueryService;
pub use settings::{
    DEFAULT_SHELL_FONT_SIZE, MAX_SHELL_FONT_SIZE, MIN_SHELL_FONT_SIZE, SettingsService,
};
pub use shell_hosts::{HostCredentials, HostError, HostService};
pub use structure::{StructureError, StructureService};
pub use superlink_drivers::QueryCancel;
pub use superlink_drivers::install_crypto_provider;
pub use transfer::{data_root, diagnostics, drivers_dir};
