use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::sync::atomic::{AtomicBool, Ordering};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rs_poker::arena::{Agent, GameState, HoldemSimulationBuilder};
use tokio::runtime::Handle;

use crate::events::{
	Blinds, BettingStructure as EventBettingStructure, GameConfig, GameEndReason, GameEvent,
	GameId, HandId, HandResult, Position, Seat, SeatInfo, Standing,
};
use crate::logging;
use crate::players::{ActionRecord, PlayerPort};
use crate::engine::adapter::{BettingStructure, PlayerAdapter};
use std::collections::HashSet;

use crate::engine::historian::{EventHistorian, RakeConfig};
use crate::table::BlindClock;

fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct GameRunner {
	game_id: GameId,
	config: RunnerConfig,
	players: Vec<Arc<dyn PlayerPort>>,
	event_tx: mpsc::Sender<GameEvent>,
	action_history: Arc<Mutex<Vec<ActionRecord>>>,
	blind_clock: Option<BlindClock>,
	rng: StdRng,
	runtime_handle: Handle,
	quit_signal: Arc<AtomicBool>,
	sitting_out: Arc<Mutex<HashSet<Seat>>>,
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
	pub quit_signal: Arc<AtomicBool>,
	pub sitting_out: Arc<Mutex<HashSet<Seat>>>,
}

impl GameRunner {
	pub fn new(config: RunnerConfig, runtime_handle: Handle) -> (Self, GameHandle) {
		let (event_tx, event_rx) = mpsc::channel();
		let blind_clock = config.blind_clock.clone();

		let mut rng = match config.seed {
			Some(s) => StdRng::seed_from_u64(s),
			None => StdRng::from_os_rng(),
		};

		let game_id = GameId(rng.random());
		let quit_signal = Arc::new(AtomicBool::new(false));
		let sitting_out = Arc::new(Mutex::new(HashSet::new()));

		let runner = Self {
			game_id,
			config,
			players: Vec::new(),
			event_tx,
			action_history: Arc::new(Mutex::new(Vec::new())),
			blind_clock,
			rng,
			runtime_handle,
			quit_signal: Arc::clone(&quit_signal),
			sitting_out: Arc::clone(&sitting_out),
		};

		let handle = GameHandle {
			event_rx,
			game_id,
			quit_signal,
			sitting_out,
		};

		(runner, handle)
	}

	pub fn add_player(&mut self, player: Arc<dyn PlayerPort>) {
		let seat = player.seat();

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
				seat: p.seat(),
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

			if self.quit_signal.load(Ordering::SeqCst) {
				logging::engine::game_ended("User quit");
				break;
			}

			let sitting_out = lock_mutex(&self.sitting_out);
			let active_count = self.players.iter().enumerate()
				.filter(|(i, p)| stacks[*i] > 0.0 && !sitting_out.contains(&p.seat()))
				.count();
			drop(sitting_out);
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

			lock_mutex(&self.action_history).clear();

			let hand_id = HandId(self.rng.random());

			let seat_infos = self.build_seat_infos(&stacks, dealer_idx);
			let button_seat = self.players[dealer_idx].seat();

			self.emit(GameEvent::HandStarted {
				hand_id,
				hand_num,
				button: button_seat,
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
					button: button_seat,
					blinds: Blinds {
						small: small_blind,
						big: big_blind,
						ante: None,
					},
					seats: self.build_seat_infos(&stacks, dealer_idx),
				});
			}

			let sitting_out = lock_mutex(&self.sitting_out);

			// Build seat_map: player_idx -> table_seat
			let seat_map: Vec<Seat> = self.players.iter().map(|p| p.seat()).collect();

			// Create stacks for game state - sitting out players have 0 so they're skipped entirely
			let game_stacks: Vec<f32> = self.players.iter().enumerate()
				.map(|(i, p)| if sitting_out.contains(&p.seat()) { 0.0 } else { stacks[i] })
				.collect();

