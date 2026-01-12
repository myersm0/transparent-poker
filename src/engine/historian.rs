use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, mpsc::Sender};
use rs_poker::arena::{
	GameState, Historian,
	action::{Action, AgentAction, ForcedBetType},
	historian::HistorianError,
	game_state::Round,
};

use crate::events::{
	BlindType, Card, ChatSender, GameEvent, HandId, PlayerAction, PotType, Seat, Street,
};
use crate::players::ActionRecord;

fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct RakeConfig {
	pub percent: f32,
	pub cap: Option<f32>,
	pub no_flop_no_drop: bool,
}

impl Default for RakeConfig {
	fn default() -> Self {
		Self {
			percent: 0.0,
			cap: None,
			no_flop_no_drop: false,
		}
	}
}

pub struct EventHistorian {
	event_tx: Sender<GameEvent>,
	player_names: Vec<String>,
	hand_id: HandId,
	hand_num: u32,
	board: Arc<Mutex<Vec<Card>>>,
	emitted_streets: Arc<Mutex<HashSet<u8>>>,
	hole_cards_captured: Arc<Mutex<bool>>,
	stacks: Vec<f32>,
	original_hole_cards: Arc<Mutex<Vec<Option<[Card; 2]>>>>,
	folded: Arc<Mutex<Vec<bool>>>,
	rake_config: RakeConfig,
	rake_collected: Arc<Mutex<f32>>,
	action_history: Arc<Mutex<Vec<ActionRecord>>>,
	current_street: Arc<Mutex<Street>>,
}

impl EventHistorian {
	pub fn with_rake(
		event_tx: Sender<GameEvent>,
		player_names: Vec<String>,
		hand_id: HandId,
		hand_num: u32,
		starting_stacks: Vec<f32>,
		rake_config: RakeConfig,
		action_history: Arc<Mutex<Vec<ActionRecord>>>,
	) -> Self {
		let num_players = starting_stacks.len();
		Self {
			event_tx,
			player_names,
			hand_id,
			hand_num,
			board: Arc::new(Mutex::new(Vec::new())),
			emitted_streets: Arc::new(Mutex::new(HashSet::new())),
			hole_cards_captured: Arc::new(Mutex::new(false)),
			stacks: starting_stacks,
			original_hole_cards: Arc::new(Mutex::new(vec![None; num_players])),
			folded: Arc::new(Mutex::new(vec![false; num_players])),
			rake_config,
			rake_collected: Arc::new(Mutex::new(0.0)),
			action_history,
			current_street: Arc::new(Mutex::new(Street::Preflop)),
		}
	}

	pub fn hole_cards(&self) -> Arc<Mutex<Vec<Option<[Card; 2]>>>> {
		Arc::clone(&self.original_hole_cards)
	}

	pub fn folded(&self) -> Arc<Mutex<Vec<bool>>> {
		Arc::clone(&self.folded)
	}

	fn emit(&self, event: GameEvent) {
		let _ = self.event_tx.send(event);
	}

	fn round_to_u8(round: Round) -> u8 {
		match round {
			Round::Preflop => 0,
			Round::Flop => 1,
			Round::Turn => 2,
			Round::River => 3,
			Round::Showdown => 4,
			_ => 255,
		}
	}

	fn capture_and_emit_hole_cards(&self, game_state: &GameState) {
		let mut captured = lock_mutex(&self.hole_cards_captured);
		if *captured {
			return;
		}
		*captured = true;

		let mut hole_cards = lock_mutex(&self.original_hole_cards);
		for (i, hand) in game_state.hands.iter().enumerate() {
			let cards: Vec<_> = hand.iter().take(2).collect();
			if cards.len() >= 2 {
				let hole = [convert_card(&cards[0]), convert_card(&cards[1])];
				hole_cards[i] = Some(hole);
				self.emit(GameEvent::HoleCardsDealt {
					seat: Seat(i),
					cards: hole,
				});
			}
		}
	}

	fn convert_street(&self, round: Round) -> Street {
		match round {
			Round::Preflop => Street::Preflop,
			Round::Flop => Street::Flop,
			Round::Turn => Street::Turn,
			Round::River => Street::River,
			Round::Showdown => Street::Showdown,
			_ => Street::Preflop,
		}
	}

	fn format_rank(rank: &rs_poker::core::Rank) -> String {
		let s = format!("{:?}", rank);
		if let Some(pos) = s.find('(') {
			s[..pos].to_string()
		} else {
			s
		}
	}
}

