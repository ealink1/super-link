//! Adapters: local metadata, credential vault, driver agent transport.
//!
//! Nothing in this crate may depend on a UI toolkit.

pub mod cancel;
pub mod ipc;
pub mod secrets;
pub mod state;

pub use cancel::QueryCancel;
pub use ipc::{AgentError, AgentFrame, DriverAgent};
pub use secrets::{LocalVault, SecretError};
pub use state::{Store, StoreError};

pub mod instance;
pub use instance::WorkspaceLease;
