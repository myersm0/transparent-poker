use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::oneshot;

use crate::events::{Card, GameEvent, PlayerAction, Seat, ValidActions};
use crate::net::protocol::{encode_message, ServerMessage};
use crate::players::{GameSnapshot, PlayerPort, PlayerResponse};

pub struct NetworkPlayer {
	seat: Seat,
	name: String,
	stream: Arc<Mutex<TcpStream>>,
	pending_action: Arc<Mutex<Option<oneshot::Sender<PlayerResponse>>>>,
}

impl NetworkPlayer {
	pub fn new(seat: Seat, name: String, stream: TcpStream) -> Self {
		Self {
			seat,
			name,
			stream: Arc::new(Mutex::new(stream)),
			pending_action: Arc::new(Mutex::new(None)),
		}
	}

	pub fn pending_action_sender(&self) -> Arc<Mutex<Option<oneshot::Sender<PlayerResponse>>>> {
		Arc::clone(&self.pending_action)
	}

	fn send_message(&self, msg: &ServerMessage) {
		if let Ok(mut stream) = self.stream.lock() {
			let data = encode_message(msg);
			let _ = stream.write_all(&data);
		}
	}

	fn filter_event(&self, event: &GameEvent) -> Option<GameEvent> {
		match event {
			GameEvent::HoleCardsDealt { seat, cards: _ } => {
				if *seat == self.seat {
					Some(event.clone())
				} else {
					Some(GameEvent::HoleCardsDealt {
						seat: *seat,
						cards: [
							Card { rank: '?', suit: '?' },
							Card { rank: '?', suit: '?' },
						],
					})
				}
			}
			_ => Some(event.clone()),
		}
	}
}

#[async_trait]
impl PlayerPort for NetworkPlayer {
	async fn request_action(
		&self,
		_seat: Seat,
		valid_actions: ValidActions,
		_game_state: &GameSnapshot,
	) -> PlayerResponse {
		let (tx, rx) = oneshot::channel();

		{
			let mut pending = self.pending_action.lock().unwrap();
			*pending = Some(tx);
		}

		self.send_message(&ServerMessage::ActionRequest {
			valid_actions,
			time_limit: Some(60),
		});

		// Simple blocking wait with timeout using spawn_blocking
		let result = tokio::task::spawn_blocking(move || {
			match rx.blocking_recv() {
				Ok(response) => Some(response),
				Err(_) => None,
			}
		}).await;

		match result {
			Ok(Some(response)) => response,
			_ => {
				let mut pending = self.pending_action.lock().unwrap();
				*pending = None;
				PlayerResponse::Timeout
			}
		}
	}

	fn notify(&self, event: &GameEvent) {
		if let Some(filtered) = self.filter_event(event) {
			self.send_message(&ServerMessage::GameEvent(filtered));
		}
	}

	fn seat(&self) -> Seat {
		self.seat
	}

	fn name(&self) -> &str {
		&self.name
	}

	fn is_human(&self) -> bool {
		true
	}
}

pub fn submit_action(
	pending: &Arc<Mutex<Option<oneshot::Sender<PlayerResponse>>>>,
	action: PlayerAction,
) -> Result<(), String> {
	let mut guard = pending.lock().unwrap();
	if let Some(tx) = guard.take() {
		tx.send(PlayerResponse::Action(action))
			.map_err(|_| "Failed to send action".to_string())
	} else {
		Err("No pending action request".to_string())
	}
}
