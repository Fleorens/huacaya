//! The agent protocol: parsing a request head, routing it, the log ring clients page through, and
//! the token check. Pure — no socket, no thread; everything is a function of strings and values.
pub mod proto;
