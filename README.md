# ZK Battleship

A two-player Battleship game powered by zero-knowledge proofs using the RISC0 zkVM. One player can host and another can join over a TLS-secured connection. Each turn's board transition is proven inside the zkVM and only the committed board hash is shared until shots reveal information.

## Features
- Deterministic board commitment (player board hashed & committed before play)
- RISC0 zkVM guest proves each turn's state transition
- Host verifies receipts; shooter gets a receipt proving correctness of the defender's response
- Secure network channel:
  - TLS (OpenSSL) for transport encryption + server authentication
  - X25519 Diffie-Hellman over the established TLS channel to derive a per-match secret
  - HMAC-SHA256 auth token on every game message (match_id + seq + payload) preventing tampering / replay
- Sequence numbers to prevent message reordering
- All prover / verifier internal DEBUG logs suppressed by default for a clean game UI
- In-memory only (no receipts or sequence state persisted to disk)

## Crate Layout
- `core/`: Pure game logic (board, placement, state transitions, commitment)
- `methods/`: zk guest method (RISC-V ELF) + build script producing `METHOD_ELF` & `METHOD_ID`
- `host/`: CLI game orchestrator, networking, proof creation/verification

## Prerequisites
- Rust (stable toolchain; workspace includes `rust-toolchain.toml` if pinning is required)
- OpenSSL development libraries (for `openssl` crate)
- RISC0 build dependencies (to build the guest method). If you cannot or do not wish to build the guest locally, you can still run the host if the provided ELF is present.

## Quick Start (Local Two-Player Single Process)
You can run a local two-human hot-seat style game (no networking) as a fast sanity check.

```bash
cargo run -p host --release
# Choose option 1 (Local 2-player)
```

Follow prompts to place ships for Player 1 then Player 2.

## Networked Play
One player acts as the host (server) and the other as the client.

### 1. Generate / Supply TLS Material
Place PEM files (self-signed for development is fine) somewhere accessible, e.g. `certs/` (already present):
- `server.crt`, `server.key`
- `ca.crt` (issuer / self-signed root)
- (Optional) client cert/key if you want mutual TLS; by default only server is authenticated.

### 2. Environment Variables
#### Host
```bash
export BATTLE_SERVER_CERT=certs/server.crt
export BATTLE_SERVER_KEY=certs/server.key
export BATTLE_CA_CERT=certs/ca.crt   # Optional if you want to validate client certs; not required for default flow
cargo run -p host --release
# Choose option 2 (Host a networked game)
```

#### Client
```bash
export BATTLE_CA_CERT=certs/ca.crt
# Optional if mutual TLS desired:
# export BATTLE_CLIENT_CERT=certs/client.crt
# export BATTLE_CLIENT_KEY=certs/client.key
cargo run -p host --release
# Choose option 3 (Join a networked game)
```

### 3. Handshake Flow
1. Host selects option 2, enters port, player name, places ships (board committed & hashed).
2. Client selects option 3, enters host IP/port, name, places ships.
3. Both sides exchange a `BoardReady` message containing board commitment (+ optional initial proof if implemented).
4. X25519 DH runs over the TLS channel to derive a per-match secret for HMAC.
5. Turns proceed: shooter sends shot; defender proves response and returns proof + updated commitment.

## Proof Lifecycle
- Guest builds an `ExecutorEnv` with prior state + shot list.
- Prover runs the method and emits a journal: initial state digest + one `RoundCommit` per shot.
- Host / shooter verifies receipt against `METHOD_ID` and extracts relevant `RoundCommit`.
- Old state commitment must match expected; new state digest becomes the opponent's new commitment.

## Logging
All RISC0 / proving crate logs (targets starting with `risc0` or `ark_`) are completely suppressed in `main.rs` via a custom filter.
To enable verbose debugging temporarily, edit `host/src/main.rs` and adjust the filter (remove the `drop_risc_targets` filter) then rebuild.

## Security Notes
| Layer | Mechanism | Purpose |
|-------|-----------|---------|
| Transport | TLS (OpenSSL) | Confidentiality + server authentication |
| Session Secret | X25519 ephemeral DH | Derive symmetric match secret |
| Message Auth | HMAC-SHA256 over JSON envelope (without auth_token) | Integrity + replay protection (with sequence numbers) |
| ZK Integrity | RISC0 receipt verification | Ensures board transition correctness |

## Environment Summary
| Variable | Role | Required (Host) | Required (Client) |
|----------|------|-----------------|-------------------|
| BATTLE_SERVER_CERT | Server TLS certificate | Yes | No |
| BATTLE_SERVER_KEY  | Server TLS private key | Yes | No |
| BATTLE_CA_CERT     | CA cert to validate peer | Optional* | Yes |
| BATTLE_CLIENT_CERT | Client TLS cert (mTLS)    | Only if using client auth | Optional (if host requires) |
| BATTLE_CLIENT_KEY  | Client TLS key (mTLS)     | Only if using client auth | Optional (if host requires) |

* If omitted on host, client certs are not enforced.

## Troubleshooting
| Symptom | Possible Cause | Fix |
|---------|----------------|-----|
| Connection refused | Wrong port or host not running | Verify port and host process |
| TLS error: unknown certificate | CA mismatch | Ensure both sides use same `ca.crt` |
| "auth token missing or invalid" | HMAC mismatch (different DH secret) | Restart both sides to redo handshake |
| Prover errors / build fails | RISC0 toolchain missing | Install RISC0 toolchain or ensure prebuilt method ELF present |

## Development Tips
- Run tests: `cargo test -p host` (if tests added).
- Rebuild guest quickly: touch files in `methods/guest/src`.
- Adjust logging: modify the subscriber configuration in `host/src/main.rs`.

## Future Enhancements
- Persist commitments (optional audit log) — removed for simplicity now.
- UI improvements / colored boards.
- Aggregated multi-shot proofs.
- Remote attestation / Bonsai integration.

## License
See `LICENSE`.
