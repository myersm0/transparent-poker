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
		Self {
			id: config.id.clone(),
			name: config.name.clone(),
			format: config.format.to_string(),
			betting: config.betting.to_string(),
			blinds,
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
					let seat = Seat(self.lobby_players.len());
					let ai_bankroll = self.bank.get_bankroll(&ai_config.id);
					let player = LobbyPlayer {
						seat: Some(seat),
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
						seat,
						username: ai_config.display_name(),
						is_ai: true,
					});
				}
			}

			LobbyCommand::RemoveAI(seat) => {
				if let Some(idx) = self.lobby_players.iter().position(|p| p.seat == Some(seat) && !p.is_human) {
					self.lobby_players.remove(idx);
					self.reassign_seats();
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

	fn reassign_seats(&mut self) {
		for (i, player) in self.lobby_players.iter_mut().enumerate() {
			player.seat = Some(Seat(i));
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
}

impl NetworkBackend {
	pub fn new(client: GameClient) -> Self {
		Self {
			client,
			pending_events: Vec::new(),
		}
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
		while let Some(msg) = self.client.try_recv() {
			match msg {
				ServerMessage::LobbyState { tables } => {
					let summaries = tables.into_iter().map(|t| t.into()).collect();
					self.emit(LobbyEvent::TablesListed(summaries));
				}

				ServerMessage::TableJoined { table_id, table_name, seat, players, min_players, max_players } => {
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
					self.emit(LobbyEvent::GameStarting);
				}

				ServerMessage::Error { message } => {
					self.emit(LobbyEvent::Error(message));
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
		0.0
	}
}
