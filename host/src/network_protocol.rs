use serde::{Deserialize, Serialize};
use risc0_zkvm::sha::Digest;
use core::{HitType, Position, RoundCommit};

/// Messages sent between players over the network. JSON lines framing.
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
