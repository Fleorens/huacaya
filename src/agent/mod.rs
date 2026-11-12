//! The agent: the protocol (pure, in `proto`) and the server that answers over HTTP (in `server`), and
//! the client a remote dashboard uses to read them (in `client`).
pub mod client;
pub mod proto;
pub mod server;
