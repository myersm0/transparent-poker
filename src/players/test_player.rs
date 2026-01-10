use std::sync::Mutex;
use std::collections::VecDeque;

use async_trait::async_trait;
use crate::events::{GameEvent, PlayerAction, Seat, ValidActions};
use crate::players::port::{PlayerPort, PlayerResponse, GameSnapshot};

pub struct TestPlayer {
	seat: Seat,
	name: String,
	scripted_actions: Mutex<VecDeque<PlayerAction>>,
	default_action: PlayerAction,
	received_events: Mutex<Vec<GameEvent>>,
}

impl TestPlayer {
	pub fn new(seat: Seat, name: impl Into<String>) -> Self {
		Self {
			seat,
			name: name.into(),
			scripted_actions: Mutex::new(VecDeque::new()),
			default_action: PlayerAction::Fold,
			received_events: Mutex::new(Vec::new()),
		}
	}

	pub fn with_actions(mut self, actions: Vec<PlayerAction>) -> Self {
		self.scripted_actions = Mutex::new(actions.into());
		self
	}

	pub fn with_default(mut self, action: PlayerAction) -> Self {
		self.default_action = action;
		self
	}

	pub fn events(&self) -> Vec<GameEvent> {
		self.received_events.lock().unwrap().clone()
	}

	pub fn clear_events(&self) {
		self.received_events.lock().unwrap().clear();
	}
}

#[async_trait]
impl PlayerPort for TestPlayer {
	async fn request_action(
		&self,
		_seat: Seat,
		valid_actions: ValidActions,
		_game_state: &GameSnapshot,
	) -> PlayerResponse {
		let action = self
			.scripted_actions
			.lock()
			.unwrap()
			.pop_front()
			.unwrap_or_else(|| self.default_action.clone());

		let validated = validate_action(action, &valid_actions);
		PlayerResponse::Action(validated)
	}

	fn notify(&self, event: &GameEvent) {
		self.received_events.lock().unwrap().push(event.clone());
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

fn validate_action(action: PlayerAction, valid: &ValidActions) -> PlayerAction {
	match action {
		PlayerAction::Fold if valid.can_fold => action,
		PlayerAction::Check if valid.can_check => action,
		PlayerAction::Call { .. } if valid.call_amount.is_some() => {
			PlayerAction::Call {
				amount: valid.call_amount.unwrap(),
			}
		}
		PlayerAction::Bet { amount } | PlayerAction::Raise { amount } => {
			if let Some(ref raise) = valid.raise_options {
				match raise {
					crate::events::RaiseOptions::Fixed { amount: fixed } => {
						PlayerAction::Raise { amount: *fixed }
					}
					crate::events::RaiseOptions::Variable { min_raise, max_raise } => {
						let clamped = amount.max(*min_raise).min(*max_raise);
						PlayerAction::Raise { amount: clamped }
					}
				}
			} else if valid.can_check {
				PlayerAction::Check
			} else if let Some(call) = valid.call_amount {
				PlayerAction::Call { amount: call }
			} else {
				PlayerAction::Fold
			}
		}
		PlayerAction::AllIn { .. } if valid.can_all_in => {
			PlayerAction::AllIn {
				amount: valid.all_in_amount,
			}
		}
		_ => {
			if valid.can_check {
				PlayerAction::Check
			} else if let Some(call) = valid.call_amount {
				PlayerAction::Call { amount: call }
			} else {
				PlayerAction::Fold
			}
		}
	}
}

pub struct CallingPlayer {
	seat: Seat,
	name: String,
}

impl CallingPlayer {
	pub fn new(seat: Seat, name: impl Into<String>) -> Self {
		Self {
			seat,
			name: name.into(),
		}
	}
}

#[async_trait]
impl PlayerPort for CallingPlayer {
	async fn request_action(
		&self,
		_seat: Seat,
		valid_actions: ValidActions,
		_game_state: &GameSnapshot,
	) -> PlayerResponse {
		let action = if valid_actions.can_check {
			PlayerAction::Check
		} else if let Some(amount) = valid_actions.call_amount {
			PlayerAction::Call { amount }
		} else {
			PlayerAction::Fold
		};

		PlayerResponse::Action(action)
	}

	fn notify(&self, _event: &GameEvent) {}

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

pub struct FoldingPlayer {
	seat: Seat,
	name: String,
}

impl FoldingPlayer {
	pub fn new(seat: Seat, name: impl Into<String>) -> Self {
		Self {
			seat,
			name: name.into(),
		}
	}
}

#[async_trait]
impl PlayerPort for FoldingPlayer {
	async fn request_action(
		&self,
		_seat: Seat,
		valid_actions: ValidActions,
		_game_state: &GameSnapshot,
	) -> PlayerResponse {
		let action = if valid_actions.can_check {
			PlayerAction::Check
		} else {
			PlayerAction::Fold
		};

		PlayerResponse::Action(action)
	}

	fn notify(&self, _event: &GameEvent) {}

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
