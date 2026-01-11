use std::sync::{mpsc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use crate::events::{GameEvent, PlayerAction, Seat, ValidActions};
use crate::players::{GameSnapshot, PlayerPort, PlayerResponse};

pub struct RemotePlayer {
	seat: Seat,
	name: String,
	action_rx: Mutex<mpsc::Receiver<PlayerAction>>,
}

impl RemotePlayer {
	pub fn new(seat: Seat, name: String, action_rx: mpsc::Receiver<PlayerAction>) -> Self {
		Self {
			seat,
			name,
			action_rx: Mutex::new(action_rx),
		}
	}
}

#[async_trait]
impl PlayerPort for RemotePlayer {
	async fn request_action(
		&self,
		_seat: Seat,
		_valid_actions: ValidActions,
		_game_state: &GameSnapshot,
	) -> PlayerResponse {
		let rx = self.action_rx.lock().unwrap();
		match rx.recv_timeout(Duration::from_secs(120)) {
			Ok(action) => PlayerResponse::Action(action),
			Err(_) => PlayerResponse::Timeout,
		}
	}

	fn notify(&self, _event: &GameEvent) {
		// Events forwarded via event_rx stream, not per-player notify
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
