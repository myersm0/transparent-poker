use crossterm::event::KeyCode;
use crate::events::{PlayerAction, RaiseOptions, ValidActions};
use crate::players::PlayerResponse;

#[derive(Debug, Clone)]
pub enum InputState {
	Watching,
	AwaitingAction { valid: ValidActions },
	EnteringRaise {
		valid: ValidActions,
		amount: f32,
		min: f32,
		max: f32,
	},
	GameOver,
}

#[derive(Debug)]
pub enum InputEffect {
	None,
	SetPrompt(String),
	ClearPrompt,
	Respond(PlayerResponse),
	Quit,
}

impl Default for InputState {
	fn default() -> Self {
		Self::Watching
	}
}

impl InputState {
	pub fn is_awaiting_input(&self) -> bool {
		matches!(self, Self::AwaitingAction { .. } | Self::EnteringRaise { .. })
	}

	pub fn is_game_over(&self) -> bool {
		matches!(self, Self::GameOver)
	}

	pub fn enter_action_mode(valid: ValidActions) -> (Self, InputEffect) {
		let prompt = build_action_prompt(&valid);
		let state = Self::AwaitingAction { valid };
		(state, InputEffect::SetPrompt(prompt))
	}

	pub fn enter_game_over() -> (Self, InputEffect) {
		(
			Self::GameOver,
			InputEffect::SetPrompt("Game Over! Press 'q' to quit.".into()),
		)
	}

	pub fn handle_key(self, key: KeyCode) -> (Self, InputEffect) {
		match self {
			Self::Watching => handle_watching(key),
			Self::AwaitingAction { valid } => handle_awaiting_action(valid, key),
			Self::EnteringRaise { valid, amount, min, max } => {
				handle_entering_raise(valid, amount, min, max, key)
			}
			Self::GameOver => handle_game_over(key),
		}
	}
}

fn handle_watching(key: KeyCode) -> (InputState, InputEffect) {
	match key {
		KeyCode::Char('q') | KeyCode::Esc => (InputState::Watching, InputEffect::Quit),
		_ => (InputState::Watching, InputEffect::None),
	}
}

fn handle_game_over(key: KeyCode) -> (InputState, InputEffect) {
	match key {
		KeyCode::Char('q') | KeyCode::Esc => (InputState::GameOver, InputEffect::Quit),
		_ => (InputState::GameOver, InputEffect::None),
	}
}

fn handle_awaiting_action(valid: ValidActions, key: KeyCode) -> (InputState, InputEffect) {
	match key {
		KeyCode::Char('f') => {
			if valid.can_fold {
				(
					InputState::Watching,
					InputEffect::Respond(PlayerResponse::Action(PlayerAction::Fold)),
				)
			} else {
				let prompt = format!("Can't fold. {}", build_action_prompt(&valid));
				(InputState::AwaitingAction { valid }, InputEffect::SetPrompt(prompt))
			}
		}

		KeyCode::Enter => {
			if valid.can_check {
				(
					InputState::Watching,
					InputEffect::Respond(PlayerResponse::Action(PlayerAction::Check)),
				)
			} else {
				(InputState::AwaitingAction { valid }, InputEffect::None)
			}
		}

		KeyCode::Char('c') => {
			if let Some(amount) = valid.call_amount {
				(
					InputState::Watching,
					InputEffect::Respond(PlayerResponse::Action(PlayerAction::Call { amount })),
				)
			} else if valid.can_check {
				(
					InputState::Watching,
					InputEffect::Respond(PlayerResponse::Action(PlayerAction::Check)),
				)
			} else {
				(InputState::AwaitingAction { valid }, InputEffect::None)
			}
		}

		KeyCode::Char('b') => {
			if valid.can_check {
				if let Some(ref raise_opts) = valid.raise_options {
					let bet_amount = match raise_opts {
						RaiseOptions::Fixed { amount } => *amount,
						RaiseOptions::Variable { min_raise, max_raise } => {
							(min_raise * 1.5).min(*max_raise)
						}
					};
					(
						InputState::Watching,
						InputEffect::Respond(PlayerResponse::Action(PlayerAction::Bet {
							amount: bet_amount,
						})),
					)
				} else {
					(InputState::AwaitingAction { valid }, InputEffect::None)
				}
			} else {
				(InputState::AwaitingAction { valid }, InputEffect::None)
			}
		}

		KeyCode::Char('r') => {
			if let Some(ref raise_opts) = valid.raise_options {
				let (min, max) = match raise_opts {
					RaiseOptions::Fixed { amount } => (*amount, *amount),
					RaiseOptions::Variable { min_raise, max_raise } => (*min_raise, *max_raise),
				};
				let prompt = format!("Raise: ${:.0} [←/→ adjust] [Enter confirm] [Esc cancel]", min);
				(
					InputState::EnteringRaise { valid, amount: min, min, max },
					InputEffect::SetPrompt(prompt),
				)
			} else {
				(InputState::AwaitingAction { valid }, InputEffect::None)
			}
		}

		KeyCode::Char('a') => {
			if valid.can_all_in {
				(
					InputState::Watching,
					InputEffect::Respond(PlayerResponse::Action(PlayerAction::AllIn {
						amount: valid.all_in_amount,
					})),
				)
			} else {
				(InputState::AwaitingAction { valid }, InputEffect::None)
			}
		}

		KeyCode::Char('q') | KeyCode::Esc => {
			(InputState::AwaitingAction { valid }, InputEffect::Quit)
		}

		_ => (InputState::AwaitingAction { valid }, InputEffect::None),
	}
}

