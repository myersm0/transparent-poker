use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::net::protocol::*;

pub struct GameClient {
	stream: TcpStream,
	rx: Receiver<ServerMessage>,
}

impl GameClient {
	pub fn connect(addr: &str) -> std::io::Result<Self> {
		let stream = TcpStream::connect(addr)?;
		stream.set_read_timeout(Some(Duration::from_millis(100)))?;

		let reader = stream.try_clone()?;
		let (tx, rx) = mpsc::channel();

		thread::spawn(move || {
			read_loop(reader, tx);
		});

		Ok(Self { stream, rx })
	}

	pub fn send(&mut self, msg: &ClientMessage) -> std::io::Result<()> {
		let data = encode_message(msg);
		self.stream.write_all(&data)
	}

	pub fn try_recv(&self) -> Option<ServerMessage> {
		self.rx.try_recv().ok()
	}

	pub fn recv(&self) -> Option<ServerMessage> {
		self.rx.recv().ok()
	}

	pub fn recv_timeout(&self, timeout: Duration) -> Option<ServerMessage> {
		self.rx.recv_timeout(timeout).ok()
	}

	pub fn login(&mut self, username: &str) -> std::io::Result<()> {
		self.send(&ClientMessage::Login {
			username: username.to_string(),
		})
	}

	pub fn list_tables(&mut self) -> std::io::Result<()> {
		self.send(&ClientMessage::ListTables)
	}

	pub fn join_table(&mut self, table_id: &str) -> std::io::Result<()> {
		self.send(&ClientMessage::JoinTable {
			table_id: table_id.to_string(),
		})
	}

	pub fn leave_table(&mut self) -> std::io::Result<()> {
		self.send(&ClientMessage::LeaveTable)
	}

	pub fn ready(&mut self) -> std::io::Result<()> {
		self.send(&ClientMessage::Ready)
	}

	pub fn add_ai(&mut self, strategy: Option<String>) -> std::io::Result<()> {
		self.send(&ClientMessage::AddAI { strategy })
	}

	pub fn remove_ai(&mut self, seat: crate::events::Seat) -> std::io::Result<()> {
		self.send(&ClientMessage::RemoveAI { seat })
	}

	pub fn action(&mut self, action: crate::events::PlayerAction) -> std::io::Result<()> {
		self.send(&ClientMessage::Action { action })
	}

	pub fn chat(&mut self, text: &str) -> std::io::Result<()> {
		self.send(&ClientMessage::Chat {
			text: text.to_string(),
		})
	}
}

fn read_loop(mut reader: TcpStream, tx: Sender<ServerMessage>) {
	let mut buf = vec![0u8; 4096];
	let mut pending = Vec::new();

	loop {
		match reader.read(&mut buf) {
			Ok(0) => break,
			Ok(n) => {
				pending.extend_from_slice(&buf[..n]);
				while let Some(msg) = try_decode_message(&mut pending) {
					if tx.send(msg).is_err() {
						return;
					}
				}
			}
			Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
				continue;
			}
			Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
				continue;
			}
			Err(_) => break,
		}
	}
}

fn try_decode_message(buf: &mut Vec<u8>) -> Option<ServerMessage> {
	if buf.len() < 4 {
		return None;
	}
	let len = decode_length(buf)? as usize;
	if buf.len() < 4 + len {
		return None;
	}
	let json = String::from_utf8_lossy(&buf[4..4 + len]).to_string();
	buf.drain(..4 + len);
	serde_json::from_str(&json).ok()
}
