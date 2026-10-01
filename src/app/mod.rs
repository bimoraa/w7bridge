/*! CLI bootstrap과 host lifecycle을 구분해. */

mod bootstrap;
mod lifecycle;
pub use bootstrap::{run, run_desktop};
