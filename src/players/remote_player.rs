use std::sync::mpsc;
use crate::events::{GameEvent, PlayerAction, Seat, ValidActions};
use crate::players::{GameSnapshot, PlayerPort, PlayerResponse};

pub struct RemotePlayerConfig {
	pub server_url: String,
	pub auth_token: String,
	pub opponent_id: String,
}

pub struct RemotePlayer {
	seat: Seat,
	name: String,
	#[allow(dead_code)]
	config: RemotePlayerConfig,
}

impl RemotePlayer {
	pub fn new(seat: Seat, name: &str, config: RemotePlayerConfig) -> Self {
		Self {
			seat,
			name: name.to_string(),
			config,
		}
	}
}

impl PlayerPort for RemotePlayer {
	fn request_action(
		&self,
		_seat: Seat,
		valid_actions: ValidActions,
		_game_state: &GameSnapshot,
	) -> mpsc::Receiver<PlayerResponse> {
		let (tx, rx) = mpsc::channel();

		// TODO: implement server call
		// POST {server_url}/v1/action
		// Authorization: Bearer {auth_token}
		// Body: { opponent_id, game_state, valid_actions }
		// Response: { action, chat?, delay_ms? }

		let action = if valid_actions.can_check {
			PlayerAction::Check
		} else {
			PlayerAction::Fold
		};

		let _ = tx.send(PlayerResponse::Action(action));
		rx
	}

	fn notify(&self, _event: &GameEvent) {
		// TODO: optionally send events to server for opponent state tracking
	}

	fn seat(&self) -> Seat {
		self.seat
	}

	fn name(&self) -> &str {
		&self.name
	}

	fn is_human(&self) -> bool {
		false
	}
}
