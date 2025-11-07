use anyhow::Context;
use serde_json;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

pub struct NetworkConnection {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    match_id: Option<uuid::Uuid>,
    next_seq: u64,
    expected_seq: u64,
}

impl NetworkConnection {
    /// Host: Create a server and wait for connection
    pub fn host(port: u16) -> anyhow::Result<Self> {
        println!("🌐 Starting server on port {}...", port);
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
        println!("⏳ Waiting for opponent to connect...");
        let (stream, addr) = listener.accept()?;
        println!("✓ Opponent connected from: {}", addr);
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader, match_id: None, next_seq: 0, expected_seq: 0 })
    }

    /// Client: Connect to a host
    pub fn connect(host: &str, port: u16) -> anyhow::Result<Self> {
        println!("🌐 Connecting to {}:{}...", host, port);
        let stream = TcpStream::connect(format!("{}:{}", host, port))?;
        println!("✓ Connected to opponent!");
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader, match_id: None, next_seq: 0, expected_seq: 0 })
    }

    /// Host-side handshake: generate match_id, send our BoardReady, then
    /// receive opponent's BoardReady. Returns (opponent_name, opponent_commit, opponent_proof)
    pub fn handshake_as_host(&mut self, player_name: &str, commitment: risc0_zkvm::sha::Digest, proof: Option<crate::network_protocol::ProofData>) -> anyhow::Result<(String, risc0_zkvm::sha::Digest, Option<crate::network_protocol::ProofData>)> {
        use crate::network_protocol::{GameMessage, Envelope};
        let match_id = uuid::Uuid::new_v4();
        self.match_id = Some(match_id);

        let msg = GameMessage::BoardReady { commitment, player_name: player_name.to_string(), proof };
        let env = Envelope::new(match_id, self.next_seq, msg);
        let json = serde_json::to_string(&env)?;
        writeln!(self.stream, "{}", json)?;
        self.stream.flush()?;
        self.next_seq = self.next_seq.wrapping_add(1);

        // Wait for opponent's BoardReady
        let resp = self.receive_enveloped()?;
        if let crate::network_protocol::GameMessage::BoardReady { commitment: opp_commit, player_name: opp_name, proof: opp_proof } = resp.payload {
            Ok((opp_name, opp_commit, opp_proof))
        } else {
            anyhow::bail!("expected BoardReady from opponent during handshake")
        }
    }

    /// Client-side handshake: receive host's BoardReady to set match_id, then send ours.
    pub fn handshake_as_client(&mut self, player_name: &str, commitment: risc0_zkvm::sha::Digest, proof: Option<crate::network_protocol::ProofData>) -> anyhow::Result<(String, risc0_zkvm::sha::Digest, Option<crate::network_protocol::ProofData>)> {
        use crate::network_protocol::{GameMessage, Envelope};
        // Receive host's initial BoardReady
        let env = self.receive_enveloped()?;
        if let crate::network_protocol::GameMessage::BoardReady { commitment: host_commit, player_name: host_name, proof: host_proof } = env.payload {
            // adopt match id from host
            self.match_id = Some(env.match_id);
            // send our BoardReady with same match_id
            let msg = GameMessage::BoardReady { commitment, player_name: player_name.to_string(), proof };
            let out = Envelope::new(env.match_id, self.next_seq, msg);
            let json = serde_json::to_string(&out)?;
            writeln!(self.stream, "{}", json)?;
            self.stream.flush()?;
            self.next_seq = self.next_seq.wrapping_add(1);
            Ok((host_name, host_commit, host_proof))
        } else {
            anyhow::bail!("expected BoardReady from host during handshake")
        }
    }

    /// Send a message
    /// Send a message wrapped in an Envelope (match_id + seq).
    pub fn send_enveloped(&mut self, payload: &crate::network_protocol::GameMessage) -> anyhow::Result<()> {
        use crate::network_protocol::Envelope;
        // Ensure we have a match_id; the caller should set it during handshake.
        let match_id = if let Some(id) = self.match_id { id } else { uuid::Uuid::new_v4() };
        let env = Envelope::new(match_id, self.next_seq, payload.clone());
        let json = serde_json::to_string(&env)?;
        writeln!(self.stream, "{}", json)?;
        self.stream.flush()?;
        self.next_seq = self.next_seq.wrapping_add(1);
        Ok(())
    }

    /// Receive a message (blocking)
    /// Receive an enveloped message and verify match_id and sequence number.
    pub fn receive_enveloped(&mut self) -> anyhow::Result<crate::network_protocol::Envelope> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        let env: crate::network_protocol::Envelope = serde_json::from_str(&line).context("failed to parse incoming envelope")?;

        // If we don't yet have a match_id, accept the first one seen
        if self.match_id.is_none() {
            self.match_id = Some(env.match_id);
        }

        // Validate match id
        if let Some(id) = self.match_id {
            if env.match_id != id {
                anyhow::bail!("mismatched match_id: expected {} got {}", id, env.match_id);
            }
        }

        // Validate sequence
        if env.seq != self.expected_seq {
            anyhow::bail!("unexpected sequence number: expected {} got {}", self.expected_seq, env.seq);
        }
        self.expected_seq = self.expected_seq.wrapping_add(1);

        Ok(env)
    }
}
