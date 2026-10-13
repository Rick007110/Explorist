//! File-system logic: reading folders, sorting, filtering and file operations.
//!
//! Nothing in here depends on the UI, so all of it is unit-tested.

pub mod entry;
pub mod filter;
pub mod format;
pub mod kind;
pub mod listing;
pub mod ops;
pub mod path_input;
pub mod places;
pub mod sort;
