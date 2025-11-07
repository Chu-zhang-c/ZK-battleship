use anyhow::{Context, Result, bail};
use core::{GameState, Position, RoundCommit};
use methods::{METHOD_ELF, METHOD_ID};
use risc0_zkvm::{default_prover, ExecutorEnv, Receipt};
use serde::Serialize;
use std::io::Cursor;

#[derive(Serialize)]
pub struct GuestInput {
    pub initial: GameState,
    pub shots: Vec<Position>,
}

/// Run the RISC0 guest with `input`, produce a receipt, verify it, and
/// return the verified receipt.
pub fn produce_and_verify_proof(input: &GuestInput) -> Result<Receipt> {
    let env = ExecutorEnv::builder().write(input)?.build()?;
    let prover = default_prover();
    let prove_info = prover.prove(env, METHOD_ELF)?;
    let receipt = prove_info.receipt;
    // Verify the receipt against the method id
    receipt.verify(METHOD_ID)?;
    Ok(receipt)
}

/// Extract all RoundCommit entries from the receipt journal. The guest
/// commits the initial board, then one RoundCommit per shot; this
/// function deserializes them in order using bincode.
pub fn extract_round_commits(receipt: &Receipt) -> Result<Vec<RoundCommit>> {
    let mut commits = Vec::new();
    let bytes: &[u8] = &receipt.journal;
    let mut cursor = Cursor::new(bytes);
    while (cursor.position() as usize) < bytes.len() {
        let commit: RoundCommit = bincode::deserialize_from(&mut cursor)?;
        commits.push(commit);
    }
    Ok(commits)
}

/// Verify a remote receipt's cryptographic proof and validate that the
/// RoundCommit sequence matches the expected transition from `old_state`
/// after applying `shot` to the authoritative `server_state`.
pub fn verify_remote_round_proof(receipt: &Receipt, server_state: &GameState, shot: Position) -> Result<Vec<RoundCommit>> {
    // Cryptographically verify the receipt
    receipt.verify(METHOD_ID)?;

    // Extract commits from the journal (assumes bincode serialisation)
    let commits = extract_round_commits(receipt)
        .context("failed to extract round commits from receipt journal")?;

    if commits.is_empty() {
        anyhow::bail!("no commits found in receipt journal");
    }

    // For simple guests that emit one initial commit + one round commit per run,
    // the last commit is the one we care about.
    let round_commit = commits.last().unwrap();

    // Ensure the round_commit.shot matches the expected shot
    if round_commit.shot != shot {
        anyhow::bail!("commit shot mismatch: expected {:?}, got {:?}", shot, round_commit.shot);
    }

    // Verify old_state equals the server's current commit digest
    let server_old = server_state.commit();
    if round_commit.old_state != server_old {
        anyhow::bail!("old_state digest mismatch: possible replay or desync");
    }

    // Apply shot to a clone of server_state and compute expected new digest
    let mut clone = server_state.clone();
    let apply_res = clone.apply_shot(shot);
    match apply_res {
        None => anyhow::bail!("server would reject shot (OOB or already-shot)"),
        Some(_hit) => {
            let expected_new = clone.commit();
            if round_commit.new_state != expected_new {
                anyhow::bail!("new_state digest mismatch after applying shot on server");
            }
        }
    }

    Ok(commits)
}

/// Convert a Receipt and RoundCommit into a serializable ProofData structure
/// defined in `network_protocol` for transport.
pub fn proofdata_from_receipt(receipt: &Receipt, commit: RoundCommit) -> Result<crate::network_protocol::ProofData> {
    let receipt_bytes = bincode::serialize(receipt)?;
    Ok(crate::network_protocol::ProofData { receipt_bytes, commit })
}

/// Reconstruct a Receipt from ProofData received over the network.
pub fn receipt_from_proofdata(pd: &crate::network_protocol::ProofData) -> Result<Receipt> {
    let receipt: Receipt = bincode::deserialize(&pd.receipt_bytes)?;
    Ok(receipt)
}
