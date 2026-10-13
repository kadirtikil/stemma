//! Git and domain logic for stemma. Contains no UI code.

pub mod error;
pub mod git;
pub mod service;

pub use error::GitError;
pub use git::Git;
