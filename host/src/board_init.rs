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
use core::{GameState, ShipType, Direction, Position, BOARD_SIZE};

/// Prompt the user to place ships and return a filled `GameState`.
///
/// This function mirrors the previous interactive helper but now uses the
/// `core::GameState` as the authoritative structure. The returned
/// `GameState` will have `ships` populated; the `grid` remains empty until
/// shots are applied.
pub fn prompt_place_ships(player_name: &str) -> GameState {
    let mut state = GameState::new([0u8; 16]);
    println!("{}: place your ships on a {}x{} board.", player_name, BOARD_SIZE, BOARD_SIZE);
    println!("Coordinates are 0-based: x in [0..{}], y in [0..{}].", BOARD_SIZE-1, BOARD_SIZE-1);

    // Allow a more forgiving placement flow:
    // - "x y H/V" still works
    // - "x y" will use the current direction (defaults to Horizontal)
    // - "h" or "v" will set the direction
    // - "show" displays the current board
    for &st in [ShipType::Carrier, ShipType::Battleship, ShipType::Cruiser, ShipType::Submarine, ShipType::Destroyer].iter() {
        let mut cur_dir = Direction::Horizontal;
        loop {
            print!("Place {} (size {}) as: x y [H/V]  (commands: 'h','v','show') [current {:?}]: ", format!("{:?}", st), st.size(), cur_dir);
            io::stdout().flush().ok();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                println!("Failed to read input, try again.");
                continue;
            }
            let s = input.trim();
            if s.eq_ignore_ascii_case("show") {
                crate::visualize::display_board(&state, true);
                continue;
            }
            if s.eq_ignore_ascii_case("h") {
                cur_dir = Direction::Horizontal;
                println!("Direction set to H");
                continue;
            }
            if s.eq_ignore_ascii_case("v") {
                cur_dir = Direction::Vertical;
                println!("Direction set to V");
                continue;
            }

            let parts: Vec<_> = s.split_whitespace().collect();
            if parts.len() == 2 || parts.len() == 3 {
                let x = match parts[0].parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => { println!("Invalid x"); continue; }
                };
                let y = match parts[1].parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => { println!("Invalid y"); continue; }
                };
                let dir = if parts.len() == 3 {
                    match parts[2].to_uppercase().as_str() {
                        "H" => Direction::Horizontal,
                        "V" => Direction::Vertical,
                        _ => { println!("Invalid direction, use H or V"); continue; }
                    }
                } else { cur_dir };
                let pos = Position::new(x, y);
                if !state.can_place_ship(st, pos, dir) {
                    println!("Invalid placement (out of bounds or overlapping). Try again.");
                    continue;
                }
                state.place_ship(st, pos, dir);
                // Show the board after each placement so the player can confirm
                crate::visualize::display_board(&state, true);
                break;
            }
            println!("Unrecognized input. Enter 'x y' or 'x y H/V' or commands 'h','v','show'.");
            continue;
        }
    }

    println!("{}: placement complete.\n", player_name);
    state
}
