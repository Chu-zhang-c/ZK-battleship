use serde::{Deserialize, Serialize};
// Removed duplicate serde import
use risc0_zkvm::sha::Digest;
use core::{HitType, Position, RoundCommit};
use uuid::Uuid;

/// Core game messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameMessage {
    /// Initial handshake: send board commitment + optional proof
    BoardReady {
        commitment: Digest,
        player_name: String,
        proof: Option<ProofData>,
    },

    /// Request to take a shot
    TakeShot {
        position: Position,
    },

    /// Response with ZK proof of hit/miss (proof required)
    ShotResult {
        position: Position,
        hit_type: HitType,
        proof: ProofData,
    },

    /// Game over notification
    GameOver {
        winner: String,
    },

    /// Error message
    Error {
        message: String,
    },
}

/// Serializable proof data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofData {
    pub receipt_bytes: Vec<u8>,
    pub commit: RoundCommit,
}

impl ProofData {
    pub fn from_bytes(receipt_bytes: Vec<u8>, commit: RoundCommit) -> Self {
        Self { receipt_bytes, commit }
    }
}

/// Envelope that wraps every message with a match id and sequence number.
///
/// - `match_id` ties messages to a particular match/session and prevents
///   cross-match replay.
/// - `seq` is a monotonically increasing sequence number per-peer to
///   prevent replay and enforce ordering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub match_id: Uuid,
    pub seq: u64,
    pub payload: GameMessage,
    /// Optional authentication token (recommend using TLS + auth in prod)
    pub auth_token: Option<String>,
}

impl Envelope {
    pub fn new(match_id: Uuid, seq: u64, payload: GameMessage) -> Self {
        Self { match_id, seq, payload, auth_token: None }
    }
}