fn handle_entering_raise(
	valid: ValidActions,
	amount: f32,
	min: f32,
	max: f32,
	key: KeyCode,
) -> (InputState, InputEffect) {
	match key {
		KeyCode::Left => {
			let step = ((max - min) / 10.0).max(1.0);
			let new_amount = (amount - step).max(min);
			let prompt = format!(
				"Raise: ${:.0} [←/→ adjust] [Enter confirm] [Esc cancel]",
				new_amount
			);
			(
				InputState::EnteringRaise { valid, amount: new_amount, min, max },
				InputEffect::SetPrompt(prompt),
			)
		}

		KeyCode::Right => {
			let step = ((max - min) / 10.0).max(1.0);
			let new_amount = (amount + step).min(max);
			let prompt = format!(
				"Raise: ${:.0} [←/→ adjust] [Enter confirm] [Esc cancel]",
				new_amount
			);
			(
				InputState::EnteringRaise { valid, amount: new_amount, min, max },
				InputEffect::SetPrompt(prompt),
			)
		}

		KeyCode::Enter => (
			InputState::Watching,
			InputEffect::Respond(PlayerResponse::Action(PlayerAction::Raise { amount })),
		),

		KeyCode::Esc => {
			let prompt = build_action_prompt(&valid);
			(InputState::AwaitingAction { valid }, InputEffect::SetPrompt(prompt))
		}

		KeyCode::Char('q') => {
			(InputState::EnteringRaise { valid, amount, min, max }, InputEffect::Quit)
		}

		_ => (InputState::EnteringRaise { valid, amount, min, max }, InputEffect::None),
	}
}

