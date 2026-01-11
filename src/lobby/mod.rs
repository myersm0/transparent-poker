use crate::bank::Bank;
use crate::config::PlayerConfig;
use crate::events::Seat;
use crate::net::client::GameClient;
use crate::net::protocol::{PlayerInfo, ServerMessage, TableInfo, TableStatus};
use crate::table::TableConfig;

#[derive(Debug, Clone)]
pub enum LobbyCommand {
	ListTables,
	JoinTable(String),
	LeaveTable,
	AddAI,
	RemoveAI(Seat),
	Ready,
	StartGame,
}

#[derive(Debug, Clone)]
pub enum LobbyEvent {
	TablesListed(Vec<TableSummary>),
	TableJoined {
		table_id: String,
		table_name: String,
		seat: Seat,
		players: Vec<LobbyPlayer>,
		min_players: usize,
		max_players: usize,
	},
	PlayerJoined {
		seat: Seat,
		username: String,
		is_ai: bool,
	},
	PlayerLeft {
		seat: Seat,
	},
	PlayerReady {
		seat: Seat,
	},
	GameStarting,
	GameReady {
		table: TableConfig,
		players: Vec<LobbyPlayer>,
	},
	NetworkGameStarted {
		seat: Seat,
	},
	Error(String),
	LeftTable,
}

#[derive(Debug, Clone)]
pub struct TableSummary {
	pub id: String,
	pub name: String,
	pub format: String,
	pub betting: String,
	pub blinds: String,
	pub buy_in: String,
	pub players: usize,
	pub max_players: usize,
	pub status: TableStatus,
}

impl From<TableInfo> for TableSummary {
	fn from(info: TableInfo) -> Self {
		Self {
			id: info.id,
			name: info.name,
			format: info.format,
			betting: info.betting,
			blinds: info.blinds,
			buy_in: info.buy_in,
			players: info.players,
			max_players: info.max_players,
			status: info.status,
		}
	}
}

impl From<&TableConfig> for TableSummary {
	fn from(config: &TableConfig) -> Self {
		let blinds = match (config.small_blind, config.big_blind) {
			(Some(sb), Some(bb)) => format!("${:.0}/${:.0}", sb, bb),
			_ => "N/A".to_string(),
		};
		let buy_in = config.effective_buy_in();
		Self {
			id: config.id.clone(),
			name: config.name.clone(),
			format: config.format.to_string(),
			betting: config.betting.to_string(),
			blinds,
			buy_in: format!("${:.0}", buy_in),
			players: 0,
			max_players: config.max_players,
			status: TableStatus::Waiting,
		}
	}
}

#[derive(Debug, Clone)]
pub struct LobbyPlayer {
	pub seat: Option<Seat>,
	pub id: String,
	pub name: String,
	pub is_host: bool,
	pub is_human: bool,
	pub is_ready: bool,
	pub strategy: Option<String>,
	pub bankroll: Option<f32>,
}

impl From<PlayerInfo> for LobbyPlayer {
	fn from(info: PlayerInfo) -> Self {
		Self {
			seat: Some(info.seat),
			id: info.username.to_lowercase(),
			name: info.username,
			is_host: false,
			is_human: !info.is_ai,
			is_ready: info.ready,
			strategy: None,
			bankroll: None,
		}
	}
}

pub trait LobbyBackend {
	fn send(&mut self, cmd: LobbyCommand);
	fn poll(&mut self) -> Option<LobbyEvent>;
	fn table_config(&self, table_id: &str) -> Option<TableConfig>;
	fn get_bankroll(&self, player_id: &str) -> f32;
}

pub struct LocalBackend {
	tables: Vec<TableConfig>,
	roster: Vec<PlayerConfig>,
	bank: Bank,
	host_id: String,
	pending_events: Vec<LobbyEvent>,
	current_table: Option<TableConfig>,
	lobby_players: Vec<LobbyPlayer>,
}

impl LocalBackend {
	pub fn new(tables: Vec<TableConfig>, roster: Vec<PlayerConfig>, bank: Bank, host_id: String) -> Self {
		Self {
			tables,
			roster,
			bank,
			host_id,
			pending_events: Vec::new(),
			current_table: None,
			lobby_players: Vec::new(),
		}
	}

	pub fn bank(&self) -> &Bank {
		&self.bank
	}

	pub fn bank_mut(&mut self) -> &mut Bank {
		&mut self.bank
	}

