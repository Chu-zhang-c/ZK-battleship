// Game master: orchestrates a full two-player session using `core::GameState`.
//
// Rules implemented:
// - Each player places ships (interactive prompting via `board_init::prompt_place_ships`).
// - On each turn the current player's own board is revealed, the opponent's
//   board is hidden (except hits/misses).
// - Shot results:
//     * Hit: mark hit, current player gets another shot (extra turn), unless
//            that hit also sinks the ship and ends the game.
//     * Sunk: mark hit and sunk; turn passes to the next player. If all
//             opponent ships are sunk the current player wins immediately.
//     * Miss: mark miss, turn passes to the next player.
// - If a shot is invalid (out of bounds or already-shot cell), the player is
//   reprompted.

use std::io::{self, Write};
use crate::board_init::prompt_place_ships;
use crate::visualize::{display_board, display_dual};
use core::{GameState, Position, HitType};
use crate::proofs::{GuestInput, produce_and_verify_proof, verify_remote_round_proof};

/// Run the full interactive game implementing the requested turn rules.
pub fn run_game_master_interactive() {
    println!("=== Battleship: Game Master ===");

    println!("Player 1: place your ships");
    let mut p1: GameState = prompt_place_ships("Player 1");

    println!("Player 2: place your ships");
    let mut p2: GameState = prompt_place_ships("Player 2");

    // 0 -> player1, 1 -> player2
    let mut turn: usize = 0;

    loop {
        let (active_name, (active, opponent)) = if turn == 0 {
            ("Player 1", (&mut p1, &mut p2))
        } else {
            ("Player 2", (&mut p2, &mut p1))
        };

        println!("\n--- {}'s turn ---", active_name);
        // show active player's own board and the opponent's hidden board
        display_board(active, true);
        display_board(opponent, false);

        // Player may take one or more shots depending on Hit vs Miss vs Sunk rules.
        loop {
            print!("{active_name}, enter shot as 'x y' (or 'show' to display both boards): ");
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
                println!("Please enter two integers: x y");
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

            let pos = Position::new(x, y);
            // Instead of applying the shot directly, produce a per-round proof
            // using the guest and verify the produced RoundCommit matches the
            // server's authoritative application of the shot.
            
            let guest_input = GuestInput { initial: opponent.clone(), shots: vec![pos] };
            match produce_and_verify_proof(&guest_input) {
                Ok(receipt) => {
                    // Verify and validate the round's commit against authoritative state
                    match verify_remote_round_proof(&receipt, opponent, pos) {
                        Ok(commits) => {
                            // commits last element corresponds to the shot result we just proved
                            let rc = commits.last().unwrap();
                            match &rc.hit {
                                HitType::Miss => {
                                    println!("Miss (verified).");
                                    // update opponent state using the commit we verified
                                    let _ = opponent.apply_shot(pos);
                                    turn = 1 - turn;
                                    break;
                                }
                                HitType::Hit => {
                                    println!("Hit (verified)! You get another shot.");
                                    let _ = opponent.apply_shot(pos);
                                    if opponent.ships.iter().all(|s| s.is_sunk()) {
                                        println!("All opponent ships sunk! {} wins!", active_name);
                                        return;
                                    }
                                    display_board(active, true);
                                    display_board(opponent, false);
                                    continue;
                                }
                                HitType::Sunk(st) => {
                                    println!("Sunk {:?} (verified). Turn passes.", st);
                                    let _ = opponent.apply_shot(pos);
                                    if opponent.ships.iter().all(|s| s.is_sunk()) {
                                        println!("All opponent ships sunk! {} wins!", active_name);
                                        return;
                                    }
                                    turn = 1 - turn;
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            println!("Proof verification failed: {e}");
                            println!("Rejecting shot.");
                            continue;
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to produce/verify proof locally: {e}");
                    println!("Rejecting shot.");
                    continue;
                }
            }
        }
    }
}

/// Non-interactive quick demo that follows the same turn rules but uses
/// deterministic placements and a scripted series of shots (for testing).
pub fn run_game_master_demo() {
    use core::{ShipType, Direction};
    // Setup demo players
    let mut p1 = GameState::new([0; 16]);
    let mut p2 = GameState::new([0; 16]);

    // deterministic placements
    p1.place_ship(ShipType::Carrier, Position::new(0,0), Direction::Horizontal);
    p1.place_ship(ShipType::Battleship, Position::new(0,2), Direction::Horizontal);
    p1.place_ship(ShipType::Cruiser, Position::new(0,4), Direction::Horizontal);
    p1.place_ship(ShipType::Submarine, Position::new(0,6), Direction::Horizontal);
    p1.place_ship(ShipType::Destroyer, Position::new(0,8), Direction::Horizontal);

    p2.place_ship(ShipType::Carrier, Position::new(0,0), Direction::Vertical);
    p2.place_ship(ShipType::Battleship, Position::new(2,0), Direction::Vertical);
    p2.place_ship(ShipType::Cruiser, Position::new(4,0), Direction::Vertical);
    p2.place_ship(ShipType::Submarine, Position::new(6,0), Direction::Vertical);
    p2.place_ship(ShipType::Destroyer, Position::new(8,0), Direction::Vertical);

    // Scripted play: P1 shoots and misses, P2 hits, etc. We exercise rules.
    let mut turn = 0usize;
    let shots = vec![
        // (player, x,y)
        (0, 9, 9), // P1 miss
        (1, 0, 0), // P2 hit (continues)
        (1, 0, 1), // P2 hit (continues)
        (1, 0, 2), // P2 hit (continues)
        // ... continue until demo ends
    ];

    let mut idx = 0usize;
    while idx < shots.len() {
    let (_p, x, y) = shots[idx];
    let (_active, opponent, active_name) = if turn == 0 { (&mut p1, &mut p2, "P1") } else { (&mut p2, &mut p1, "P2") };
        println!("{} shoots at {},{}", active_name, x, y);
        let pos = Position::new(x, y);
        if let Some(hit_type) = opponent.apply_shot(pos) {
            match hit_type {
                HitType::Miss => { println!("Miss."); turn = 1 - turn; idx += 1; }
                HitType::Hit => { println!("Hit! {} shoots again.", active_name); idx += 1; }
                HitType::Sunk(st) => { println!("Sunk {:?}. Turn passes.", st); turn = 1 - turn; idx += 1; }
            }
        } else {
            println!("Invalid shot (OOB or already shot). Skipping."); idx += 1; }

        if opponent.ships.iter().all(|s| s.is_sunk()) {
            println!("{} wins!", active_name);
            break;
        }
    }
}
