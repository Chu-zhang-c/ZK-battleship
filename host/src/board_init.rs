// Helpers for interactively initializing a player's board.
//
// This file provides a small, self-contained `PlayerBoard` type and
// interactive helpers that prompt a human user (stdin) for placement of
// the canonical five ships. It does not depend on the `core` crate so it
// can be used independently; the types are intentionally similar so they
// are easy to map to `GameState` if you later wire them together.
//
// Usage (from a host binary):
//    let board = board_init::prompt_place_ships("Player 1");
//
use std::io::{self, Write};

// Use the canonical `core` crate types so host code and guest code share the
// same definitions and behavior.
use core::{GameState, ShipType, Direction, Position};

/// Prompt the user to place ships and return a filled `GameState`.
///
/// This function mirrors the previous interactive helper but now uses the
/// `core::GameState` as the authoritative structure. The returned
/// `GameState` will have `ships` populated; the `grid` remains empty until
/// shots are applied.
pub fn prompt_place_ships(player_name: &str) -> GameState {
    let mut state = GameState::new([0u8; 16]);
    println!("{}: place your ships on a {}x{} board.", player_name, super::BOARD_SIZE, super::BOARD_SIZE);
    println!("Coordinates are 0-based: x in [0..{}], y in [0..{}].", super::BOARD_SIZE-1, super::BOARD_SIZE-1);

    for &st in [ShipType::Carrier, ShipType::Battleship, ShipType::Cruiser, ShipType::Submarine, ShipType::Destroyer].iter() {
        loop {
            print!("Place {} (size {}) as: x y H/V: ", format!("{:?}", st), st.size());
            io::stdout().flush().ok();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                println!("Failed to read input, try again.");
                continue;
            }
            let parts: Vec<_> = input.trim().split_whitespace().collect();
            if parts.len() != 3 {
                println!("Expected three tokens: x y H/V");
                continue;
            }
            let x = match parts[0].parse::<u32>() {
                Ok(v) => v,
                Err(_) => { println!("Invalid x"); continue; }
            };
            let y = match parts[1].parse::<u32>() {
                Ok(v) => v,
                Err(_) => { println!("Invalid y"); continue; }
            };
            let dir = match parts[2].to_uppercase().as_str() {
                "H" => Direction::Horizontal,
                "V" => Direction::Vertical,
                _ => { println!("Invalid direction, use H or V"); continue; }
            };
            let pos = Position::new(x, y);
            if !state.can_place_ship(st, pos, dir) {
                println!("Invalid placement (out of bounds or overlapping). Try again.");
                continue;
            }
            state.place_ship(st, pos, dir);
            break;
        }
    }

    println!("{}: placement complete.\n", player_name);
    state
}
