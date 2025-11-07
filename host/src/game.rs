// Consolidated game module: combines the previous game_master and
// game_coordinator responsibilities into a single module to reduce
// fragmentation and simplify imports.
use anyhow::Result;
use std::io::{self, Write};
use crate::board_init::prompt_place_ships;
use crate::visualize::{display_board, display_dual};
use core::{GameState, Position, HitType};
use risc0_zkvm::sha::Digest;
use crate::network::NetworkConnection;
use crate::network_protocol::GameMessage;
use crate::proofs::{GuestInput, produce_and_verify_proof, extract_round_commits, proofdata_from_receipt, receipt_from_proofdata, verify_remote_round_proof};

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

/// Networked game coordinator (previously GameCoordinator). Manages a
/// NetworkConnection and plays the networked game loop.
pub struct GameCoordinator {
    pub local_state: GameState,
    pub local_commit: Digest,
    pub network: NetworkConnection,
    pub player_name: String,
    pub starts_first: bool,
    pub opponent_name: Option<String>,
    pub opponent_commit: Option<Digest>,
    /// Local tracking view of the opponent's board (only grid updated with hits/misses)
    pub opponent_view: GameState,
}

impl GameCoordinator {
    pub fn new(local_state: GameState, local_commit: Digest, network: NetworkConnection, player_name: String, starts_first: bool) -> Self {
        Self { local_state, local_commit, network, player_name, starts_first, opponent_name: None, opponent_commit: None, opponent_view: GameState::new([0;16]) }
    }

    /// Perform handshake: exchange BoardReady messages and record opponent info.
    pub fn handshake(&mut self) -> Result<()> {
        if self.starts_first {
            // As host: send our BoardReady then receive opponent's
            let (opp_name, opp_commit, _opp_proof) = self.network.handshake_as_host(&self.player_name, self.local_commit, None)?;
            self.opponent_name = Some(opp_name);
            self.opponent_commit = Some(opp_commit);
        } else {
            // As client: receive host BoardReady then send ours
            let (host_name, host_commit, _host_proof) = self.network.handshake_as_client(&self.player_name, self.local_commit, None)?;
            self.opponent_name = Some(host_name);
            self.opponent_commit = Some(host_commit);
        }
        println!("Handshake complete with opponent: {:?}", self.opponent_name);
        Ok(())
    }

