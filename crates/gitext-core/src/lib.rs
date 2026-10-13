//! Git and domain logic for gitext. Contains no UI code.

pub mod error;
pub mod git;

pub use error::GitError;
pub use git::Git;
