use risc0_zkvm::guest::env;
use serde::{Deserialize};

// Import canonical types from the core crate. `GameState::commit()` and
// `RoundCommit` are used to produce the public commitments that the
// verifier will later check.
use core::{GameState, RoundCommit, HitType, Position};

/// Input supplied to the guest prover.
/// - `initial`: the initial board placement (authoritative GameState)
/// - `shots`: a list of shots (in order) for which the guest will emit
///   per-round commits.
#[derive(Deserialize)]
struct GuestInput {
    initial: GameState,
    shots: Vec<Position>,
}

fn main() {
    // Read the public input (or witness depending on your protocol).
    // The `env::read()` will deserialize from the host-supplied input.
    let input: GuestInput = env::read();

    // Start from the provided initial board state. For ZK protocols the
    // prover should ensure the initial state satisfies invariants (e.g.,
    // `check()`), otherwise the proof should fail.
    let mut state: GameState = input.initial;
    if !state.check() {
        // Invalid initial board -> abort proof generation.
        panic!("initial GameState failed validation");
    }

    // Commit the initial board state and publish it to the journal so the
    // verifier can observe the initial commit value.
    let initial_commit = state.commit();
    env::commit(&initial_commit);

    // For each shot, record the old/new state commits and the hit result
    // in a `RoundCommit` which is written to the journal.
    for shot in input.shots {
        let old_state = state.commit();

        // Apply the shot. Per the core API, `apply_shot` returns `None`
        // for out-of-bounds or already-shot cells. Instead of panicking we
        // treat such cases as a harmless no-op and record a Miss. This
        // prevents the guest from aborting the proof when a remote peer
        // requests an invalid/repeated shot; the host should still reject
        // repeated shots at the protocol level if desired.
        let hit = match state.apply_shot(shot) {
            Some(h) => h,
            None => {
                // Do not mutate state; represent as a Miss so the proof
                // remains decidable by the verifier.
                HitType::Miss
            }
        };

        let new_state = state.commit();

        let round = RoundCommit { old_state, new_state, shot, hit };
        env::commit(&round);
    }
}
