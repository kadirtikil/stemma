//! Git and domain logic for stemma. Contains no UI code.

pub mod error;
pub mod git;
pub mod library;
pub mod service;

pub use error::GitError;
pub use git::Git;
pub use library::Library;
