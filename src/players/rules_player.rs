use async_trait::async_trait;
use crate::ai::{try_rules, RuleDecision, Situation};
use crate::events::{Card, GameEvent, PlayerAction, RaiseOptions, Seat, Street, ValidActions};
use crate::logging::ai as log;
use crate::players::{GameSnapshot, PlayerPort, PlayerResponse};
use crate::strategy::{char_to_rank, HandGroup, HoleCards, Position, Strategy};

pub struct RulesPlayer {
	seat: Seat,
	name: String,
	strategy: Strategy,
	big_blind: f32,
	button: std::sync::RwLock<usize>,
	num_players: std::sync::RwLock<usize>,
}

impl RulesPlayer {
	pub fn new(seat: Seat, name: &str, strategy: Strategy, big_blind: f32) -> Self {
		Self {
			seat,
			name: name.to_string(),
			strategy,
			big_blind,
			button: std::sync::RwLock::new(0),
			num_players: std::sync::RwLock::new(2),
		}
	}

	fn classify_cards(&self, cards: &[Card; 2]) -> Option<HandGroup> {
		let rank1 = char_to_rank(cards[0].rank)?;
		let rank2 = char_to_rank(cards[1].rank)?;
		let suited = cards[0].suit == cards[1].suit;
		Some(HoleCards::new(rank1, rank2, suited).classify())
	}

	fn get_position(&self) -> Position {
		let button = *self.button.read().unwrap_or_else(|e| e.into_inner());
		let num_players = *self.num_players.read().unwrap_or_else(|e| e.into_inner());
		Position::from_seat(self.seat.0, button, num_players)
	}

	fn build_situation(&self, cards: &[Card; 2], snapshot: &GameSnapshot, valid: &ValidActions) -> Option<Situation> {
		let hand_group = self.classify_cards(cards)?;
		let position = self.get_position();

		let to_call = valid.call_amount.unwrap_or(0.0);
		let current_bet = snapshot.seats.iter()
			.map(|s| s.current_bet)
			.fold(0.0_f32, |a, b| a.max(b));

		let num_raises = snapshot.action_history.iter()
			.filter(|a| a.street == snapshot.street)
			.filter(|a| matches!(a.action, PlayerAction::Raise { .. } | PlayerAction::Bet { .. }))
			.count() as u32;

		let we_are_preflop_aggressor = snapshot.action_history.iter()
			.filter(|a| a.street == Street::Preflop)
			.filter(|a| a.seat == self.seat)
			.any(|a| matches!(a.action, PlayerAction::Raise { .. } | PlayerAction::Bet { .. }));

		Some(Situation {
			hand_group,
			position,
			pot: snapshot.pot,
			to_call,
			stack: snapshot.seats.iter()
				.find(|s| s.seat == self.seat)
				.map(|s| s.stack)
				.unwrap_or(0.0),
			big_blind: self.big_blind,
			current_bet,
			is_preflop: snapshot.street == Street::Preflop,
			num_raises,
			raise_cap: 4,
			we_are_preflop_aggressor,
		})
	}

	fn rule_to_action(&self, decision: RuleDecision, valid: &ValidActions, stack: f32) -> PlayerAction {
		match decision {
			RuleDecision::Fold => PlayerAction::Fold,
			RuleDecision::Check => {
				if valid.can_check {
					PlayerAction::Check
				} else if let Some(amount) = valid.call_amount {
					PlayerAction::Call { amount }
				} else {
					PlayerAction::Fold
				}
			}
			RuleDecision::Call => {
				if let Some(amount) = valid.call_amount {
					PlayerAction::Call { amount }
				} else if valid.can_check {
					PlayerAction::Check
				} else {
					PlayerAction::Fold
				}
			}
			RuleDecision::Raise(amount) => {
				if let Some(ref opts) = valid.raise_options {
					let (min, max) = match opts {
						RaiseOptions::Variable { min_raise, max_raise } => (*min_raise, *max_raise),
						RaiseOptions::Fixed { amount } => (*amount, *amount),
					};
					let amount = amount.max(min).min(max).min(stack);
					PlayerAction::Raise { amount }
				} else if let Some(call_amt) = valid.call_amount {
					PlayerAction::Call { amount: call_amt }
				} else if valid.can_check {
					PlayerAction::Check
				} else {
					PlayerAction::Fold
				}
			}
		}
	}

