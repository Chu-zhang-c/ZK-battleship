# ZK Battleship

A two‑player Battleship game secured by zero‑knowledge proofs using the RISC0 zkVM. Players commit to their boards up front and then prove each turn’s state transition without revealing hidden ships, exchanging only commitments and minimal revelations from shots.

---

## At a glance
- zk proofs (RISC0 v3): guest proves each turn; host/shooter verifies receipts
- Clean UX: noisy prover/verification logs are suppressed by default
- Secure channel: TLS transport, X25519 DH per match, HMAC-SHA256 on every message
- Replay/order defense: per‑message sequence numbers (in‑memory)
- No on‑disk persistence by default (no receipts or match state written)

---

## Contents
- Prerequisites
- Build
- Run locally (no network)
- Run over network (TLS)
- How it works
- Security model (what’s covered, what isn’t)
- Troubleshooting
- Development notes
- License

---

## Prerequisites
- Rust toolchain (stable). The repo includes `rust-toolchain.toml` for consistency.
- OpenSSL development libraries (for the `openssl` crate used by TLS).
- RISC0 build prerequisites (for compiling the guest method). If you can build this repo once, you’re good.

Tip (Ubuntu/Debian): `sudo apt-get install build-essential pkg-config libssl-dev`.

---

## Build
```bash
# from repo root
cargo build --release
```

This compiles the host CLI, the core game logic, and the guest method.

---

## Run locally (no network)
Quick two‑human hot‑seat mode in one terminal:
```bash
cargo run -p host --release
# Choose: 1) Local 2-player
```
Follow the prompts to place ships for both players and play on one machine.

---

## Run over network (TLS)
You’ll use two terminals or two machines: one host (server), one client.

### 1) Prepare TLS material
Use the provided `certs/` (already in the repo) or supply your own PEM files:
- Server: `server.crt`, `server.key`
- CA (to validate the server on the client): `ca.crt`

For quick local testing you can reuse the included sample certs, but for a real deployment you should generate and protect your own keys/certs.

### 2) Start the host (server)
Terminal A:
```bash
cd /home/user/ZK-battleship
export BATTLE_SERVER_CERT="$PWD/certs/server.crt"
export BATTLE_SERVER_KEY="$PWD/certs/server.key"
# Optional if you want to validate client certs (mTLS off by default):
# export BATTLE_CA_CERT="$PWD/certs/ca.crt"

cargo run -p host --release
# Choose: 2) Host a networked game
# Enter port (default 7878)
# Enter your player name and place ships
```

### 3) Join from the client
Terminal B:
```bash
cd /home/user/ZK-battleship
# Client validates the server using the CA cert
export BATTLE_CA_CERT="$PWD/certs/ca.crt"

# Optional: enable mutual TLS if the host is configured to require it
# export BATTLE_CLIENT_CERT="$PWD/certs/client.crt"
# export BATTLE_CLIENT_KEY="$PWD/certs/client.key"

cargo run -p host --release
# Choose: 3) Join a networked game
# Enter the host IP (127.0.0.1 for local), and the same port
# Enter your player name and place ships
```

That’s it—you’re playing over an encrypted channel. The game UI is plain ASCII; your board commitment is exchanged during the handshake.

---

## How it works
- Commitment: Each player’s board is committed to via a SHA‑based digest (RISC0’s `sha::Digest`).
- Turn proving: The guest method runs on the committed state with the shot, producing a journal:
  - initial GameState commit digest
  - one `RoundCommit` per processed shot
- Receipts: The host/shooter verifies the receipt against the method ID and extracts the relevant `RoundCommit`:
  - `old_state` must match the expected opponent commitment
  - `new_state` becomes the opponent’s updated commitment for the next turn
- Networking:
  - TLS (OpenSSL) protects transport
  - X25519 DH over TLS derives a per‑match secret
  - Each JSON envelope includes `match_id`, `seq`, `payload`, and an HMAC‑SHA256 token over the envelope (without the token) using the per‑match secret
  - Sequence numbers provide in‑session replay/order protection

---

## Security model
What’s covered
- Confidentiality and integrity in transit (TLS)
- Session‑bound integrity of game messages (HMAC + sequence numbers)
- Cryptographic correctness of turn state transitions (RISC0 receipt verification)
- Log hygiene: internal RISC0/ark_* DEBUG logs are suppressed by default for a clean UX

What isn’t (by default)
- Client authentication: mutual TLS is supported by code paths but not enforced by default
- Cross‑restart replay protection: sequence state is in‑memory only (simpler UX). If either side restarts, start a new match. If you need cross‑restart protection, add persistent seq storage or signed, expiring session tokens.
- Attestation of remote binary: you don’t cryptographically prove the peer is running an unmodified build. Consider TEEs or service attestation for that.

Hardening options
- Enable mutual TLS (mTLS) and/or pin the server certificate fingerprint on the client
- Re‑enable persistent expected_seq if you want cross‑restart replay defense
- Add per‑connection timeouts/rate limits and zeroize secrets after use

---

## Troubleshooting
- TLS: “unknown certificate” → ensure both sides reference the same `ca.crt` and that the server cert’s SAN matches the hostname/IP you connect to
- “auth token missing or invalid” → handshake mismatch; restart both sides to renegotiate DH and ensure env vars point to the same CA/server certs
- Build failures in guest method → install platform build tools and OpenSSL dev headers
- Too many internal logs → already suppressed; if you want more detail, edit `host/src/main.rs` to relax the filtering

---

## Development notes
- Workspace crates:
  - `core/` – pure game logic and commitments
  - `methods/` – zk guest (RISC‑V) and build outputs (`METHOD_ELF`, `METHOD_ID`)
  - `host/` – CLI, networking, proof orchestration
- Logging: `host/src/main.rs` installs a subscriber that drops targets starting with `risc0` or `ark_` and caps others at INFO. Adjust there if you need verbose tracing.
- No disk persistence: receipts and match sequence files were intentionally removed for a simpler UX.

---

## License
See `LICENSE`.