	fn emit(&mut self, event: LobbyEvent) {
		self.pending_events.push(event);
	}
}

impl LobbyBackend for LocalBackend {
	fn send(&mut self, cmd: LobbyCommand) {
		match cmd {
			LobbyCommand::ListTables => {
				let summaries: Vec<TableSummary> = self.tables.iter()
					.map(|t| t.into())
					.collect();
				self.emit(LobbyEvent::TablesListed(summaries));
			}

			LobbyCommand::JoinTable(table_id) => {
				if let Some(table) = self.tables.iter().find(|t| t.id == table_id) {
					self.current_table = Some(table.clone());
					self.lobby_players.clear();

					let host_bankroll = self.bank.get_bankroll(&self.host_id);
					let host_player = LobbyPlayer {
						seat: Some(Seat(0)),
						id: self.host_id.clone(),
						name: self.host_id.clone(),
						is_host: true,
						is_human: true,
						is_ready: false,
						strategy: None,
						bankroll: Some(host_bankroll),
					};
					self.lobby_players.push(host_player.clone());

					self.emit(LobbyEvent::TableJoined {
						table_id: table.id.clone(),
						table_name: table.name.clone(),
						seat: Seat(0),
						players: vec![host_player],
						min_players: table.min_players,
						max_players: table.max_players,
					});

					self.auto_fill_lobby();
				} else {
					self.emit(LobbyEvent::Error("Table not found".to_string()));
				}
			}

			LobbyCommand::LeaveTable => {
				self.current_table = None;
				self.lobby_players.clear();
				self.emit(LobbyEvent::LeftTable);
			}

			LobbyCommand::AddAI => {
				let max_players = self.current_table.as_ref().map(|t| t.max_players).unwrap_or(6);
				if self.lobby_players.len() >= max_players {
					return;
				}

				let used_ids: Vec<String> = self.lobby_players.iter().map(|p| p.id.clone()).collect();
				let mut available: Vec<_> = self.roster.iter()
					.filter(|p| !used_ids.contains(&p.id))
					.collect();

				use rand::seq::SliceRandom;
				available.shuffle(&mut rand::rng());

				let selected = available.iter()
					.find(|p| rand::random::<f32>() < p.join_probability)
					.copied()
					.or_else(|| available.first().copied());

				if let Some(ai_config) = selected {
					let next_seat = self.lobby_players.iter()
						.filter_map(|p| p.seat)
						.map(|s| s.0)
						.max()
						.map(|m| Seat(m + 1))
						.unwrap_or(Seat(0));
					let ai_bankroll = self.bank.get_bankroll(&ai_config.id);
					let player = LobbyPlayer {
						seat: Some(next_seat),
						id: ai_config.id.clone(),
						name: ai_config.display_name(),
						is_host: false,
						is_human: false,
						is_ready: true,
						strategy: Some(ai_config.strategy.clone()),
						bankroll: Some(ai_bankroll),
					};
					self.lobby_players.push(player);

					self.emit(LobbyEvent::PlayerJoined {
						seat: next_seat,
						username: ai_config.display_name(),
						is_ai: true,
					});
				}
			}

			LobbyCommand::RemoveAI(seat) => {
				if let Some(idx) = self.lobby_players.iter().position(|p| p.seat == Some(seat) && !p.is_human) {
					self.lobby_players.remove(idx);
					self.emit(LobbyEvent::PlayerLeft { seat });
				}
			}

			LobbyCommand::Ready => {
				if let Some(player) = self.lobby_players.iter_mut().find(|p| p.is_host) {
					player.is_ready = true;
					if let Some(seat) = player.seat {
						self.emit(LobbyEvent::PlayerReady { seat });
					}
				}

				if self.all_ready() && self.can_start() {
					self.emit(LobbyEvent::GameStarting);
				}
			}

			LobbyCommand::StartGame => {
				if let Some(table) = &self.current_table {
					if self.can_start() {
						self.emit(LobbyEvent::GameReady {
							table: table.clone(),
							players: self.lobby_players.clone(),
						});
					} else {
						self.emit(LobbyEvent::Error("Not enough players".to_string()));
					}
				}
			}
		}
	}

	fn poll(&mut self) -> Option<LobbyEvent> {
		if self.pending_events.is_empty() {
			None
		} else {
			Some(self.pending_events.remove(0))
		}
	}

