/*! 불변 registry와 공유 executor를 MCP에 연결해. */

mod handler;
pub(crate) mod hub;
mod router;
pub(crate) mod runtime;
pub use handler::Bridge;
pub use runtime::serve_stdio;
