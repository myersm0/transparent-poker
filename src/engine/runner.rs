use std::sync::{mpsc, Arc, Mutex};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rs_poker::arena::{Agent, GameState, HoldemSimulationBuilder};

use crate::events::{
	Blinds, BettingStructure as EventBettingStructure, GameConfig, GameEndReason, GameEvent,
	GameId, HandId, HandResult, Position, Seat, SeatInfo, Standing,
};
use crate::logging;
use crate::players::{ActionRecord, PlayerPort};
use crate::engine::adapter::{BettingStructure, PlayerAdapter};
use crate::engine::historian::{EventHistorian, RakeConfig};
use crate::table::BlindClock;

pub struct GameRunner {
	game_id: GameId,
	config: RunnerConfig,
	players: Vec<Arc<dyn PlayerPort>>,
	event_tx: mpsc::Sender<GameEvent>,
	action_history: Arc<Mutex<Vec<ActionRecord>>>,
	blind_clock: Option<BlindClock>,
	rng: StdRng,
}

pub struct RunnerConfig {
	pub small_blind: f32,
	pub big_blind: f32,
	pub starting_stack: f32,
	pub betting_structure: BettingStructure,
	pub blind_clock: Option<BlindClock>,
	pub max_raises_per_round: u32,
	pub rake_percent: f32,
	pub rake_cap: Option<f32>,
	pub no_flop_no_drop: bool,
	pub max_hands: Option<u32>,
	pub seed: Option<u64>,
}

impl Default for RunnerConfig {
	fn default() -> Self {
		Self {
			small_blind: 5.0,
			big_blind: 10.0,
			starting_stack: 500.0,
			betting_structure: BettingStructure::NoLimit,
			blind_clock: None,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			max_hands: None,
			seed: None,
		}
	}
}

pub struct GameHandle {
	pub event_rx: mpsc::Receiver<GameEvent>,
	pub game_id: GameId,
}

impl GameRunner {
	pub fn new(config: RunnerConfig) -> (Self, GameHandle) {
		let (event_tx, event_rx) = mpsc::channel();
		let blind_clock = config.blind_clock.clone();

		let mut rng = match config.seed {
			Some(s) => StdRng::seed_from_u64(s),
			None => StdRng::from_os_rng(),
		};

		let game_id = GameId(rng.random());

		let runner = Self {
			game_id,
			config,
			players: Vec::new(),
			event_tx,
			action_history: Arc::new(Mutex::new(Vec::new())),
			blind_clock,
			rng,
		};

		let handle = GameHandle { event_rx, game_id };

		(runner, handle)
	}

	pub fn add_player(&mut self, player: Arc<dyn PlayerPort>) {
		let seat = Seat(self.players.len());

		self.emit(GameEvent::PlayerJoined {
			seat,
			name: player.name().to_string(),
			stack: self.config.starting_stack,
			is_human: player.is_human(),
		});

		self.players.push(player);
	}

