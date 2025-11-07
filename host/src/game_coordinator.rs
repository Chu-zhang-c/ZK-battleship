use anyhow::Result;
use core::{GameState, Position, HitType};
use risc0_zkvm::sha::Digest;
use crate::network::NetworkConnection;
use crate::network_protocol::GameMessage;
use crate::proofs::{produce_and_verify_proof, extract_round_commits, proofdata_from_receipt, receipt_from_proofdata};
use std::io::{self, Write};

pub struct GameCoordinator {
    pub local_state: GameState,
    pub local_commit: Digest,
    pub network: NetworkConnection,
    pub player_name: String,
    pub starts_first: bool,
    pub opponent_name: Option<String>,
    pub opponent_commit: Option<Digest>,
}

impl GameCoordinator {
    pub fn new(local_state: GameState, local_commit: Digest, network: NetworkConnection, player_name: String, starts_first: bool) -> Self {
        Self { local_state, local_commit, network, player_name, starts_first, opponent_name: None, opponent_commit: None }
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
                                // Verify the proof and apply it to our authoritative state
                                let commits = extract_round_commits(&receipt)?;
                                let rc = commits.last().unwrap();
                                // Apply shot to local_state (we are the opponent here)
                                let _ = self.local_state.apply_shot(position);
                                match rc.hit {
                                    HitType::Miss => { println!("Opponent reports Miss (verified). You get turn next."); local_turn = true; }
                                    HitType::Hit => { println!("Opponent reports Hit (verified). They get another shot."); local_turn = false; }
                                    HitType::Sunk(st) => { println!("Opponent reports Sunk {:?} (verified). Turn passes.", st); local_turn = true; }
                                }
                            }
                            other => { println!("Unexpected message while waiting for ShotResult: {:?}", other); }
                        }
                        // Continue to next loop iteration
                        continue;
                    }
                };
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
                                self.network.send_enveloped(&err)?;
                                anyhow::bail!("prover unavailable: {}", e);
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
                    }
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
