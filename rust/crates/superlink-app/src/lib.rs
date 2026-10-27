//! Application services. No UI toolkit, no driver protocol details.
//!
//! Services own the invariants that used to live in the Go application layer:
//! credential separation, revision checks and bounded input.

pub mod profiles;
pub mod queries;

pub use profiles::{Credentials, ProfileService};
pub use queries::{DraftService, QueryLibrary};
