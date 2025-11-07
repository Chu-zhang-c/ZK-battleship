// Library entry for the host crate. This re-exports the host modules so
// integration tests and other crates can depend on `host` as a library.

pub mod board_init;
pub mod visualize;
pub mod game_round;
pub mod game_master;
pub mod proofs;
pub mod network;
pub mod network_protocol;

// Optionally, you can expose helper functions here that combine the above
// modules into common flows.
