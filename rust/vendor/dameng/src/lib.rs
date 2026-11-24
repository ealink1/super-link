//! Dameng database sync driver.
//!
//! Provides synchronous connection and query execution against
//! Dameng database servers.

pub mod client;
pub mod config;
pub mod error;
pub mod row;
pub mod transaction;
pub mod sql_bind;

pub use client::Client;
pub use config::ConnectOptions;
pub use error::{Error, Result};
pub use dameng_protocol::Row;

// Re-export protocol types needed for parameter binding
pub use dameng_protocol::message::{BindParam, ParameterDirection};
pub use dameng_protocol::message::isolation::{IsolationLevel, SetIsolationMessage};

// Re-export ToDmValue trait for SQLx-style dynamic binding
pub use dameng_types::ToDmValue;

// Re-export row types
pub use row::{DmDecode, QueryRow, QueryRowRef, ResultSet, ResultSetIter};

// Re-export Transaction
pub use transaction::Transaction;
