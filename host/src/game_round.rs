// High-level interactive game round loop between two human players.
//
// This module orchestrates: prompting each player to place their ships
// (via `board_init::prompt_place_ships`), alternating turns to ask for
// shots, applying shots to opponent boards, visualizing both boards, and
// announcing the winner when all ships for a player are sunk.
//
// The module keeps the interaction simple and synchronous using stdin/stdout.

use std::io::{self, Write};
use crate::board_init::{prompt_place_ships, PlayerBoard, Position};
use crate::visualize::{display_board, display_dual};

/// Run a fully interactive two-player session. This function blocks on
/// stdin and prints progress to stdout.
pub fn run_interactive() {
    println!("Welcome to Battleship (interactive host-mode)");

    println!("Player 1, place your ships:");
    let mut p1 = prompt_place_ships("Player 1");

    println!("Player 2, place your ships:");
    let mut p2 = prompt_place_ships("Player 2");

    // Optionally show both boards to each player here. For simple
    // play-through, we show the current player's own board and the
    // opponent's hidden board.
    let mut turn = 0usize; // 0 => player1, 1 => player2

    loop {
        println!("\n---- Round: player {} ----", if turn == 0 { 1 } else { 2 });
        if turn == 0 {
            println!("Your board (revealed):");
            display_board(&p1, true);
            println!("Opponent board (hidden):");
            display_board(&p2, false);

            if handle_player_turn(&mut p1, &mut p2, "Player 1") {
                println!("Player 1 wins!");
                break;
            }
        } else {
            println!("Your board (revealed):");
            display_board(&p2, true);
            println!("Opponent board (hidden):");
            display_board(&p1, false);

            if handle_player_turn(&mut p2, &mut p1, "Player 2") {
                println!("Player 2 wins!");
                break;
            }
        }
        turn = 1 - turn;
    }
}

/// Handle a single player's turn: prompt for shot coordinates, apply shot
/// to opponent board, print the outcome, and return `true` if opponent is
/// fully sunk (game over).
fn handle_player_turn(active: &mut PlayerBoard, opponent: &mut PlayerBoard, player_name: &str) -> bool {
    loop {
        print!("{player_name}, enter shot as: x y (or 'show' to display boards): ");
        io::stdout().flush().ok();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            println!("Failed to read input, try again.");
            continue;
        }
        let s = input.trim();
        if s.eq_ignore_ascii_case("show") {
            display_dual(active, opponent, true);
            continue;
        }
        let parts: Vec<_> = s.split_whitespace().collect();
        if parts.len() != 2 {
            println!("Please enter two integers 'x y'.");
            continue;
        }
        let x = match parts[0].parse::<usize>() {
            Ok(v) => v,
            Err(_) => { println!("Invalid x"); continue; }
        };
        let y = match parts[1].parse::<usize>() {
            Ok(v) => v,
            Err(_) => { println!("Invalid y"); continue; }
        };
        let pos = Position { x, y };
        match opponent.apply_shot(pos) {
            None => { println!("Shot out of bounds or already taken; try again."); continue; }
            Some((hit, ship_opt)) => {
                if hit {
                    println!("Hit!");
                    if let Some(st) = ship_opt {
                        // if the ship was sunk, it will be reflected in the player's ship state
                        // but we don't track sunk vs hit here precisely; the board's hit markers are shown.
                        println!("Ship affected: {:?}", st);
                    }
                } else {
                    println!("Miss.");
                }
                break;
            }
        }
    }

    // After applying shot, check for game over
    if opponent.all_sunk() {
        return true;
    }
    false
}

/// Small helper to run a quick demo game without interactive placement.
/// Places each player's ships in predefined locations (useful for
/// automated testing or demoing visualization).
pub fn run_demo() {
    use crate::board_init::{PlayerBoard, ShipType, Direction, Position};
    let mut p1 = PlayerBoard::new_empty();
    p1.place_ship(ShipType::Carrier, Position { x:0,y:0 }, Direction::Horizontal);
    p1.place_ship(ShipType::Battleship, Position { x:0,y:2 }, Direction::Horizontal);
    p1.place_ship(ShipType::Cruiser, Position { x:0,y:4 }, Direction::Horizontal);
    p1.place_ship(ShipType::Submarine, Position { x:0,y:6 }, Direction::Horizontal);
    p1.place_ship(ShipType::Destroyer, Position { x:0,y:8 }, Direction::Horizontal);

    let mut p2 = PlayerBoard::new_empty();
    p2.place_ship(ShipType::Carrier, Position { x:0,y:0 }, Direction::Vertical);
    p2.place_ship(ShipType::Battleship, Position { x:2,y:0 }, Direction::Vertical);
    p2.place_ship(ShipType::Cruiser, Position { x:4,y:0 }, Direction::Vertical);
    p2.place_ship(ShipType::Submarine, Position { x:6,y:0 }, Direction::Vertical);
    p2.place_ship(ShipType::Destroyer, Position { x:8,y:0 }, Direction::Vertical);

    println!("Demo: Player boards (left: P1 revealed, right: P2 hidden)");
    display_dual(&p1, &p2, true);
}
