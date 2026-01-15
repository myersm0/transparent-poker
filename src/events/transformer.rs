use crate::events::types::*;
use crate::view::{
	ActionPrompt, Card as ViewCard, ChatMessage, PlayerStatus, PlayerView,
	Position as ViewPosition, Street as ViewStreet, TableView,
};

pub struct ViewUpdater {
	hero_seat: Option<Seat>,
}

impl ViewUpdater {
	pub fn new(hero_seat: Option<Seat>) -> Self {
		Self { hero_seat }
	}

	pub fn apply(&self, view: &mut TableView, event: &GameEvent) {
		match event {
			GameEvent::HandStarted {
				hand_num,
				button,
				blinds,
				seats,
				..
			} => {
				view.hand_num = *hand_num;
				view.street = ViewStreet::Preflop;
				view.board.clear();
				view.pot = 0.0;
				view.blinds = (blinds.small, blinds.big);

				view.players = seats
					.iter()
					.map(|s| PlayerView {
						seat: s.seat.0,
						name: if s.is_occupied { s.name.clone() } else { "Empty".to_string() },
						stack: s.stack,
						current_bet: 0.0,
						status: if !s.is_occupied {
							PlayerStatus::Empty
						} else if s.is_active {
							PlayerStatus::Active
						} else {
							PlayerStatus::Eliminated
						},
						position: self.convert_position(&s.position),
						hole_cards: None,
						is_hero: self.hero_seat.map(|h| h == s.seat).unwrap_or(false),
						is_actor: false,
						last_action: None,
						action_fresh: false,
					})
					.collect();

				self.update_button(view, *button);

				let active_count = seats.iter().filter(|s| s.is_active).count();
				view.chat_messages.push(ChatMessage {
					sender: String::new(),
					text: format!("Preflop ({} players)", active_count),
					is_system: true,
				});
			}

			GameEvent::HoleCardsDealt { seat, cards } => {
				if let Some(player) = view.players.iter_mut().find(|p| p.seat == seat.0) {
					player.hole_cards = Some([
						ViewCard::new(cards[0].rank, cards[0].suit),
						ViewCard::new(cards[1].rank, cards[1].suit),
					]);
				}
			}

			GameEvent::BlindPosted { seat, amount, .. } => {
				if let Some(player) = view.players.iter_mut().find(|p| p.seat == seat.0) {
					player.current_bet = *amount;
					player.stack -= amount;
				}
				view.pot += amount;
			}

			GameEvent::StreetChanged { street, board } => {
				view.street = self.convert_street(street);
				view.board = board
					.iter()
					.map(|c| ViewCard::new(c.rank, c.suit))
					.collect();

				for player in &mut view.players {
					player.current_bet = 0.0;
					player.last_action = None;
				}

				let active_count = view.players.iter()
					.filter(|p| matches!(p.status, PlayerStatus::Active | PlayerStatus::AllIn))
					.count();

				let msg = match street {
					Street::Flop => {
						let cards: Vec<String> = board.iter()
							.map(|c| ViewCard::new(c.rank, c.suit).display())
							.collect();
						format!("Flop ({} players): {}", active_count, cards.join(" "))
					}
					Street::Turn => {
						if let Some(card) = board.last() {
							let card_str = ViewCard::new(card.rank, card.suit).display();
							format!("Turn ({} players): {}", active_count, card_str)
						} else {
							return;
						}
					}
					Street::River => {
						if let Some(card) = board.last() {
							let card_str = ViewCard::new(card.rank, card.suit).display();
							format!("River ({} players): {}", active_count, card_str)
						} else {
							return;
						}
					}
					_ => return,
				};

				view.chat_messages.push(ChatMessage {
					sender: String::new(),
					text: msg,
					is_system: true,
				});
			}

			GameEvent::ActionRequest { seat, valid_actions, .. } => {
				for player in &mut view.players {
					player.is_actor = player.seat == seat.0;
				}

				if self.hero_seat.map(|h| h.0 == seat.0).unwrap_or(false) {
					view.action_prompt = Some(self.build_action_prompt(valid_actions));
				} else {
					view.action_prompt = None;
				}
			}

			GameEvent::ActionTaken {
				seat,
				action,
				stack_after,
				pot_after,
			} => {
				for player in &mut view.players {
					if player.seat == seat.0 {
						player.stack = *stack_after;
						player.last_action = Some(action.description());
						player.is_actor = false;
						player.action_fresh = true;

						match action {
							PlayerAction::Fold => {
								player.status = PlayerStatus::Folded;
								player.current_bet = 0.0;
							}
							PlayerAction::Check => {}
							PlayerAction::Call { amount } | PlayerAction::Bet { amount } | PlayerAction::Raise { amount } => {
								player.current_bet = *amount;
							}
							PlayerAction::AllIn { amount } => {
								player.current_bet = *amount;
								player.status = PlayerStatus::AllIn;
							}
							PlayerAction::Timeout => {
								player.status = PlayerStatus::Folded;
							}
						}
					} else {
						player.action_fresh = false;
					}
				}
				view.pot = *pot_after;
				view.action_prompt = None;
			}

			GameEvent::PotAwarded {
				seat,
				amount,
				hand_description,
				..
			} => {
				let msg = if let Some(desc) = hand_description {
					format!(
						"{} wins ${:.0} with {}",
						self.player_name(view, *seat),
						amount,
						desc
					)
				} else {
					format!("{} wins ${:.0}", self.player_name(view, *seat), amount)
				};

				view.chat_messages.push(ChatMessage {
					sender: String::new(),
					text: msg,
					is_system: true,
				});

				if let Some(player) = view.players.iter_mut().find(|p| p.seat == seat.0) {
					player.stack += amount;
				}
			}

			GameEvent::HandEnded { results, .. } => {
				view.action_prompt = None;
				for player in &mut view.players {
					player.is_actor = false;
					player.current_bet = 0.0;
				}

				// Reveal cards from showdown (backup if ShowdownReveal wasn't received)
				for result in results {
					if let Some(cards) = &result.showed_cards {
						if let Some(player) = view.players.iter_mut().find(|p| p.seat == result.seat.0) {
							player.hole_cards = Some([
								ViewCard::new(cards[0].rank, cards[0].suit),
								ViewCard::new(cards[1].rank, cards[1].suit),
							]);
						}
					}
				}
			}

			GameEvent::ShowdownReveal { reveals } => {
				for (seat, cards) in reveals {
					if let Some(player) = view.players.iter_mut().find(|p| p.seat == seat.0) {
						player.hole_cards = Some([
							ViewCard::new(cards[0].rank, cards[0].suit),
							ViewCard::new(cards[1].rank, cards[1].suit),
						]);
					}
				}
			}

			GameEvent::ChatMessage { sender, text } => {
				let (sender_str, is_system) = match sender {
					ChatSender::System => (String::new(), true),
					ChatSender::Dealer => ("Dealer".to_string(), true),
					ChatSender::Player(seat) => (self.player_name(view, *seat), false),
					ChatSender::Spectator(name) => (name.clone(), false),
				};

				view.chat_messages.push(ChatMessage {
					sender: sender_str,
					text: text.clone(),
					is_system,
				});
			}

			GameEvent::PlayerLeft { seat, reason } => {
				if let Some(player) = view.players.iter_mut().find(|p| p.seat == seat.0) {
					let name = player.name.clone();
					match reason {
						LeaveReason::Eliminated => player.status = PlayerStatus::Eliminated,
						LeaveReason::Spectating => player.status = PlayerStatus::SittingOut,
						LeaveReason::Disconnected => {
							player.status = PlayerStatus::SittingOut;
							view.chat_messages.push(ChatMessage {
								sender: String::new(),
								text: format!("{} disconnected", name),
								is_system: true,
							});
						}
						_ => player.status = PlayerStatus::Eliminated,
					}
				}
			}

			GameEvent::GameEnded { reason, final_standings } => {
				let msg = match reason {
					GameEndReason::Winner => {
						if let Some(winner) = final_standings.first() {
							format!("{} wins the game!", winner.name)
						} else {
							"Game over".to_string()
						}
					}
					GameEndReason::AllPlayersLeft => "All players left".to_string(),
					GameEndReason::HostTerminated => "Host ended the game".to_string(),
					GameEndReason::Error => "Game ended due to error".to_string(),
				};

				view.chat_messages.push(ChatMessage {
					sender: String::new(),
					text: msg,
					is_system: true,
				});
			}

			_ => {}
		}
	}

