use anyhow::{Context, Result, bail};
use core::{GameState, Position, RoundCommit};
use uuid::Uuid;
use methods::{METHOD_ELF, METHOD_ID};
use risc0_zkvm::{default_prover, ExecutorEnv, Receipt};
use risc0_zkvm::serde::{Deserializer, Error as SerdeError};
use serde::Serialize;
use anyhow::anyhow;
use risc0_zkvm::sha::Digest;
use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use base64::{engine::general_purpose, Engine as _};
use serde_json;

#[derive(Serialize)]
pub struct GuestInput {
    pub initial: GameState,
    pub shots: Vec<Position>,
    pub match_id: Uuid,
    pub seq: u64,
}

/// NOTE: In this development environment the riscv guest prover APIs and
/// the riscv toolchain may not be available. To keep the host crate
/// compiling while you iterate on host-side code we provide stubbed
/// implementations here. These return an error indicating the prover is
/// disabled. When you install the RISC0 toolchain and want host-local
/// proving, replace these stubs with calls to the real prover API.
pub fn produce_and_verify_proof(input: &GuestInput) -> Result<Receipt> {
    // Build an executor environment and write the guest input into stdin for the guest
    let mut builder = ExecutorEnv::builder();
    builder.write(input).context("serializing guest input")?;
    let env = builder.build().context("building executor env")?;

    // Run the default prover (chosen via RISC0_PROVER env or feature flags)
    let info = default_prover().prove(env, METHOD_ELF).context("prover failed")?;
    let receipt = info.receipt;

    // Verify the receipt locally against the expected METHOD_ID
    receipt.verify(METHOD_ID).context("receipt verification failed")?;

    Ok(receipt)
}

pub fn extract_round_commits(receipt: &Receipt) -> Result<Vec<RoundCommit>> {
    // The journal contains a sequence of committed objects. The guest writes
    // an initial GameState commit (Digest) followed by one RoundCommit per
    // shot. We stream-deserialize over the journal bytes to extract the
    // RoundCommit entries while skipping the initial digest.

    let bytes = &receipt.journal.bytes;

    // Convert the journal bytes into a Vec<u32> (little-endian). We avoid
    // depending on `bytemuck` here to keep the host crate minimal.
    if bytes.len() % 4 != 0 {
        return Err(anyhow!("journal bytes length not a multiple of 4"));
    }
    let mut owned_words: Vec<u32> = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let w = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        owned_words.push(w);
    }
    let words_slice: &[u32] = owned_words.as_slice();

    let mut deser = Deserializer::new(words_slice);

    // First decode the initial digest (GameState::commit() output) and ignore it.
    // Digest type used by GameState::commit
    let _: risc0_zkvm::sha::Digest = match serde::Deserialize::deserialize(&mut deser) {
        Ok(d) => d,
        Err(e) => {
            return Err(anyhow!("failed to read initial commit from journal: {:?}", e));
        }
    };

    // Now read zero-or-more RoundCommit entries until we hit EOF.
    let mut commits: Vec<RoundCommit> = Vec::new();
    loop {
        match serde::Deserialize::deserialize(&mut deser) {
            Ok(rc) => commits.push(rc),
            Err(SerdeError::DeserializeUnexpectedEnd) => break,
            Err(e) => return Err(anyhow!("failed to deserialize RoundCommit: {:?}", e)),
        }
    }

    Ok(commits)
}

/// Verify cryptographic integrity of a remote round proof and ensure it is
/// bound to the provided optional match/session id and sequence number.
/// If `expected_match` or `expected_seq` are None, those checks are skipped
/// (useful for local single-process proofs/tests).
pub fn verify_remote_round_proof(receipt: &Receipt, server_state: &GameState, shot: Position, expected_match: Option<Uuid>, expected_seq: Option<u64>) -> Result<Vec<RoundCommit>> {
    // Verify cryptographic integrity and extract commits
    receipt.verify(METHOD_ID).context("receipt verification failed")?;
    let commits = extract_round_commits(receipt)?;

    if commits.is_empty() {
        bail!("no round commits found in receipt")
    }

    // Ensure the receiver's known server_state matches the pre-image of the first commit
    let expected_old = server_state.commit();
    if commits[0].old_state != expected_old {
        bail!("mismatched base state in provided proof")
    }

    // Ensure one of the commits corresponds to the shot in question
    if !commits.iter().any(|c| c.shot == shot) {
        bail!("receipt does not contain a commit for the requested shot")
    }

    // If match/session binding was requested, ensure at least one commit
    // in the proof carries the expected match id and sequence number.
    if let Some(exp_mid) = expected_match {
        if let Some(exp_seq) = expected_seq {
            if !commits.iter().any(|c| c.match_id == exp_mid && c.seq == exp_seq) {
                bail!("receipt proof not bound to expected match_id/seq");
            }
        }
    }

    Ok(commits)
}

