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
		table_config: TableConfig,
		num_players: usize,
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

pub struct NetworkBackend {
	client: GameClient,
	pending_events: Vec<LobbyEvent>,
	my_seat: Option<Seat>,
	game_started: bool,
	username: Option<String>,
	bankroll: f32,
	tables: Vec<TableInfo>,
	lobby_players: Vec<LobbyPlayer>,
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
			tables: Vec::new(),
			lobby_players: Vec::new(),
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
					self.tables = tables.clone();
					let summaries = tables.into_iter().map(|t| t.into()).collect();
					self.emit(LobbyEvent::TablesListed(summaries));
				}

				ServerMessage::TableJoined { table_id, table_name, seat, players, min_players, max_players } => {
					self.my_seat = Some(seat);
					let lobby_players: Vec<LobbyPlayer> = players.into_iter().map(|p| p.into()).collect();
					self.lobby_players = lobby_players.clone();
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
					self.lobby_players.push(LobbyPlayer {
						seat: Some(seat),
						id: username.to_lowercase(),
						name: username.clone(),
						is_host: false,
						is_human: true,
						is_ready: false,
						strategy: None,
						bankroll: None,
					});
					self.emit(LobbyEvent::PlayerJoined {
						seat,
						username,
						is_ai: false,
					});
				}

				ServerMessage::PlayerLeftTable { seat, .. } => {
					self.lobby_players.retain(|p| p.seat != Some(seat));
					self.emit(LobbyEvent::PlayerLeft { seat });
				}

				ServerMessage::PlayerReady { seat } => {
					self.emit(LobbyEvent::PlayerReady { seat });
				}

				ServerMessage::AIAdded { seat, name } => {
					self.lobby_players.push(LobbyPlayer {
						seat: Some(seat),
						id: name.to_lowercase(),
						name: name.clone(),
						is_host: false,
						is_human: false,
						is_ready: true,
						strategy: None,
						bankroll: None,
					});
					self.emit(LobbyEvent::PlayerJoined {
						seat,
						username: name,
						is_ai: true,
					});
				}

				ServerMessage::AIRemoved { seat } => {
					self.lobby_players.retain(|p| p.seat != Some(seat));
					self.emit(LobbyEvent::PlayerLeft { seat });
				}

				ServerMessage::GameStarting { table_config, .. } => {
					self.game_started = true;
					let num_players = self.lobby_players.len();
					if let Some(seat) = self.my_seat {
						self.emit(LobbyEvent::NetworkGameStarted {
							seat,
							table_config,
							num_players,
						});
					} else {
						self.emit(LobbyEvent::GameStarting);
					}
					return;
				}

				ServerMessage::TableLeft => {
					self.my_seat = None;
					self.lobby_players.clear();
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

	fn table_config(&self, table_id: &str) -> Option<TableConfig> {
		self.tables.iter()
			.find(|t| t.id == table_id)
			.map(|t| t.config.clone())
	}

	fn get_bankroll(&self, _player_id: &str) -> f32 {
		self.bankroll
	}
}

