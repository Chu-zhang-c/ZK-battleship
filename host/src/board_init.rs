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
use rand::thread_rng;

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

    // Show the (empty) board initially so the player gets orientation
    println!("Current board (your ships will be shown as they are placed):");
    crate::visualize::display_board(&state, true);

    // Ask whether to place manually or randomly
    loop {
        print!("Choose placement mode: (M)anual or (R)andom?: ");
        io::stdout().flush().ok();
        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).is_err() {
            println!("Failed to read input, try again.");
            continue;
        }
        let choice = choice.trim().to_uppercase();
        if choice == "R" || choice == "RANDOM" {
            let mut rng = thread_rng();
            if state.place_ships_randomly(&mut rng) {
                println!("Random placement complete:");
                crate::visualize::display_board(&state, true);
                return state;
            } else {
                println!("Random placement failed; falling back to manual placement.");
                break; // fall through to manual placement loop
            }
        } else if choice == "M" || choice == "MANUAL" {
            break; // proceed to manual placement
        } else {
            println!("Please enter 'M' for manual or 'R' for random.");
            continue;
        }
    }

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
            // Show the board after each placement so the player can confirm
            crate::visualize::display_board(&state, true);
            break;
        }
    }

    println!("{}: placement complete.\n", player_name);
    state
}