	fn table_config(&self, table_id: &str) -> Option<TableConfig> {
		self.tables.iter().find(|t| t.id == table_id).cloned()
	}

	fn get_bankroll(&self, player_id: &str) -> f32 {
		self.bank.get_bankroll(player_id)
	}
}

impl LocalBackend {
	fn auto_fill_lobby(&mut self) {
		let max_players = self.current_table.as_ref().map(|t| t.max_players).unwrap_or(6);
		let used_ids: Vec<String> = self.lobby_players.iter().map(|p| p.id.clone()).collect();

		let mut available: Vec<_> = self.roster.iter()
			.filter(|p| !used_ids.contains(&p.id))
			.collect();

		use rand::seq::SliceRandom;
		available.shuffle(&mut rand::rng());

		// Collect players to add first to avoid borrow issues
		let mut to_add: Vec<(String, String, String, f32)> = Vec::new();
		let mut current_count = self.lobby_players.len();

		for ai_config in available {
			if current_count >= max_players {
				break;
			}
			if rand::random::<f32>() < ai_config.join_probability {
				let ai_bankroll = self.bank.get_bankroll(&ai_config.id);
				to_add.push((
					ai_config.id.clone(),
					ai_config.display_name(),
					ai_config.strategy.clone(),
					ai_bankroll,
				));
				current_count += 1;
			}
		}

		// Now add players and emit events
		for (id, name, strategy, bankroll) in to_add {
			let seat = Seat(self.lobby_players.len());
			let player = LobbyPlayer {
				seat: Some(seat),
				id,
				name: name.clone(),
				is_host: false,
				is_human: false,
				is_ready: true,
				strategy: Some(strategy),
				bankroll: Some(bankroll),
			};
			self.lobby_players.push(player);

			self.emit(LobbyEvent::PlayerJoined {
				seat,
				username: name,
				is_ai: true,
			});
		}
	}

	fn all_ready(&self) -> bool {
		self.lobby_players.iter().all(|p| p.is_ready)
	}

	fn can_start(&self) -> bool {
		if let Some(table) = &self.current_table {
			let count = self.lobby_players.len();
			count >= table.min_players && count <= table.max_players
		} else {
			false
		}
	}
}

pub struct NetworkBackend {
	client: GameClient,
	pending_events: Vec<LobbyEvent>,
	my_seat: Option<Seat>,
	game_started: bool,
	username: Option<String>,
	bankroll: f32,
}

impl NetworkBackend {
	pub fn new(client: GameClient) -> Self {
		Self {
			client,
			pending_events: Vec::new(),
			my_seat: None,
			game_started: false,
			username: None,
			bankroll: 0.0,
		}
	}

	pub fn username(&self) -> Option<&str> {
		self.username.as_deref()
	}

	pub fn into_client(self) -> GameClient {
		self.client
	}

	pub fn client_mut(&mut self) -> &mut GameClient {
		&mut self.client
	}

	fn emit(&mut self, event: LobbyEvent) {
		self.pending_events.push(event);
	}

	fn process_server_messages(&mut self) {
		if self.game_started {
			return;
		}

		while let Some(msg) = self.client.try_recv() {
			match msg {
				ServerMessage::LobbyState { tables } => {
					let summaries = tables.into_iter().map(|t| t.into()).collect();
					self.emit(LobbyEvent::TablesListed(summaries));
				}

				ServerMessage::TableJoined { table_id, table_name, seat, players, min_players, max_players } => {
					self.my_seat = Some(seat);
					let lobby_players = players.into_iter().map(|p| p.into()).collect();
					self.emit(LobbyEvent::TableJoined {
						table_id,
						table_name,
						seat,
						players: lobby_players,
						min_players,
						max_players,
					});
				}

				ServerMessage::PlayerJoinedTable { seat, username } => {
					self.emit(LobbyEvent::PlayerJoined {
						seat,
						username,
						is_ai: false,
					});
				}

				ServerMessage::PlayerLeftTable { seat, .. } => {
					self.emit(LobbyEvent::PlayerLeft { seat });
				}

				ServerMessage::PlayerReady { seat } => {
					self.emit(LobbyEvent::PlayerReady { seat });
				}

				ServerMessage::AIAdded { seat, name } => {
					self.emit(LobbyEvent::PlayerJoined {
						seat,
						username: name,
						is_ai: true,
					});
				}

				ServerMessage::AIRemoved { seat } => {
					self.emit(LobbyEvent::PlayerLeft { seat });
				}

				ServerMessage::GameStarting { .. } => {
					self.game_started = true;
					if let Some(seat) = self.my_seat {
						self.emit(LobbyEvent::NetworkGameStarted { seat });
					} else {
						self.emit(LobbyEvent::GameStarting);
					}
					return;
				}

				ServerMessage::TableLeft => {
					self.my_seat = None;
					self.emit(LobbyEvent::LeftTable);
				}

				ServerMessage::Error { message } => {
					self.emit(LobbyEvent::Error(message));
				}

				ServerMessage::Welcome { username, bankroll, .. } => {
					self.username = Some(username);
					self.bankroll = bankroll;
				}

				_ => {}
			}
		}
	}
}