fn build_action_prompt(valid: &ValidActions) -> String {
	let mut parts = Vec::new();

	if valid.can_check {
		parts.push("[Enter] check".to_string());
		if valid.raise_options.is_some() {
			parts.push("[b]et".to_string());
		}
	} else if let Some(amt) = valid.call_amount {
		parts.push(format!("[c]all ${:.0}", amt));
		if valid.can_fold {
			parts.push("[f]old".to_string());
		}
	}

	if valid.raise_options.is_some() && !valid.can_check {
		parts.push("[r]aise".to_string());
	}

	if valid.can_all_in {
		parts.push("[a]ll-in".to_string());
	}

	parts.join("  ")
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_valid_actions(can_check: bool, call_amount: Option<f32>) -> ValidActions {
		ValidActions {
			can_fold: call_amount.is_some(),
			can_check,
			call_amount,
			raise_options: Some(RaiseOptions::Variable {
				min_raise: 20.0,
				max_raise: 100.0,
			}),
			can_all_in: true,
			all_in_amount: 100.0,
		}
	}

	#[test]
	fn watching_q_quits() {
		let state = InputState::Watching;
		let (new_state, effect) = state.handle_key(KeyCode::Char('q'));

		assert!(matches!(new_state, InputState::Watching));
		assert!(matches!(effect, InputEffect::Quit));
	}

	#[test]
	fn watching_other_key_does_nothing() {
		let state = InputState::Watching;
		let (new_state, effect) = state.handle_key(KeyCode::Char('x'));

		assert!(matches!(new_state, InputState::Watching));
		assert!(matches!(effect, InputEffect::None));
	}

	#[test]
	fn awaiting_action_fold_when_allowed() {
		let valid = make_valid_actions(false, Some(10.0));
		let state = InputState::AwaitingAction { valid };
		let (new_state, effect) = state.handle_key(KeyCode::Char('f'));

		assert!(matches!(new_state, InputState::Watching));
		assert!(matches!(
			effect,
			InputEffect::Respond(PlayerResponse::Action(PlayerAction::Fold))
		));
	}

	#[test]
	fn awaiting_action_fold_when_not_allowed() {
		let valid = make_valid_actions(true, None);
		let state = InputState::AwaitingAction { valid };
		let (new_state, effect) = state.handle_key(KeyCode::Char('f'));

		assert!(matches!(new_state, InputState::AwaitingAction { .. }));
		assert!(matches!(effect, InputEffect::SetPrompt(_)));
	}

	#[test]
	fn awaiting_action_check_when_allowed() {
		let valid = make_valid_actions(true, None);
		let state = InputState::AwaitingAction { valid };
		let (new_state, effect) = state.handle_key(KeyCode::Enter);

		assert!(matches!(new_state, InputState::Watching));
		assert!(matches!(
			effect,
			InputEffect::Respond(PlayerResponse::Action(PlayerAction::Check))
		));
	}

	#[test]
	fn awaiting_action_r_enters_raise_mode() {
		let valid = make_valid_actions(false, Some(10.0));
		let state = InputState::AwaitingAction { valid };
		let (new_state, effect) = state.handle_key(KeyCode::Char('r'));

		assert!(matches!(new_state, InputState::EnteringRaise { .. }));
		assert!(matches!(effect, InputEffect::SetPrompt(_)));
	}

	#[test]
	fn entering_raise_left_decreases_amount() {
		let valid = make_valid_actions(false, Some(10.0));
		let state = InputState::EnteringRaise {
			valid,
			amount: 50.0,
			min: 20.0,
			max: 100.0,
		};
		let (new_state, _) = state.handle_key(KeyCode::Left);

		if let InputState::EnteringRaise { amount, .. } = new_state {
			assert!(amount < 50.0);
			assert!(amount >= 20.0);
		} else {
			panic!("Expected EnteringRaise state");
		}
	}

	#[test]
	fn entering_raise_enter_confirms() {
		let valid = make_valid_actions(false, Some(10.0));
		let state = InputState::EnteringRaise {
			valid,
			amount: 50.0,
			min: 20.0,
			max: 100.0,
		};
		let (new_state, effect) = state.handle_key(KeyCode::Enter);

		assert!(matches!(new_state, InputState::Watching));
		if let InputEffect::Respond(PlayerResponse::Action(PlayerAction::Raise { amount })) = effect
		{
			assert_eq!(amount, 50.0);
		} else {
			panic!("Expected Raise response");
		}
	}

	#[test]
	fn entering_raise_esc_cancels() {
		let valid = make_valid_actions(false, Some(10.0));
		let state = InputState::EnteringRaise {
			valid,
			amount: 50.0,
			min: 20.0,
			max: 100.0,
		};
		let (new_state, effect) = state.handle_key(KeyCode::Esc);

		assert!(matches!(new_state, InputState::AwaitingAction { .. }));
		assert!(matches!(effect, InputEffect::SetPrompt(_)));
	}

	#[test]
	fn game_over_q_quits() {
		let state = InputState::GameOver;
		let (_, effect) = state.handle_key(KeyCode::Char('q'));

		assert!(matches!(effect, InputEffect::Quit));
	}
}