	pub fn run(&mut self) {
		let num_players = self.players.len();
		if num_players < 2 {
			self.emit(GameEvent::GameEnded {
				reason: GameEndReason::Error,
				final_standings: vec![],
			});
			return;
		}

		self.emit(GameEvent::GameCreated {
			game_id: self.game_id,
			config: GameConfig {
				betting_structure: self.convert_betting_structure(),
				small_blind: self.config.small_blind,
				big_blind: self.config.big_blind,
				starting_stack: self.config.starting_stack,
				max_players: 10,
				time_bank: None,
			},
		});

		logging::set_game_id(self.game_id.0);

		for player in &self.players {
			player.notify(&GameEvent::GameCreated {
				game_id: self.game_id,
				config: GameConfig {
					betting_structure: self.convert_betting_structure(),
					small_blind: self.config.small_blind,
					big_blind: self.config.big_blind,
					starting_stack: self.config.starting_stack,
					max_players: 10,
					time_bank: None,
				},
			});
		}

		let seat_infos: Vec<SeatInfo> = self
			.players
			.iter()
			.enumerate()
			.map(|(i, p)| SeatInfo {
				seat: Seat(i),
				name: p.name().to_string(),
				stack: self.config.starting_stack,
				position: Position::None,
				is_active: true,
				is_human: p.is_human(),
			})
			.collect();

		self.emit(GameEvent::GameStarted {
			seats: seat_infos,
		});

		let mut stacks = vec![self.config.starting_stack; num_players];
		let mut dealer_idx = 0;
		let mut hand_num: u32 = 0;

		loop {
			hand_num += 1;
			logging::set_hand_num(hand_num);

			let active_count = stacks.iter().filter(|&&s| s > 0.0).count();
			if active_count <= 1 {
				break;
			}

			if let Some(max) = self.config.max_hands {
				if hand_num > max {
					break;
				}
			}

			let (small_blind, big_blind) = if let Some(ref clock) = self.blind_clock {
				clock.current()
			} else {
				(self.config.small_blind, self.config.big_blind)
			};

			self.action_history.lock().unwrap().clear();

			let hand_id = HandId(self.rng.random());

			let seat_infos = self.build_seat_infos(&stacks, dealer_idx);

			self.emit(GameEvent::HandStarted {
				hand_id,
				hand_num,
				button: Seat(dealer_idx),
				blinds: Blinds {
					small: small_blind,
					big: big_blind,
					ante: None,
				},
				seats: seat_infos,
			});

			logging::engine::hand_started(dealer_idx, num_players);

			for player in &self.players {
				player.notify(&GameEvent::HandStarted {
					hand_id,
					hand_num,
					button: Seat(dealer_idx),
					blinds: Blinds {
						small: small_blind,
						big: big_blind,
						ante: None,
					},
					seats: self.build_seat_infos(&stacks, dealer_idx),
				});
			}

			let game_state = GameState::new_starting(
				stacks.clone(),
				big_blind,
				small_blind,
				0.0,
				dealer_idx,
			);

			let player_names: Vec<String> = self.players.iter().map(|p| p.name().to_string()).collect();

			let agents: Vec<Box<dyn Agent>> = self
				.players
				.iter()
				.enumerate()
				.map(|(i, p)| {
					if stacks[i] <= 0.0 {
						Box::new(FoldingAgent) as Box<dyn Agent>
					} else {
						Box::new(PlayerAdapter::new(
							Arc::clone(p),
							Seat(i),
							self.config.betting_structure,
							Arc::clone(&self.action_history),
							self.event_tx.clone(),
							self.config.max_raises_per_round,
						)) as Box<dyn Agent>
					}
				})
				.collect();

			let rake_config = RakeConfig {
				percent: self.config.rake_percent,
				cap: self.config.rake_cap,
				no_flop_no_drop: self.config.no_flop_no_drop,
			};

			let historian = EventHistorian::with_rake(
				self.event_tx.clone(),
				player_names,
				hand_id,
				hand_num,
				stacks.clone(),
				rake_config,
			);

			let mut sim = HoldemSimulationBuilder::default()
				.game_state(game_state)
				.agents(agents)
				.historians(vec![Box::new(historian)])
				.build()
				.unwrap();

			sim.run(&mut self.rng);

			let old_stacks = stacks.clone();
			stacks = sim.game_state.stacks.clone();

			let results: Vec<HandResult> = self
				.players
				.iter()
				.enumerate()
				.map(|(i, _p)| HandResult {
					seat: Seat(i),
					stack_change: stacks[i] - old_stacks[i],
					final_stack: stacks[i],
					showed_cards: None,
					hand_description: None,
				})
				.collect();

			self.emit(GameEvent::HandEnded { hand_id, results });

			if let Some(ref mut clock) = self.blind_clock {
				clock.advance_hand();
			}

			dealer_idx = self.next_active(dealer_idx, &stacks);
		}

		let mut standings: Vec<Standing> = self
			.players
			.iter()
			.enumerate()
			.map(|(i, p)| Standing {
				seat: Seat(i),
				name: p.name().to_string(),
				final_stack: stacks[i],
				finish_position: 0,
			})
			.collect();

		standings.sort_by(|a, b| b.final_stack.partial_cmp(&a.final_stack).unwrap());
		for (i, s) in standings.iter_mut().enumerate() {
			s.finish_position = (i + 1) as u8;
		}

		self.emit(GameEvent::GameEnded {
			reason: GameEndReason::Winner,
			final_standings: standings,
		});
	}

	fn emit(&self, event: GameEvent) {
		let _ = self.event_tx.send(event);
	}

	fn build_seat_infos(&self, stacks: &[f32], dealer_idx: usize) -> Vec<SeatInfo> {
		let n = self.players.len();
		self.players
			.iter()
			.enumerate()
			.map(|(i, p)| {
				let position = if stacks[i] <= 0.0 {
					Position::None
				} else if i == dealer_idx {
					Position::Button
				} else if i == (dealer_idx + 1) % n {
					Position::SmallBlind
				} else if i == (dealer_idx + 2) % n {
					Position::BigBlind
				} else {
					Position::None
				};

				SeatInfo {
					seat: Seat(i),
					name: p.name().to_string(),
					stack: stacks[i],
					position,
					is_active: stacks[i] > 0.0,
					is_human: p.is_human(),
				}
			})
			.collect()
	}

	fn next_active(&self, start: usize, stacks: &[f32]) -> usize {
		let n = stacks.len();
		let mut idx = start;
		loop {
			idx = (idx + 1) % n;
			if stacks[idx] > 0.0 {
				return idx;
			}
			if idx == start {
				return start;
			}
		}
	}

	fn convert_betting_structure(&self) -> EventBettingStructure {
		match self.config.betting_structure {
			BettingStructure::NoLimit => EventBettingStructure::NoLimit,
			BettingStructure::PotLimit => EventBettingStructure::PotLimit,
			BettingStructure::FixedLimit => EventBettingStructure::FixedLimit,
		}
	}
}

struct FoldingAgent;

impl Agent for FoldingAgent {
	fn act(&mut self, _id: u128, game_state: &GameState) -> rs_poker::arena::action::AgentAction {
		let to_call = game_state.current_round_bet() - game_state.current_round_current_player_bet();
		if to_call <= 0.0 {
			rs_poker::arena::action::AgentAction::Call
		} else {
			rs_poker::arena::action::AgentAction::Fold
		}
	}
}
