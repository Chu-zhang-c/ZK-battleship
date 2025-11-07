use anyhow::Context;
use crate::network_protocol::GameMessage;
use serde_json;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

pub struct NetworkConnection {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
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
        Ok(Self { stream, reader })
    }

    /// Client: Connect to a host
    pub fn connect(host: &str, port: u16) -> anyhow::Result<Self> {
        println!("🌐 Connecting to {}:{}...", host, port);
        let stream = TcpStream::connect(format!("{}:{}", host, port))?;
        println!("✓ Connected to opponent!");
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader })
    }

    /// Send a message
    pub fn send(&mut self, message: &GameMessage) -> anyhow::Result<()> {
        let json = serde_json::to_string(message)?;
        writeln!(self.stream, "{}", json)?;
        self.stream.flush()?;
        Ok(())
    }

    /// Receive a message (blocking)
    pub fn receive(&mut self) -> anyhow::Result<GameMessage> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        let message: GameMessage = serde_json::from_str(&line).context("failed to parse incoming message")?;
        Ok(message)
    }
}
