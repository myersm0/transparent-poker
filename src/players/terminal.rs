use std::sync::mpsc::{self, Receiver, Sender};

use crate::events::{GameEvent, Seat, ValidActions};
use crate::players::{GameSnapshot, PlayerPort, PlayerResponse};

pub struct TerminalPlayer {
	seat: Seat,
	name: String,
	action_tx: Sender<ActionRequest>,
}

pub struct ActionRequest {
	pub seat: Seat,
	pub valid_actions: ValidActions,
	pub snapshot: GameSnapshot,
	pub response_tx: Sender<PlayerResponse>,
}

pub struct TerminalPlayerHandle {
	pub action_rx: Receiver<ActionRequest>,
}

impl TerminalPlayer {
	pub fn new(seat: Seat, name: impl Into<String>) -> (Self, TerminalPlayerHandle) {
		let (action_tx, action_rx) = mpsc::channel();

		let player = Self {
			seat,
			name: name.into(),
			action_tx,
		};

		let handle = TerminalPlayerHandle { action_rx };

		(player, handle)
	}
}

impl PlayerPort for TerminalPlayer {
	fn request_action(
		&self,
		seat: Seat,
		valid_actions: ValidActions,
		game_state: &GameSnapshot,
	) -> Receiver<PlayerResponse> {
		let (response_tx, response_rx) = mpsc::channel();

		let request = ActionRequest {
			seat,
			valid_actions,
			snapshot: game_state.clone(),
			response_tx,
		};

		let _ = self.action_tx.send(request);

		response_rx
	}

	fn notify(&self, _event: &GameEvent) {}

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
