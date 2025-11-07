// Library entry for the host crate. This re-exports the host modules so
// integration tests and other crates can depend on `host` as a library.

pub mod board_init;
pub mod visualize;
pub mod game_round;
pub mod game_master;

// Simple egui UI module (optional). Contains the desktop UI used by
// the host binary. Kept minimal so the rest of the crate remains usable
// as a library in tests and other tools.
pub mod ui;

// Optionally, you can expose helper functions here that combine the above
// modules into common flows.
