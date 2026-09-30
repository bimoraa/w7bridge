/*! SSH transport와 MCP client 연결을 소유해. */

pub(crate) mod client;
mod handshake;
pub(crate) mod session;
pub(crate) mod transport;

#[cfg(test)]
#[path = "../../tests/unit/connect.rs"]
mod tests;
