use std::sync::{Arc, Mutex, mpsc::Sender};
use rs_poker::arena::{Agent, GameState, action::AgentAction};
use rs_poker::arena::game_state::Round;

use crate::events::{
	Card, GameEvent, PlayerAction, Position, RaiseOptions, Seat, Street, ValidActions,
};
use crate::players::{ActionRecord, GameSnapshot, PlayerPort, PlayerResponse, SeatSnapshot};

pub struct PlayerAdapter {
	port: Arc<dyn PlayerPort>,
	seat: Seat,
	hole_cards: Option<[Card; 2]>,
	action_history: Arc<Mutex<Vec<ActionRecord>>>,
	betting_structure: BettingStructure,
	event_tx: Sender<GameEvent>,
	max_raises_per_round: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum BettingStructure {
	NoLimit,
	PotLimit,
	FixedLimit,
}

impl PlayerAdapter {
	pub fn new(
		port: Arc<dyn PlayerPort>,
		seat: Seat,
		betting_structure: BettingStructure,
		action_history: Arc<Mutex<Vec<ActionRecord>>>,
		event_tx: Sender<GameEvent>,
		max_raises_per_round: u32,
	) -> Self {
		Self {
			port,
			seat,
			hole_cards: None,
			action_history,
			betting_structure,
			event_tx,
			max_raises_per_round,
		}
	}

	fn build_snapshot(&self, game_state: &GameState) -> GameSnapshot {
		let street = convert_round(game_state.round);
		let board = game_state
			.board
			.iter()
			.map(|c| convert_card(c))
			.collect();

		let seats = game_state
			.stacks
			.iter()
			.enumerate()
			.map(|(i, &stack)| {
				let position = self.compute_position(i, game_state);
				let is_active = game_state.player_active.get(i);
				SeatSnapshot {
					seat: Seat(i),
					name: String::new(),
					stack,
					current_bet: 0.0,
					is_folded: !is_active,
					is_all_in: stack <= 0.0 && is_active,
					position,
				}
			})
			.collect();

		GameSnapshot {
			hand_num: 0,
			street,
			board,
			pot: game_state.total_pot,
			seats,
			hero_cards: self.hole_cards,
			action_history: self.action_history.lock().unwrap().clone(),
		}
	}

	fn build_valid_actions(&self, game_state: &GameState) -> ValidActions {
		let stack = game_state.stacks[self.seat.0];
		let current_bet = game_state.current_round_bet();
		let my_bet = game_state.current_round_current_player_bet();
		let to_call = current_bet - my_bet;
		let min_raise = game_state.current_round_min_raise();

		let can_check = to_call <= 0.0;
		let call_amount = if to_call > 0.0 {
			Some(to_call.min(stack))
		} else {
			None
		};

		let current_street = convert_round(game_state.round);
		let raises_this_round = self.count_raises_this_round(current_street);
		let raise_cap_reached = raises_this_round >= self.max_raises_per_round;

		let raise_options = if raise_cap_reached {
			None
		} else {
			self.compute_raise_options(game_state, stack, current_bet, min_raise)
		};

		ValidActions {
			can_fold: to_call > 0.0,
			can_check,
			call_amount,
			raise_options,
			can_all_in: stack > 0.0 && !raise_cap_reached,
			all_in_amount: stack + my_bet,
		}
	}

	fn count_raises_this_round(&self, current_street: Street) -> u32 {
		self.action_history
			.lock()
			.unwrap()
			.iter()
			.filter(|a| a.street == current_street)
			.filter(|a| matches!(a.action, PlayerAction::Raise { .. } | PlayerAction::Bet { .. }))
			.count() as u32
	}

	fn compute_raise_options(
		&self,
		game_state: &GameState,
		stack: f32,
		current_bet: f32,
		min_raise: f32,
	) -> Option<RaiseOptions> {
		let my_bet = game_state.current_round_current_player_bet();
		let can_afford_raise = stack > (current_bet - my_bet);

		if !can_afford_raise {
			return None;
		}

		match self.betting_structure {
			BettingStructure::FixedLimit => {
				let bet_size = match game_state.round {
					Round::Preflop | Round::Flop => game_state.big_blind,
					_ => game_state.big_blind * 2.0,
				};
				let raise_to = current_bet + bet_size;
				if stack + my_bet >= raise_to {
					Some(RaiseOptions::Fixed { amount: raise_to })
				} else {
					None
				}
			}
			BettingStructure::PotLimit => {
				let pot = game_state.total_pot;
				let to_call = current_bet - my_bet;
				let max_raise = pot + to_call * 2.0;
				let min_raise_to = current_bet + min_raise;
				let max_raise_to = (current_bet + max_raise).min(stack + my_bet);

				if max_raise_to >= min_raise_to {
					Some(RaiseOptions::Variable {
						min_raise: min_raise_to,
						max_raise: max_raise_to,
					})
				} else {
					None
				}
			}
			BettingStructure::NoLimit => {
				let min_raise_to = current_bet + min_raise;
				let max_raise_to = stack + my_bet;

				if max_raise_to >= min_raise_to {
					Some(RaiseOptions::Variable {
						min_raise: min_raise_to,
						max_raise: max_raise_to,
					})
				} else {
					None
				}
			}
		}
	}