pub fn proofdata_from_receipt(receipt: &Receipt, commit: RoundCommit) -> Result<crate::network_protocol::ProofData> {
    let receipt_bytes = bincode::serialize(receipt).context("serializing Receipt to bytes")?;
    Ok(crate::network_protocol::ProofData::from_bytes(receipt_bytes, commit))
}

pub fn receipt_from_proofdata(pd: &crate::network_protocol::ProofData) -> Result<Receipt> {
    let receipt: Receipt = bincode::deserialize(&pd.receipt_bytes).context("deserializing Receipt from bytes")?;
    Ok(receipt)
}

/// Verify a receipt for a shooter (who does not hold the defender's full
/// GameState). This verifies the receipt cryptographically, extracts the
/// round commits, finds the commit bound to the provided match/seq and
/// shot, ensures the commit.old_state equals `expected_old`, and returns
/// the matching RoundCommit (which contains the new_state the shooter can
/// adopt as the opponent's updated commitment).
pub fn verify_shot_result_for_shooter(receipt: &Receipt, expected_old: Digest, shot: Position, expected_match: Option<Uuid>, expected_seq: Option<u64>) -> Result<RoundCommit> {
    // 1) cryptographic verification
    receipt.verify(METHOD_ID).context("receipt verification failed")?;

    // 2) extract commits
    let commits = extract_round_commits(receipt)?;
    if commits.is_empty() {
        bail!("no round commits found in receipt");
    }

    // 3) locate commit matching shot and optional binding
    let mut found: Option<RoundCommit> = None;
    for c in commits.iter() {
        if c.shot == shot {
            if let (Some(mid), Some(sq)) = (expected_match, expected_seq) {
                if c.match_id == mid && c.seq == sq {
                    found = Some(c.clone());
                    break;
                }
            } else {
                found = Some(c.clone());
                break;
            }
        }
    }

    let commit = match found {
        Some(c) => c,
        None => bail!("no matching RoundCommit found for shot/match/seq"),
    };

    // 4) ensure old_state matches expected_old (the shooter's recorded opponent commit)
    if commit.old_state != expected_old {
        bail!("commit.old_state does not match expected old digest");
    }

    // Persist receipt+commit for audit
    if let Err(e) = persist_receipt_and_commit(receipt, &commit) {
        // non-fatal: warn but continue accepting the commit
        eprintln!("warning: failed to persist receipt: {}", e);
    }

    Ok(commit)
}

fn persist_receipt_and_commit(receipt: &Receipt, commit: &RoundCommit) -> Result<()> {
    // Ensure receipts directory
    create_dir_all("receipts").context("creating receipts dir")?;
    // filename by match id
    let filename = format!("receipts/{}.log", commit.match_id);
    let mut f = OpenOptions::new().create(true).append(true).open(&filename).context("opening receipt log")?;

    // Serialize receipt bytes as base64 and commit as JSON line
    let receipt_bytes = bincode::serialize(receipt).context("serializing receipt for persistence")?;
    let receipt_b64 = general_purpose::STANDARD.encode(&receipt_bytes);
    let commit_json = serde_json::to_string(commit).context("serializing commit to json")?;
    let line = format!("{{\"seq\":{},\"receipt_b64\":\"{}\",\"commit\":{}}}\n", commit.seq, receipt_b64, commit_json);
    f.write_all(line.as_bytes()).context("writing receipt log")?;
    Ok(())
}
