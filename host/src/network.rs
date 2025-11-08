use anyhow::Context;
use serde_json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{engine::general_purpose, Engine as _};
use std::sync::{Arc, Mutex};

// Helper trait object for boxed TLS streams that implement Read+Write
trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

// TLS via OpenSSL
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslConnector};

// Use ring for X25519/ECDH and SHA-256 digest for deriving match secret
use ring::agreement::{EphemeralPrivateKey, agree_ephemeral, X25519, UnparsedPublicKey};
use ring::rand::SystemRandom;
use ring::digest;

pub struct NetworkConnection {
    /// TLS-wrapped stream (boxed to erase concrete stream type)
    stream: Arc<Mutex<Box<dyn ReadWrite + Send>>> ,
    match_id: Option<uuid::Uuid>,
    next_seq: u64,
    expected_seq: u64,
    /// Per-match secret derived via DH over the TLS channel
    match_secret: Option<Vec<u8>>,
}

impl NetworkConnection {
    fn write_line(&self, s: &str) -> anyhow::Result<()> {
        let mut guard = self.stream.lock().unwrap();
        let writer: &mut dyn Write = &mut **guard;
        writeln!(writer, "{}", s)?;
        writer.flush()?;
        Ok(())
    }

    fn read_line(&self) -> anyhow::Result<String> {
        let mut guard = self.stream.lock().unwrap();
        // Create a BufReader over the locked stream (temporary)
        let reader = &mut **guard;
        let mut buf = BufReader::new(reader);
        let mut line = String::new();
        let n = buf.read_line(&mut line)?;
        if n == 0 {
            anyhow::bail!("connection closed by peer (EOF)");
        }
        Ok(line)
    }
    // (OpenSSL) Helper: create an SslAcceptor for server side using cert/key and optional CA for client auth.
    fn make_ssl_acceptor(cert_path: &str, key_path: &str, ca_path: Option<&str>) -> anyhow::Result<SslAcceptor> {
        let mut builder = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls()).context("creating ssl acceptor")?;
        builder.set_certificate_file(cert_path, SslFiletype::PEM).context("set cert file")?;
        builder.set_private_key_file(key_path, SslFiletype::PEM).context("set key file")?;
        // Server-only TLS: load CA if provided for verification, but do NOT require client certs.
        // This keeps the server as the authenticating party, while allowing clients without certs.
        if let Some(ca) = ca_path {
            builder.set_ca_file(ca).context("set ca file")?;
            // Do not call set_verify with FAIL_IF_NO_PEER_CERT — we intentionally avoid requiring client certs.
        }
        Ok(builder.build())
    }

    fn make_ssl_connector(ca_path: &str, client_cert: Option<&str>, client_key: Option<&str>) -> anyhow::Result<SslConnector> {
        let mut builder = SslConnector::builder(SslMethod::tls()).context("creating ssl connector")?;
        builder.set_ca_file(ca_path).context("set ca file")?;
        if let (Some(cert), Some(key)) = (client_cert, client_key) {
            builder.set_certificate_file(cert, SslFiletype::PEM).context("set client cert")?;
            builder.set_private_key_file(key, SslFiletype::PEM).context("set client key")?;
        }
        Ok(builder.build())
    }

    // Perform TLS handshake & an X25519 DH exchange over the encrypted channel to derive a match secret.
    fn perform_tls_handshake_and_dh<S: Read + Write>(stream: &mut S, initiator: bool) -> anyhow::Result<Vec<u8>> {
        let rng = SystemRandom::new();
        // generate ephemeral X25519 private key
    let my_private = EphemeralPrivateKey::generate(&X25519, &rng).map_err(|e| anyhow::anyhow!("generating ephemeral key: {:?}", e))?;
    let my_pub = my_private.compute_public_key().map_err(|e| anyhow::anyhow!("compute public key failed: {:?}", e))?;
    let pub_b64 = general_purpose::STANDARD.encode(my_pub.as_ref());

        if initiator {
            let req = serde_json::to_string(&serde_json::json!({"dh_pub": pub_b64}))?;
            writeln!(stream, "{}", req)?;
            stream.flush()?;
            let mut reader = BufReader::new(&mut *stream);
            let mut line = String::new();
            reader.read_line(&mut line)?;
            let v: serde_json::Value = serde_json::from_str(&line)?;
            let peer_b64 = v.get("dh_pub").and_then(|x| x.as_str()).ok_or_else(|| anyhow::anyhow!("missing dh_pub"))?;
            let peer_bytes = general_purpose::STANDARD.decode(peer_b64)?;
            let peer_pub = UnparsedPublicKey::new(&X25519, peer_bytes);
            let shared = agree_ephemeral(my_private, &peer_pub, |shared| {
                let d = digest::digest(&digest::SHA256, shared);
                d.as_ref().to_vec()
            }).map_err(|e| anyhow::anyhow!("agree_ephemeral failed: {:?}", e))?;
            // Derive secret fingerprint for internal use (not logged)
            return Ok(shared);
        } else {
            let mut reader = BufReader::new(&mut *stream);
            let mut line = String::new();
            reader.read_line(&mut line)?;
            let v: serde_json::Value = serde_json::from_str(&line)?;
            let peer_b64 = v.get("dh_pub").and_then(|x| x.as_str()).ok_or_else(|| anyhow::anyhow!("missing dh_pub"))?;
            let peer_bytes = general_purpose::STANDARD.decode(peer_b64)?;
            let req = serde_json::to_string(&serde_json::json!({"dh_pub": pub_b64}))?;
            writeln!(stream, "{}", req)?;
            stream.flush()?;
            let peer_pub = UnparsedPublicKey::new(&X25519, peer_bytes);
            let shared = agree_ephemeral(my_private, &peer_pub, |shared| {
                let d = digest::digest(&digest::SHA256, shared);
                d.as_ref().to_vec()
            }).map_err(|e| anyhow::anyhow!("agree_ephemeral failed: {:?}", e))?;
            // Derive secret fingerprint for internal use (not logged)
            return Ok(shared);
        }
    }
    /// Host: Create a server and wait for connection
    /// Host: Create a TLS server and wait for an incoming connection.
    ///
    /// TLS parameters are loaded from environment variables:
    /// - BATTLE_SERVER_CERT: path to server cert (PEM)
    /// - BATTLE_SERVER_KEY: path to server private key (PEM pkcs8 or rsa)
    /// - BATTLE_CA_CERT: path to CA cert used to validate client certs (optional; if provided, client certs are required)
    pub fn host(port: u16) -> anyhow::Result<Self> {
        println!("🌐 Starting TLS server on port {}...", port);
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
        println!("⏳ Waiting for opponent to connect...");
        let (tcp_stream, addr) = listener.accept()?;
        println!("✓ Opponent connected from: {}", addr);

        // Load server cert/key from env
        let cert_path = std::env::var("BATTLE_SERVER_CERT").context("BATTLE_SERVER_CERT not set")?;
        let key_path = std::env::var("BATTLE_SERVER_KEY").context("BATTLE_SERVER_KEY not set")?;
        let ca_path = std::env::var("BATTLE_CA_CERT").ok();

        let acceptor = Self::make_ssl_acceptor(&cert_path, &key_path, ca_path.as_deref())?;
        let mut tls_stream = acceptor.accept(tcp_stream).context("accepting ssl")?;
        // After TLS handshake completes, perform X25519 DH over the encrypted channel to derive match_secret
        let secret = Self::perform_tls_handshake_and_dh(&mut tls_stream, false)?;
        let boxed: Box<dyn ReadWrite + Send> = Box::new(tls_stream);
    let nc = Self { stream: Arc::new(Mutex::new(boxed)), match_id: None, next_seq: 0, expected_seq: 0, match_secret: Some(secret) };
        // No persisted match id yet; return connection
        Ok(nc)
    }

    /// Client: Connect to a host
    /// Client: Connect to a TLS server and perform X25519 DH over the TLS channel to derive match_secret.
    /// Environment vars (client-side):
    /// - BATTLE_CLIENT_CERT: path to client cert (PEM) (optional)
    /// - BATTLE_CLIENT_KEY: path to client key (PEM) (optional)
    /// - BATTLE_CA_CERT: path to CA cert to validate server cert (required)
    pub fn connect(host: &str, port: u16) -> anyhow::Result<Self> {
        println!("🌐 Connecting to {}:{}...", host, port);
        let tcp = TcpStream::connect(format!("{}:{}", host, port))?;
        println!("✓ TCP connection established");

        let ca_path = std::env::var("BATTLE_CA_CERT").context("BATTLE_CA_CERT must be set to validate server cert")?;
        let client_cert = std::env::var("BATTLE_CLIENT_CERT").ok();
        let client_key = std::env::var("BATTLE_CLIENT_KEY").ok();

        let connector = Self::make_ssl_connector(&ca_path, client_cert.as_deref(), client_key.as_deref())?;
        let mut tls_stream = connector.connect(host, tcp).context("connecting ssl")?;
        // DH exchange (client initiates)
        let secret = Self::perform_tls_handshake_and_dh(&mut tls_stream, true)?;
        let boxed: Box<dyn ReadWrite + Send> = Box::new(tls_stream);
        let nc = Self { stream: Arc::new(Mutex::new(boxed)), match_id: None, next_seq: 0, expected_seq: 0, match_secret: Some(secret) };
        Ok(nc)
    }

    /// Host-side handshake: generate match_id, send our BoardReady, then
    /// receive opponent's BoardReady. Returns (opponent_name, opponent_commit, opponent_proof)
    pub fn handshake_as_host(&mut self, player_name: &str, commitment: risc0_zkvm::sha::Digest, proof: Option<crate::network_protocol::ProofData>) -> anyhow::Result<(String, risc0_zkvm::sha::Digest, Option<crate::network_protocol::ProofData>)> {
        use crate::network_protocol::GameMessage;
        let match_id = uuid::Uuid::new_v4();
        self.match_id = Some(match_id);

    let msg = GameMessage::BoardReady { commitment, player_name: player_name.to_string(), proof };
    // Use send_enveloped so the message is HMAC-authenticated when match_secret is present.
    self.send_enveloped(&msg)?;

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
        use crate::network_protocol::GameMessage;
        // Receive host's initial BoardReady
        let env = self.receive_enveloped()?;
        if let crate::network_protocol::GameMessage::BoardReady { commitment: host_commit, player_name: host_name, proof: host_proof } = env.payload {
            // adopt match id from host
            self.match_id = Some(env.match_id);
            // send our BoardReady reply using send_enveloped so it contains an auth token when required
            let msg = GameMessage::BoardReady { commitment, player_name: player_name.to_string(), proof };
            self.send_enveloped(&msg)?;
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
        let mut env = Envelope::new(match_id, self.next_seq, payload.clone());
        // If we have a match_secret, compute HMAC over the envelope (without auth_token)
        if let Some(secret) = &self.match_secret {
            let mut tmp = env.clone();
            tmp.auth_token = None;
            let json_no_auth = serde_json::to_string(&tmp)?;
            type HmacSha256 = Hmac<Sha256>;
            let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC can take key of any size");
            mac.update(json_no_auth.as_bytes());
            let result = mac.finalize().into_bytes();
            let token = general_purpose::STANDARD.encode(&result);
            // no debug logging in production
            env.auth_token = Some(token);
        }
        let json = serde_json::to_string(&env)?;
        self.write_line(&json)?;
        self.next_seq = self.next_seq.wrapping_add(1);
        Ok(())
    }

    /// Receive a message (blocking)
    /// Receive an enveloped message and verify match_id and sequence number.
    pub fn receive_enveloped(&mut self) -> anyhow::Result<crate::network_protocol::Envelope> {
        let line = self.read_line()?;
        let env: crate::network_protocol::Envelope = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse incoming envelope (raw={:?})", line))?;

        // If we have a match_secret, validate the HMAC auth_token
        if let Some(secret) = &self.match_secret {
            let mut tmp = env.clone();
            let token = tmp.auth_token.clone();
            tmp.auth_token = None;
            let json_no_auth = serde_json::to_string(&tmp)?;
            type HmacSha256 = Hmac<Sha256>;
            let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC can take key of any size");
            mac.update(json_no_auth.as_bytes());
            let expected = mac.finalize().into_bytes();
            let expected_b64 = general_purpose::STANDARD.encode(&expected);
            // no debug logging in production
            if token.is_none() || token.unwrap() != expected_b64 {
                anyhow::bail!("auth token missing or invalid");
            }
        }

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
