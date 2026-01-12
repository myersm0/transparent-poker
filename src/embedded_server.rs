use std::net::TcpListener;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use crate::net::GameServer;

pub struct EmbeddedServer {
	port: u16,
	_handle: JoinHandle<()>,
}

impl EmbeddedServer {
	pub fn start() -> std::io::Result<Self> {
		let listener = TcpListener::bind("127.0.0.1:0")?;
		let port = listener.local_addr()?.port();

		let (ready_tx, ready_rx) = mpsc::channel();

		let handle = thread::spawn(move || {
			let server = GameServer::new();
			ready_tx.send(()).ok();
			server.run_with_listener(listener);
		});

		ready_rx.recv().ok();

		Ok(Self {
			port,
			_handle: handle,
		})
	}

	pub fn port(&self) -> u16 {
		self.port
	}

	pub fn addr(&self) -> String {
		format!("127.0.0.1:{}", self.port)
	}
}
