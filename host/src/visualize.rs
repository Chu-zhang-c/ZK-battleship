// Simple ASCII visualization helpers for player boards.
//
// This module provides functions to pretty-print a `PlayerBoard` produced
// by `board_init.rs`. It supports optionally hiding ship positions so the
// opponent's board can be displayed without revealing ship locations.

use core::{GameState, CellState};

/// Render a single `GameState` to stdout. If `reveal_ships` is false,
/// ship cells (derived from `GameState.ships`) are hidden unless they are
/// hit in the grid.
pub fn display_board(state: &GameState, reveal_ships: bool) {
    // Header
    print!("   ");
    for x in 0..crate::board_init::BOARD_SIZE { print!("{:2} ", x); }
    println!();
    // Build a fast lookup of ship-occupied cells when revealing ships
    let mut ship_map = vec![vec![false; crate::board_init::BOARD_SIZE]; crate::board_init::BOARD_SIZE];
    if reveal_ships {
        for ship in &state.ships {
            for p in ship.get_coordinates() {
                let x = p.x as usize;
                let y = p.y as usize;
                ship_map[y][x] = true;
            }
        }
    }

    for y in 0..crate::board_init::BOARD_SIZE {
        print!("{:2} ", y);
        for x in 0..crate::board_init::BOARD_SIZE {
            let cell = state.grid[y][x];
            let ch = match cell {
                CellState::Empty => {
                    if reveal_ships && ship_map[y][x] { 'S' } else { '.' }
                }
                CellState::Miss => 'o',
                CellState::Hit => 'X',
            };
            print!(" {ch} ");
        }
        println!();
    }
}

/// Display both players' boards side-by-side. `reveal_self` will reveal the
/// left player's ships; the right player's ships remain hidden.
pub fn display_dual(left: &GameState, right: &GameState, reveal_left: bool) {
    // Left header
    print!("   ");
    for x in 0..crate::board_init::BOARD_SIZE { print!("{:2} ", x); }
    print!("    ");
    // Right header
    print!("   ");
    for x in 0..crate::board_init::BOARD_SIZE { print!("{:2} ", x); }
    println!();
    // Precompute ship maps
    let mut left_map = vec![vec![false; crate::board_init::BOARD_SIZE]; crate::board_init::BOARD_SIZE];
    if reveal_left {
        for ship in &left.ships {
            for p in ship.get_coordinates() {
                left_map[p.y as usize][p.x as usize] = true;
            }
        }
    }

    let mut right_map = vec![vec![false; crate::board_init::BOARD_SIZE]; crate::board_init::BOARD_SIZE];
    for ship in &right.ships {
        for p in ship.get_coordinates() {
            right_map[p.y as usize][p.x as usize] = true;
        }
    }

    for y in 0..crate::board_init::BOARD_SIZE {
        // left
        print!("{:2} ", y);
        for x in 0..crate::board_init::BOARD_SIZE {
            let cell = left.grid[y][x];
            let ch = match cell {
                CellState::Empty => if reveal_left && left_map[y][x] { 'S' } else { '.' },
                CellState::Miss => 'o',
                CellState::Hit => 'X',
            };
            print!(" {ch} ");
        }
        print!("    ");
        // right (never reveal ships)
        print!("{:2} ", y);
        for x in 0..crate::board_init::BOARD_SIZE {
            let cell = right.grid[y][x];
            let ch = match cell {
                CellState::Empty => '.',
                CellState::Miss => 'o',
                CellState::Hit => 'X',
            };
            print!(" {ch} ");
        }
        println!();
    }
}