			let game_state = GameState::new_starting(
				game_stacks.clone(),
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
				.map(|(player_idx, p)| {
					let seat = p.seat();
					if stacks[player_idx] <= 0.0 || sitting_out.contains(&seat) {
						Box::new(FoldingAgent) as Box<dyn Agent>
					} else {
						Box::new(PlayerAdapter::new(
							Arc::clone(p),
							seat,
							player_idx,
							seat_map.clone(),
							self.config.betting_structure,
							Arc::clone(&self.action_history),
							self.event_tx.clone(),
							self.config.max_raises_per_round,
							self.runtime_handle.clone(),
						)) as Box<dyn Agent>
					}
				})
				.collect();
			drop(sitting_out);

			let rake_config = RakeConfig {
				percent: self.config.rake_percent,
				cap: self.config.rake_cap,
				no_flop_no_drop: self.config.no_flop_no_drop,
			};

			// Note: use game_stacks for historian so it's consistent with GameState
			// (sitting_out players have 0 stack in both)
			let historian = EventHistorian::with_rake(
				self.event_tx.clone(),
				player_names,
				hand_id,
				hand_num,
				game_stacks,
				rake_config,
				Arc::clone(&self.action_history),
				seat_map,
			);

			// Keep reference to hole cards and folded status for HandResult
			let hole_cards_ref = historian.hole_cards();
			let folded_ref = historian.folded();

			let mut sim = HoldemSimulationBuilder::default()
				.game_state(game_state)
				.agents(agents)
				.historians(vec![Box::new(historian)])
				.build()
				.expect("Failed to build holdem simulation");

			sim.run(&mut self.rng);

			let old_stacks = stacks.clone();
			let new_stacks = sim.game_state.stacks.clone();

			// Update stacks, but preserve sitting_out players' stacks (they didn't participate)
			let sitting_out = lock_mutex(&self.sitting_out);
			for (i, new_stack) in new_stacks.iter().enumerate() {
				if !sitting_out.contains(&self.players[i].seat()) {
					stacks[i] = *new_stack;
				}
			}
			drop(sitting_out);

			let hole_cards = lock_mutex(&hole_cards_ref);
			let folded = lock_mutex(&folded_ref);

			let results: Vec<HandResult> = self
				.players
				.iter()
				.enumerate()
				.map(|(i, p)| {
					let showed = if !folded[i] {
						hole_cards[i].clone()
					} else {
						None
					};
					HandResult {
						seat: p.seat(),
						stack_change: stacks[i] - old_stacks[i],
						final_stack: stacks[i],
						showed_cards: showed,
						hand_description: None,
					}
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
				seat: p.seat(),
				name: p.name().to_string(),
				final_stack: stacks[i],
				finish_position: 0,
			})
			.collect();

		standings.sort_by(|a, b| {
			b.final_stack.partial_cmp(&a.final_stack).unwrap_or(std::cmp::Ordering::Equal)
		});
		for (i, s) in standings.iter_mut().enumerate() {
			s.finish_position = (i + 1) as u8;
		}

		let reason = if self.quit_signal.load(Ordering::SeqCst) {
			GameEndReason::HostTerminated
		} else {
			GameEndReason::Winner
		};

		self.emit(GameEvent::GameEnded {
			reason,
			final_standings: standings,
		});
	}

	fn emit(&self, event: GameEvent) {
		let _ = self.event_tx.send(event);
	}

	fn build_seat_infos(&self, stacks: &[f32], dealer_idx: usize) -> Vec<SeatInfo> {
		let n = self.players.len();
		let sitting_out = lock_mutex(&self.sitting_out);
		self.players
			.iter()
			.enumerate()
			.map(|(i, p)| {
				let seat = p.seat();
				let is_sitting_out = sitting_out.contains(&seat);
				let is_active = stacks[i] > 0.0 && !is_sitting_out;
				let position = if !is_active {
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
					seat,
					name: p.name().to_string(),
					stack: stacks[i],
					position,
					is_active,
					is_human: p.is_human(),
				}
			})
			.collect()
	}

	fn next_active(&self, start: usize, stacks: &[f32]) -> usize {
		let n = stacks.len();
		let sitting_out = lock_mutex(&self.sitting_out);
		let mut idx = start;
		loop {
			idx = (idx + 1) % n;
			let seat = self.players[idx].seat();
			if stacks[idx] > 0.0 && !sitting_out.contains(&seat) {
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

#[cfg(test)]
mod tests {
	use super::*;

	fn make_test_config() -> RunnerConfig {
		RunnerConfig {
			small_blind: 1.0,
			big_blind: 2.0,
			starting_stack: 100.0,
			betting_structure: BettingStructure::NoLimit,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			seed: Some(42),
			max_hands: Some(1),
			blind_clock: None,
		}
	}

	#[test]
	fn test_runner_config_creation() {
		let config = make_test_config();
		assert_eq!(config.small_blind, 1.0);
		assert_eq!(config.big_blind, 2.0);
		assert_eq!(config.starting_stack, 100.0);
	}

	#[test]
	fn test_game_runner_creation() {
		let runtime = tokio::runtime::Runtime::new().unwrap();
		let config = make_test_config();
		let (runner, handle) = GameRunner::new(config, runtime.handle().clone());
		
		assert_eq!(runner.players.len(), 0);
		assert!(!handle.quit_signal.load(Ordering::SeqCst));
	}

	#[test]
	fn test_game_handle_quit_signal() {
		let runtime = tokio::runtime::Runtime::new().unwrap();
		let config = make_test_config();
		let (_runner, handle) = GameRunner::new(config, runtime.handle().clone());
		
		assert!(!handle.quit_signal.load(Ordering::SeqCst));
		handle.quit_signal.store(true, Ordering::SeqCst);
		assert!(handle.quit_signal.load(Ordering::SeqCst));
	}

	#[test]
	fn test_sitting_out_tracking() {
		let runtime = tokio::runtime::Runtime::new().unwrap();
		let config = make_test_config();
		let (_runner, handle) = GameRunner::new(config, runtime.handle().clone());
		
		{
			let mut sitting_out = handle.sitting_out.lock().unwrap();
			sitting_out.insert(Seat(0));
		}
		
		let sitting_out = handle.sitting_out.lock().unwrap();
		assert!(sitting_out.contains(&Seat(0)));
		assert!(!sitting_out.contains(&Seat(1)));
	}
}