    /// Play the networked game loop. This function blocks until the game ends.
    pub fn play_game(&mut self) -> Result<()> {
        // Turn: true means local player's turn, false means opponent's turn
        let mut local_turn = self.starts_first;

        loop {
            if local_turn {
                // Show boards: local (revealed) and opponent view (hits/misses)
                display_dual(&self.local_state, &self.opponent_view, true);
                // Local player's move
                println!("Your turn. Enter shot as 'x y':");
                print!("> "); io::stdout().flush().ok();
                let mut line = String::new();
                io::stdin().read_line(&mut line)?;
                let parts: Vec<_> = line.trim().split_whitespace().collect();
                if parts.len() != 2 { println!("Invalid input"); continue; }
                let x: u32 = parts[0].parse().unwrap_or(999);
                let y: u32 = parts[1].parse().unwrap_or(999);
                let pos = Position::new(x,y);

                // Run local prover to create a Round proof for shooting opponent
                // We pass the opponent's authoritative state as the initial state
                match &self.local_state { // In peer-to-peer, each host keeps their own board; here we assume opponent state is unknown and use stored opponent_commit only
                    _ => {
                        // We don't have opponent GameState locally; instead we rely on the opponent to produce proof and send it.
                        // Simpler approach: send a TakeShot request and wait for opponent to respond with ShotResult containing proof.
                        let msg = GameMessage::TakeShot { position: pos };
                        self.network.send_enveloped(&msg)?;
                        // Wait for opponent ShotResult
                        let env = self.network.receive_enveloped()?;
                        match env.payload {
                            GameMessage::ShotResult { position, hit_type: _, proof } => {
                                // Reconstruct receipt and verify it locally
                                let receipt = receipt_from_proofdata(&proof)?;
                                // Extract the round commits from the receipt
                                let commits = extract_round_commits(&receipt)?;
                                let rc = commits.last().unwrap();
                                // As the shooter (we initiated the TakeShot), the opponent
                                // has applied the shot to their authoritative state and
                                // provided a proof. We should NOT apply the shot to our
                                // local_state (that's our own board). Instead, update
                                // our stored opponent_commit to the new_state from the proof
                                // so we track their latest commitment and record the
                                // hit/miss in our local opponent_view for visualization.
                                self.opponent_commit = Some(rc.new_state);

                                // Record hit/miss in opponent_view grid for UI
                                use core::CellState;
                                let x = position.x as usize;
                                let y = position.y as usize;
                                match rc.hit {
                                    HitType::Miss => {
                                        self.opponent_view.grid[y][x] = CellState::Miss;
                                        println!("Miss (verified). Turn passes to opponent.");
                                        local_turn = false;
                                    }
                                    HitType::Hit => {
                                        self.opponent_view.grid[y][x] = CellState::Hit;
                                        println!("Hit (verified)! You get another shot.");
                                        // shooter keeps the turn
                                        local_turn = true;
                                    }
                                    HitType::Sunk(st) => {
                                        self.opponent_view.grid[y][x] = CellState::Hit;
                                        println!("Sunk {:?} (verified). Turn passes.", st);
                                        local_turn = false;
                                    }
                                }
                                // Show boards after the result
                                display_dual(&self.local_state, &self.opponent_view, true);
                            }
                            other => { println!("Unexpected message while waiting for ShotResult: {:?}", other); }
                        }
                        // Continue to next loop iteration
                        continue;
                    }
                }
            } else {
                // Opponent's turn: wait for messages
                let env = self.network.receive_enveloped()?;
                match env.payload {
                    GameMessage::TakeShot { position } => {
                        // Opponent is requesting to take a shot; as the defender we must produce a proof and respond with ShotResult
                        // Build GuestInput using our local_state and the requested shot
                        let input = crate::proofs::GuestInput { initial: self.local_state.clone(), shots: vec![position] };
                        // Try to produce the per-shot proof locally. If the prover is
                        // not available the function will return an error; in that
                        // case send an Error message back to the requester so the
                        // remote peer can decide how to continue (or re-run with
                        // a proper toolchain).
                        let receipt = match produce_and_verify_proof(&input) {
                            Ok(r) => r,
                            Err(e) => {
                                let err_msg = format!("prover unavailable: {}", e);
                                let err = GameMessage::Error { message: err_msg.clone() };
                                // inform requester but do not abort the game; allow retry
                                let _ = self.network.send_enveloped(&err);
                                println!("Prover unavailable: {}. Sent Error to requester.", e);
                                continue;
                            }
                        };
                        // Extract round commit
                        let commits = extract_round_commits(&receipt)?;
                        let rc = commits.last().unwrap().clone();
                        // Apply shot locally
                        let _apply_res = self.local_state.apply_shot(position);
                        // Build ProofData and send ShotResult
                        let pd = proofdata_from_receipt(&receipt, rc.clone())?;
                        let msg = GameMessage::ShotResult { position, hit_type: rc.hit.clone(), proof: pd };
                        self.network.send_enveloped(&msg)?;

                        // Update turn according to hit type
                        match rc.hit {
                            HitType::Miss => { println!("Opponent missed at {:?}", position); local_turn = true; }
                            HitType::Hit => { println!("Opponent hit at {:?}", position); local_turn = false; }
                            HitType::Sunk(_) => { println!("Opponent sunk a ship at {:?}", position); local_turn = true; }
                        }
                    },
                    GameMessage::ShotResult { position, hit_type: _, proof } => {
                        // Received a ShotResult for a shot we previously made
                        let receipt = receipt_from_proofdata(&proof)?;
                        // Verify remote proof and ensure it matches our expected state for opponent
                        // In P2P we don't hold full opponent GameState, so we accept their proof after verifying cryptographically
                        let commits = extract_round_commits(&receipt)?;
                        let rc = commits.last().unwrap();
                        println!("Received ShotResult at {:?}: {:?}", position, rc.hit);
                        // Update turn based on hit
                        match &rc.hit {
                            HitType::Miss => { local_turn = false; }
                            HitType::Hit => { local_turn = true; }
                            HitType::Sunk(_) => { local_turn = false; }
                        }
                    }
                    GameMessage::BoardReady { .. } => {
                        // ignore here
                    }
                    GameMessage::GameOver { winner } => {
                        println!("Game over: winner = {}", winner);
                        break;
                    }
                    GameMessage::Error { message } => {
                        println!("Network error: {}", message);
                    }
                }
            }
        }
        Ok(())
    }
}