	fn convert_street(&self, street: &Street) -> ViewStreet {
		match street {
			Street::Preflop => ViewStreet::Preflop,
			Street::Flop => ViewStreet::Flop,
			Street::Turn => ViewStreet::Turn,
			Street::River => ViewStreet::River,
			Street::Showdown => ViewStreet::Showdown,
		}
	}

	fn convert_position(&self, pos: &Position) -> ViewPosition {
		match pos {
			Position::Button => ViewPosition::Button,
			Position::SmallBlind => ViewPosition::SmallBlind,
			Position::BigBlind => ViewPosition::BigBlind,
			Position::None => ViewPosition::None,
		}
	}

	fn update_button(&self, view: &mut TableView, button: Seat) {
		for player in &mut view.players {
			player.position = if player.seat == button.0 {
				ViewPosition::Button
			} else {
				ViewPosition::None
			};
		}
	}

	fn player_name(&self, view: &TableView, seat: Seat) -> String {
		view.players
			.iter()
			.find(|p| p.seat == seat.0)
			.map(|p| p.name.clone())
			.unwrap_or_else(|| format!("Seat {}", seat.0))
	}

	fn build_action_prompt(&self, valid: &ValidActions) -> ActionPrompt {
		let (min_raise, max_bet, can_raise) = match &valid.raise_options {
			Some(RaiseOptions::Fixed { amount }) => {
				(*amount, *amount, true)
			}
			Some(RaiseOptions::Variable { min_raise, max_raise }) => {
				(*min_raise, *max_raise, true)
			}
			None => (0.0, valid.all_in_amount, false),
		};

		ActionPrompt {
			to_call: valid.call_amount.unwrap_or(0.0),
			min_raise,
			max_bet,
			can_raise,
			message: None,
		}
	}
}