	fn compute_position(&self, seat_idx: usize, game_state: &GameState) -> Position {
		let btn = game_state.dealer_idx;
		let n = game_state.stacks.len();

		if seat_idx == btn {
			Position::Button
		} else if seat_idx == (btn + 1) % n {
			Position::SmallBlind
		} else if seat_idx == (btn + 2) % n {
			Position::BigBlind
		} else {
			Position::None
		}
	}

	fn convert_response(&self, response: PlayerResponse, valid: &ValidActions) -> AgentAction {
		match response {
			PlayerResponse::Action(action) => self.convert_action(action, valid),
			PlayerResponse::Admin(_) => {
				if valid.can_check {
					AgentAction::Call
				} else {
					AgentAction::Fold
				}
			}
			PlayerResponse::Timeout => {
				if valid.can_check {
					AgentAction::Call
				} else {
					AgentAction::Fold
				}
			}
		}
	}

	fn convert_action(&self, action: PlayerAction, valid: &ValidActions) -> AgentAction {
		match action {
			PlayerAction::Fold => AgentAction::Fold,
			PlayerAction::Check => AgentAction::Call,
			PlayerAction::Call { .. } => AgentAction::Call,
			PlayerAction::Bet { amount } | PlayerAction::Raise { amount } => {
				AgentAction::Bet(amount)
			}
			PlayerAction::AllIn { .. } => AgentAction::AllIn,
			PlayerAction::Timeout => {
				if valid.can_check {
					AgentAction::Call
				} else {
					AgentAction::Fold
				}
			}
		}
	}
}

impl Agent for PlayerAdapter {
	fn act(&mut self, _id: u128, game_state: &GameState) -> AgentAction {
		if self.hole_cards.is_none() {
			if let Some(hand) = game_state.hands.get(self.seat.0) {
				let cards: Vec<rs_poker::core::Card> = hand.iter().take(2).collect();
				if cards.len() >= 2 {
					self.hole_cards = Some([convert_card(&cards[0]), convert_card(&cards[1])]);
				}
			}
		}

		let snapshot = self.build_snapshot(game_state);
		let valid_actions = self.build_valid_actions(game_state);

		let _ = self.event_tx.send(GameEvent::ActionRequest {
			seat: self.seat,
			valid_actions: valid_actions.clone(),
			time_limit: None,
		});

		let rx = self.port.request_action(self.seat, valid_actions.clone(), &snapshot);

		match rx.recv() {
			Ok(response) => self.convert_response(response, &valid_actions),
			Err(_) => {
				if valid_actions.can_check {
					AgentAction::Call
				} else {
					AgentAction::Fold
				}
			}
		}
	}
}

fn convert_round(round: Round) -> Street {
	match round {
		Round::Preflop => Street::Preflop,
		Round::Flop => Street::Flop,
		Round::Turn => Street::Turn,
		Round::River => Street::River,
		Round::Showdown => Street::Showdown,
		_ => Street::Preflop,
	}
}

pub fn convert_card(card: &rs_poker::core::Card) -> Card {
	let rank = match card.value {
		rs_poker::core::Value::Two => '2',
		rs_poker::core::Value::Three => '3',
		rs_poker::core::Value::Four => '4',
		rs_poker::core::Value::Five => '5',
		rs_poker::core::Value::Six => '6',
		rs_poker::core::Value::Seven => '7',
		rs_poker::core::Value::Eight => '8',
		rs_poker::core::Value::Nine => '9',
		rs_poker::core::Value::Ten => 'T',
		rs_poker::core::Value::Jack => 'J',
		rs_poker::core::Value::Queen => 'Q',
		rs_poker::core::Value::King => 'K',
		rs_poker::core::Value::Ace => 'A',
	};
	let suit = match card.suit {
		rs_poker::core::Suit::Spade => 's',
		rs_poker::core::Suit::Heart => 'h',
		rs_poker::core::Suit::Diamond => 'd',
		rs_poker::core::Suit::Club => 'c',
	};
	Card::new(rank, suit)
}
