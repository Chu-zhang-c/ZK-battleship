// Simple ASCII visualization helpers for player boards.
//
// This module provides functions to pretty-print a `PlayerBoard` produced
// by `board_init.rs`. It supports optionally hiding ship positions so the
// opponent's board can be displayed without revealing ship locations.

use crate::board_init::{PlayerBoard, CellState, BOARD_SIZE};

/// Render a single board to stdout. If `reveal_ships` is false, ship cells
/// print as '.' (hidden) unless they are hit.
pub fn display_board(board: &PlayerBoard, reveal_ships: bool) {
    // Header
    print!("   ");
    for x in 0..BOARD_SIZE { print!("{:2} ", x); }
    println!();

    for y in 0..BOARD_SIZE {
        print!("{:2} ", y);
        for x in 0..BOARD_SIZE {
            let ch = match board.grid[y][x] {
                CellState::Empty => '.',
                CellState::Miss => 'o',
                CellState::Hit => 'X',
                CellState::Ship => {
                    if reveal_ships { 'S' } else { '.' }
                }
            };
            print!(" {ch} ");
        }
        println!();
    }
}

/// Display both players' boards side-by-side. `reveal_self` will reveal the
/// left player's ships; the right player's ships remain hidden.
pub fn display_dual(left: &PlayerBoard, right: &PlayerBoard, reveal_left: bool) {
    // Left header
    print!("   ");
    for x in 0..BOARD_SIZE { print!("{:2} ", x); }
    print!("    ");
    // Right header
    print!("   ");
    for x in 0..BOARD_SIZE { print!("{:2} ", x); }
    println!();

    for y in 0..BOARD_SIZE {
        // left
        print!("{:2} ", y);
        for x in 0..BOARD_SIZE {
            let ch = match left.grid[y][x] {
                CellState::Empty => '.',
                CellState::Miss => 'o',
                CellState::Hit => 'X',
                CellState::Ship => if reveal_left { 'S' } else { '.' },
            };
            print!(" {ch} ");
        }
        print!("    ");
        // right (never reveal)
        print!("{:2} ", y);
        for x in 0..BOARD_SIZE {
            let ch = match right.grid[y][x] {
                CellState::Empty => '.',
                CellState::Miss => 'o',
                CellState::Hit => 'X',
                CellState::Ship => '.',
            };
            print!(" {ch} ");
        }
        println!();
    }
}