	fn decide(&self, valid: &ValidActions, snapshot: &GameSnapshot) -> PlayerAction {
		let cards = match snapshot.hero_cards {
			Some(ref c) => c,
			None => {
				log::error(&self.name, "No hole cards in snapshot");
				return if valid.can_check { PlayerAction::Check } else { PlayerAction::Fold };
			}
		};

		let stack = snapshot.seats.iter()
			.find(|s| s.seat == self.seat)
			.map(|s| s.stack)
			.unwrap_or(0.0);

		if let Some(situation) = self.build_situation(cards, snapshot, valid) {
			log::strategy(&self.name, &format!(
				"{} in {} | street={:?}",
				situation.hand_group, situation.position, snapshot.street
			));

			if let Some(decision) = try_rules(&self.strategy, &situation) {
				let action = self.rule_to_action(decision, valid, stack);
				log::decision(&self.name, "RULE", &action.description());
				return action;
			}
		}

		let action = if valid.can_check {
			PlayerAction::Check
		} else if let Some(amount) = valid.call_amount {
			if amount <= self.big_blind * 2.0 {
				PlayerAction::Call { amount }
			} else {
				PlayerAction::Fold
			}
		} else {
			PlayerAction::Fold
		};

		log::decision(&self.name, "DEFAULT", &action.description());
		action
	}
}

#[async_trait]
impl PlayerPort for RulesPlayer {
	async fn request_action(
		&self,
		_seat: Seat,
		valid_actions: ValidActions,
		game_state: &GameSnapshot,
	) -> PlayerResponse {
		let action = self.decide(&valid_actions, game_state);
		PlayerResponse::Action(action)
	}

	fn notify(&self, event: &GameEvent) {
		if let GameEvent::HandStarted { button, seats, .. } = event {
			*self.button.write().unwrap_or_else(|e| e.into_inner()) = button.0;
			*self.num_players.write().unwrap_or_else(|e| e.into_inner()) = seats.len();
		}
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::strategy::Strategy;
	use crate::events::Position as EventPosition;

	fn make_test_player() -> RulesPlayer {
		RulesPlayer::new(Seat(0), "TestAI", Strategy::default(), 2.0)
	}

	#[test]
	fn test_player_creation() {
		let player = make_test_player();
		assert_eq!(player.seat(), Seat(0));
		assert_eq!(player.name(), "TestAI");
		assert!(!player.is_human());
	}

	#[test]
	fn test_position_calculation() {
		let player = RulesPlayer::new(Seat(0), "Test", Strategy::default(), 2.0);
		
		player.notify(&GameEvent::HandStarted {
			hand_id: crate::events::HandId(1),
			hand_num: 1,
			button: Seat(1),
			blinds: crate::events::Blinds { small: 1.0, big: 2.0, ante: None },
			seats: vec![
				crate::events::SeatInfo {
					seat: Seat(0),
					name: "A".to_string(),
					stack: 100.0,
					position: EventPosition::SmallBlind,
					is_active: true,
					is_human: false,
				},
				crate::events::SeatInfo {
					seat: Seat(1),
					name: "B".to_string(),
					stack: 100.0,
					position: EventPosition::Button,
					is_active: true,
					is_human: false,
				},
			],
		});
		
		let position = player.get_position();
		// Seat 0 with button at seat 1 in heads-up = big blind
		assert_eq!(position, Position::Bb);
	}

	#[test]
	fn test_card_classification() {
		let player = make_test_player();
		
		let aces = [
			Card { rank: 'A', suit: 'h' },
			Card { rank: 'A', suit: 's' },
		];
		let group = player.classify_cards(&aces);
		assert_eq!(group, Some(HandGroup::Premium));
		
		let trash = [
			Card { rank: '7', suit: 'h' },
			Card { rank: '2', suit: 's' },
		];
		let group = player.classify_cards(&trash);
		assert_eq!(group, Some(HandGroup::Trash));
	}
}