impl LobbyBackend for NetworkBackend {
	fn send(&mut self, cmd: LobbyCommand) {
		let _ = match cmd {
			LobbyCommand::ListTables => {
				self.client.list_tables()
			}
			LobbyCommand::JoinTable(table_id) => {
				self.client.join_table(&table_id)
			}
			LobbyCommand::LeaveTable => {
				self.client.leave_table()
			}
			LobbyCommand::AddAI => {
				self.client.add_ai(None)
			}
			LobbyCommand::RemoveAI(seat) => {
				self.client.remove_ai(seat)
			}
			LobbyCommand::Ready | LobbyCommand::StartGame => {
				self.client.ready()
			}
		};
	}

	fn poll(&mut self) -> Option<LobbyEvent> {
		self.process_server_messages();
		if self.pending_events.is_empty() {
			None
		} else {
			Some(self.pending_events.remove(0))
		}
	}

	fn table_config(&self, _table_id: &str) -> Option<TableConfig> {
		None
	}

	fn get_bankroll(&self, _player_id: &str) -> f32 {
		self.bankroll
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::table::{BettingStructure, GameFormat};
	use std::collections::HashMap;

	fn make_test_table(id: &str, min: usize, max: usize) -> TableConfig {
		TableConfig {
			id: id.to_string(),
			name: format!("Test Table {}", id),
			format: GameFormat::Cash,
			betting: BettingStructure::NoLimit,
			small_blind: Some(1.0),
			big_blind: Some(2.0),
			min_buy_in: Some(40.0),
			max_buy_in: Some(200.0),
			buy_in: None,
			starting_stack: None,
			min_players: min,
			max_players: max,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			blind_levels: None,
			payouts: None,
			action_delay_ms: 0,
			street_delay_ms: 0,
			hand_end_delay_ms: 0,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			seed: None,
		}
	}

	fn make_test_roster() -> Vec<PlayerConfig> {
		vec![
			PlayerConfig {
				id: "alice".to_string(),
				name: Some("Alice".to_string()),
				strategy: "default".to_string(),
				strategy_model: None,
				version: "0.1".to_string(),
				join_probability: 1.0,
			},
			PlayerConfig {
				id: "bob".to_string(),
				name: Some("Bob".to_string()),
				strategy: "default".to_string(),
				strategy_model: None,
				version: "0.1".to_string(),
				join_probability: 1.0,
			},
			PlayerConfig {
				id: "carol".to_string(),
				name: Some("Carol".to_string()),
				strategy: "default".to_string(),
				strategy_model: None,
				version: "0.1".to_string(),
				join_probability: 1.0,
			},
		]
	}

	fn make_test_bank() -> Bank {
		let mut profiles = HashMap::new();
		profiles.insert("host".to_string(), crate::bank::PlayerProfile {
			bankroll: 1000.0,
		});
		profiles.insert("alice".to_string(), crate::bank::PlayerProfile {
			bankroll: 500.0,
		});
		profiles.insert("bob".to_string(), crate::bank::PlayerProfile {
			bankroll: 500.0,
		});
		profiles.insert("carol".to_string(), crate::bank::PlayerProfile {
			bankroll: 500.0,
		});
		Bank::new_for_testing(profiles)
	}

	#[test]
	fn test_list_tables() {
		let tables = vec![
			make_test_table("table1", 2, 6),
			make_test_table("table2", 2, 10),
		];
		let mut backend = LocalBackend::new(tables, vec![], make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::ListTables);

		let event = backend.poll().expect("Should have event");
		match event {
			LobbyEvent::TablesListed(summaries) => {
				assert_eq!(summaries.len(), 2);
				assert_eq!(summaries[0].id, "table1");
				assert_eq!(summaries[1].id, "table2");
			}
			_ => panic!("Expected TablesListed event"),
		}
	}

	#[test]
	fn test_join_table() {
		let tables = vec![make_test_table("table1", 2, 6)];
		let mut backend = LocalBackend::new(tables, make_test_roster(), make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("table1".to_string()));

		let event = backend.poll().expect("Should have event");
		match event {
			LobbyEvent::TableJoined { table_id, seat, players, min_players, max_players, .. } => {
				assert_eq!(table_id, "table1");
				assert_eq!(seat, Seat(0));
				assert_eq!(players.len(), 1);
				assert!(players[0].is_host);
				assert_eq!(min_players, 2);
				assert_eq!(max_players, 6);
			}
			_ => panic!("Expected TableJoined event"),
		}
	}

	#[test]
	fn test_join_nonexistent_table() {
		let tables = vec![make_test_table("table1", 2, 6)];
		let mut backend = LocalBackend::new(tables, vec![], make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("nonexistent".to_string()));

		let event = backend.poll().expect("Should have event");
		match event {
			LobbyEvent::Error(msg) => {
				assert!(msg.contains("not found"));
			}
			_ => panic!("Expected Error event"),
		}
	}

	#[test]
	fn test_auto_fill_lobby() {
		let tables = vec![make_test_table("table1", 2, 4)];
		let mut backend = LocalBackend::new(tables, make_test_roster(), make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("table1".to_string()));

		// Drain events - should have TableJoined + multiple PlayerJoined
		let mut player_joined_count = 0;
		while let Some(event) = backend.poll() {
			if matches!(event, LobbyEvent::PlayerJoined { .. }) {
				player_joined_count += 1;
			}
		}

		// Should have auto-filled with AI players (up to max_players - 1 host)
		assert!(player_joined_count >= 1, "Should auto-fill with AI players");
	}

	#[test]
	fn test_add_ai() {
		let tables = vec![make_test_table("table1", 2, 6)];
		let roster = vec![
			PlayerConfig {
				id: "testai".to_string(),
				name: Some("TestAI".to_string()),
				strategy: "default".to_string(),
				strategy_model: None,
				version: "0.1".to_string(),
				join_probability: 0.0,
			},
		];
		let mut backend = LocalBackend::new(tables, roster, make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("table1".to_string()));
		while backend.poll().is_some() {}

		backend.send(LobbyCommand::AddAI);

		let event = backend.poll().expect("Should have event");
		match event {
			LobbyEvent::PlayerJoined { username, is_ai, .. } => {
				assert_eq!(username, "TestAI");
				assert!(is_ai);
			}
			_ => panic!("Expected PlayerJoined event"),
		}
	}

	#[test]
	fn test_remove_ai() {
		let tables = vec![make_test_table("table1", 2, 6)];
		let roster = vec![
			PlayerConfig {
				id: "testai".to_string(),
				name: Some("TestAI".to_string()),
				strategy: "default".to_string(),
				strategy_model: None,
				version: "0.1".to_string(),
				join_probability: 1.0,
			},
		];
		let mut backend = LocalBackend::new(tables, roster, make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("table1".to_string()));

		// Find the AI's seat
		let mut ai_seat = None;
		while let Some(event) = backend.poll() {
			if let LobbyEvent::PlayerJoined { seat, is_ai: true, .. } = event {
				ai_seat = Some(seat);
			}
		}

		let seat = ai_seat.expect("Should have AI player");
		backend.send(LobbyCommand::RemoveAI(seat));

		let event = backend.poll().expect("Should have event");
		match event {
			LobbyEvent::PlayerLeft { seat: left_seat } => {
				assert_eq!(left_seat, seat);
			}
			_ => panic!("Expected PlayerLeft event"),
		}
	}

	#[test]
	fn test_ready_command() {
		let tables = vec![make_test_table("table1", 2, 6)];
		let mut backend = LocalBackend::new(tables, make_test_roster(), make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("table1".to_string()));
		while backend.poll().is_some() {}

		backend.send(LobbyCommand::Ready);

		let mut saw_ready = false;
		while let Some(event) = backend.poll() {
			if let LobbyEvent::PlayerReady { seat } = event {
				assert_eq!(seat, Seat(0)); // Host seat
				saw_ready = true;
			}
		}
		assert!(saw_ready, "Should emit PlayerReady event");
	}

	#[test]
	fn test_start_game() {
		let tables = vec![make_test_table("table1", 2, 6)];
		let mut backend = LocalBackend::new(tables, make_test_roster(), make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("table1".to_string()));
		while backend.poll().is_some() {}

		backend.send(LobbyCommand::Ready);
		while backend.poll().is_some() {}

		backend.send(LobbyCommand::StartGame);

		let event = backend.poll().expect("Should have event");
		match event {
			LobbyEvent::GameReady { table, players } => {
				assert_eq!(table.id, "table1");
				assert!(players.len() >= 2);
			}
			_ => panic!("Expected GameReady event, got {:?}", event),
		}
	}

	#[test]
	fn test_leave_table() {
		let tables = vec![make_test_table("table1", 2, 6)];
		let mut backend = LocalBackend::new(tables, vec![], make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("table1".to_string()));
		while backend.poll().is_some() {}

		backend.send(LobbyCommand::LeaveTable);

		let event = backend.poll().expect("Should have event");
		assert!(matches!(event, LobbyEvent::LeftTable));
	}

	#[test]
	fn test_table_config_lookup() {
		let tables = vec![make_test_table("table1", 2, 6)];
		let backend = LocalBackend::new(tables, vec![], make_test_bank(), "host".to_string());

		let config = backend.table_config("table1");
		assert!(config.is_some());
		assert_eq!(config.unwrap().id, "table1");

		let missing = backend.table_config("nonexistent");
		assert!(missing.is_none());
	}

	#[test]
	fn test_get_bankroll() {
		let backend = LocalBackend::new(vec![], vec![], make_test_bank(), "host".to_string());

		assert_eq!(backend.get_bankroll("host"), 1000.0);
		assert_eq!(backend.get_bankroll("alice"), 500.0);
		assert_eq!(backend.get_bankroll("unknown"), 1000.0); // Default bankroll
	}

	#[test]
	fn test_seat_assignment_after_removal() {
		let tables = vec![make_test_table("table1", 2, 10)];
		let roster = vec![
			PlayerConfig {
				id: "ai1".to_string(),
				name: Some("AI1".to_string()),
				strategy: "default".to_string(),
				strategy_model: None,
				version: "0.1".to_string(),
				join_probability: 1.0,
			},
			PlayerConfig {
				id: "ai2".to_string(),
				name: Some("AI2".to_string()),
				strategy: "default".to_string(),
				strategy_model: None,
				version: "0.1".to_string(),
				join_probability: 1.0,
			},
		];
		let mut backend = LocalBackend::new(tables, roster, make_test_bank(), "host".to_string());

		backend.send(LobbyCommand::JoinTable("table1".to_string()));

		let mut seats = vec![];
		while let Some(event) = backend.poll() {
			if let LobbyEvent::PlayerJoined { seat, .. } = event {
				seats.push(seat);
			}
		}

		// Remove first AI
		if let Some(&seat) = seats.first() {
			backend.send(LobbyCommand::RemoveAI(seat));
			while backend.poll().is_some() {}
		}

		// Add new AI - should get a new seat number (not reuse old one immediately)
		backend.send(LobbyCommand::AddAI);
		let event = backend.poll();
		if let Some(LobbyEvent::PlayerJoined { seat, .. }) = event {
			// New seat should be higher than any existing
			assert!(seat.0 > 0, "New AI should get a fresh seat number");
		}
	}

	#[test]
	fn test_table_summary_from_config() {
		let config = make_test_table("test", 2, 6);
		let summary: TableSummary = (&config).into();

		assert_eq!(summary.id, "test");
		assert_eq!(summary.format, "Cash");
		assert_eq!(summary.betting, "No-Limit");
		assert_eq!(summary.blinds, "$1/$2");
		assert_eq!(summary.buy_in, "$40");
		assert_eq!(summary.max_players, 6);
	}

	#[test]
	fn test_lobby_player_from_player_info() {
		let info = PlayerInfo {
			seat: Seat(3),
			username: "TestPlayer".to_string(),
			ready: true,
			is_ai: false,
		};
		let player: LobbyPlayer = info.into();

		assert_eq!(player.seat, Some(Seat(3)));
		assert_eq!(player.id, "testplayer"); // lowercase
		assert_eq!(player.name, "TestPlayer");
		assert!(player.is_ready);
		assert!(player.is_human);
	}
}
