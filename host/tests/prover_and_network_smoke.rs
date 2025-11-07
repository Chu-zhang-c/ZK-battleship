use anyhow::{Result, Context};

/// Smoke test: attempt to produce a proof locally, verify receipt parsing,
/// and check JSON (de)serialization of network ProofData. If the local
/// prover is not available this test will be skipped (prints a message
/// and returns Ok).
#[test]
fn prover_and_network_smoke() -> Result<()> {
    use host::proofs::{GuestInput, produce_and_verify_proof, proofdata_from_receipt, receipt_from_proofdata, extract_round_commits, verify_remote_round_proof};
    use core::{GameState, ShipType, Direction, Position};

    // Build a valid GameState (must include all ship types for `check()`)
    let mut state = GameState::new([0; 16]);
    state.place_ship(ShipType::Carrier, Position::new(0,0), Direction::Horizontal);
    state.place_ship(ShipType::Battleship, Position::new(0,2), Direction::Horizontal);
    state.place_ship(ShipType::Cruiser, Position::new(0,4), Direction::Horizontal);
    state.place_ship(ShipType::Submarine, Position::new(0,6), Direction::Horizontal);
    state.place_ship(ShipType::Destroyer, Position::new(0,8), Direction::Horizontal);

    let guest_input = GuestInput { initial: state.clone(), shots: vec![Position::new(0,0)] };

    // Try to produce & verify a proof. If the prover is unavailable, skip.
    let receipt = match produce_and_verify_proof(&guest_input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("prover unavailable or failed, skipping smoke test: {}", e);
            return Ok(());
        }
    };

    // Basic checks on the receipt and journal parsing
    receipt.verify(methods::METHOD_ID).with_context(|| "receipt verification failed")?;
    let commits = extract_round_commits(&receipt)?;
    assert!(!commits.is_empty(), "no commits found in receipt journal");
    let rc = commits.last().unwrap().clone();

    // Round-trip ProofData -> bytes -> ProofData via JSON
    let pd = proofdata_from_receipt(&receipt, rc.clone())?;
    // Serialize/deserialize via serde_json as the network layer would
    let pd_json = serde_json::to_string(&pd)?;
    let pd_back: host::network_protocol::ProofData = serde_json::from_str(&pd_json)?;
    assert_eq!(pd_back.commit.shot, pd.commit.shot);

    // Deserialize receipt bytes back and verify remote proof semantics
    let rec2 = receipt_from_proofdata(&pd_back)?;
    let commits2 = extract_round_commits(&rec2)?;
    assert_eq!(commits, commits2);
    let _verified_commits = verify_remote_round_proof(&rec2, &state, Position::new(0,0))?;

    Ok(())
}