impl Historian for EventHistorian {
	fn record_action(
		&mut self,
		_id: u128,
		game_state: &GameState,
		action: Action,
	) -> Result<(), HistorianError> {
		match action {
			Action::RoundAdvance(round) => {
				let round_key = Self::round_to_u8(round);
				{
					let mut emitted = lock_mutex(&self.emitted_streets);
					if emitted.contains(&round_key) {
						return Ok(());
					}
					emitted.insert(round_key);
				}

				let street = self.convert_street(round);
				*lock_mutex(&self.current_street) = street;

				let board = lock_mutex(&self.board).clone();
				self.emit(GameEvent::StreetChanged {
					street,
					board,
				});

				if round == Round::Showdown {
					self.emit(GameEvent::ChatMessage {
						sender: ChatSender::Dealer,
						text: "Showdown".to_string(),
					});

					let hole_cards = lock_mutex(&self.original_hole_cards);
					let folded = lock_mutex(&self.folded);
					let mut reveals = Vec::new();

					for i in 0..hole_cards.len() {
						if !folded[i] {
							if let Some(cards) = &hole_cards[i] {
								let card_str = format!(
									"{}{} {}{}",
									cards[0].rank,
									card_suit_symbol(cards[0].suit),
									cards[1].rank,
									card_suit_symbol(cards[1].suit),
								);
								self.emit(GameEvent::ChatMessage {
									sender: ChatSender::Player(Seat(i)),
									text: format!("shows {}", card_str),
								});
								reveals.push((Seat(i), *cards));
							}
						}
					}

					self.emit(GameEvent::ShowdownReveal { reveals });
				}
			}

			Action::ForcedBet(payload) => {
				self.capture_and_emit_hole_cards(game_state);

				let blind_type = match payload.forced_bet_type {
					ForcedBetType::SmallBlind => BlindType::Small,
					ForcedBetType::BigBlind => BlindType::Big,
					ForcedBetType::Ante => BlindType::Ante,
				};

				self.stacks[payload.idx] = game_state.stacks[payload.idx];

				self.emit(GameEvent::BlindPosted {
					seat: Seat(payload.idx),
					blind_type,
					amount: payload.bet,
				});
			}

			Action::PlayedAction(payload) => {
				let stack_after = game_state.stacks[payload.idx];
				self.stacks[payload.idx] = stack_after;

				let action = match &payload.action {
					AgentAction::Fold => {
						lock_mutex(&self.folded)[payload.idx] = true;
						PlayerAction::Fold
					}
					AgentAction::Call => {
						if payload.final_player_bet <= payload.starting_player_bet {
							PlayerAction::Check
						} else {
							PlayerAction::Call {
								amount: payload.final_player_bet - payload.starting_player_bet,
							}
						}
					}
					AgentAction::Bet(amt) => {
						if payload.starting_bet == 0.0 {
							PlayerAction::Bet { amount: *amt }
						} else {
							PlayerAction::Raise { amount: *amt }
						}
					}
					AgentAction::AllIn => PlayerAction::AllIn {
						amount: payload.final_player_bet,
					},
				};

				let street = *lock_mutex(&self.current_street);
				lock_mutex(&self.action_history).push(ActionRecord {
					seat: Seat(payload.idx),
					action: action.clone(),
					street,
				});

				self.emit(GameEvent::ActionTaken {
					seat: Seat(payload.idx),
					action: action.clone(),
					stack_after,
					pot_after: game_state.total_pot,
				});

				self.emit(GameEvent::ChatMessage {
					sender: ChatSender::Player(Seat(payload.idx)),
					text: action.description(),
				});
			}

			Action::DealCommunity(card) => {
				lock_mutex(&self.board).push(convert_card(&card));
			}

			Action::Award(payload) => {
				let hand_desc = payload.rank.as_ref().map(Self::format_rank);

				let mut net_amount = payload.award_amount;
				let saw_flop = lock_mutex(&self.emitted_streets).contains(&1);

				if self.rake_config.percent > 0.0 && (!self.rake_config.no_flop_no_drop || saw_flop) {
					let mut rake = payload.award_amount * self.rake_config.percent;
					if let Some(cap) = self.rake_config.cap {
						rake = rake.min(cap);
					}
					rake = (rake * 100.0).round() / 100.0;
					net_amount = payload.award_amount - rake;

					if rake > 0.0 {
						*lock_mutex(&self.rake_collected) += rake;
						crate::logging::log("Engine", "RAKE", &format!("${:.2} collected", rake));
					}
				}

				self.emit(GameEvent::PotAwarded {
					seat: Seat(payload.idx),
					amount: net_amount,
					hand_description: hand_desc,
					pot_type: PotType::Main,
				});
			}

			_ => {}
		}

		Ok(())
	}
}

impl Clone for EventHistorian {
	fn clone(&self) -> Self {
		Self {
			event_tx: self.event_tx.clone(),
			player_names: self.player_names.clone(),
			hand_id: self.hand_id,
			hand_num: self.hand_num,
			board: Arc::clone(&self.board),
			emitted_streets: Arc::clone(&self.emitted_streets),
			hole_cards_captured: Arc::clone(&self.hole_cards_captured),
			stacks: self.stacks.clone(),
			original_hole_cards: Arc::clone(&self.original_hole_cards),
			folded: Arc::clone(&self.folded),
			rake_config: self.rake_config.clone(),
			rake_collected: Arc::clone(&self.rake_collected),
			action_history: Arc::clone(&self.action_history),
			current_street: Arc::clone(&self.current_street),
		}
	}
}

fn convert_card(card: &rs_poker::core::Card) -> Card {
	Card::new(rank_char(card), suit_char(card))
}

fn card_suit_symbol(suit: char) -> &'static str {
	match suit {
		's' => "♠",
		'h' => "♥",
		'd' => "♦",
		'c' => "♣",
		_ => "?",
	}
}

fn rank_char(card: &rs_poker::core::Card) -> char {
	match card.value {
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
	}
}

fn suit_char(card: &rs_poker::core::Card) -> char {
	match card.suit {
		rs_poker::core::Suit::Spade => 's',
		rs_poker::core::Suit::Heart => 'h',
		rs_poker::core::Suit::Diamond => 'd',
		rs_poker::core::Suit::Club => 'c',
	}
}
