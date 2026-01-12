use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread;

use crate::bank::Bank;
use crate::config::{load_players_auto, load_strategies_auto, PlayerConfig};
use crate::engine::{BettingStructure, GameRunner, RunnerConfig};
use crate::events::{Card, GameEvent, LeaveReason, PlayerAction, Seat};
use crate::net::protocol::*;
use crate::net::remote_player::RemotePlayer;
use crate::players::RulesPlayer;
use crate::table::{load_tables, TableConfig};

type ConnectionId = u64;

// =============================================================================
// LOCK ORDERING
// =============================================================================
// To prevent deadlocks, always acquire locks in this order:
//   1. tables
//   2. connections
//   3. bank
//
// NEVER acquire `connections` before `tables`, or `bank` before either.
// When possible, release earlier locks before acquiring later ones.
// =============================================================================

fn lock_tables(tables: &Mutex<HashMap<String, TableRoom>>) -> MutexGuard<'_, HashMap<String, TableRoom>> {
	tables.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_connections(connections: &Mutex<HashMap<ConnectionId, Connection>>) -> MutexGuard<'_, HashMap<ConnectionId, Connection>> {
	connections.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_bank(bank: &Mutex<Bank>) -> MutexGuard<'_, Bank> {
	bank.lock().unwrap_or_else(|e| e.into_inner())
}

struct Connection {
	username: Option<String>,
	stream: TcpStream,
	current_table: Option<String>,
}

impl Connection {
	fn send(&mut self, msg: &ServerMessage) {
		let data = encode_message(msg);
		if let Err(e) = self.stream.write_all(&data) {
			if e.kind() != std::io::ErrorKind::BrokenPipe {
				eprintln!("Failed to send message to client: {}", e);
			}
		}
	}
}

use std::sync::atomic::{AtomicBool, Ordering};

struct ActiveGame {
	action_senders: HashMap<Seat, mpsc::Sender<PlayerAction>>,
	conn_to_seat: HashMap<ConnectionId, Seat>,
	sitting_out: Arc<Mutex<std::collections::HashSet<Seat>>>,
	game_finished: Arc<AtomicBool>,
	quit_signal: Arc<AtomicBool>,
}

impl ActiveGame {
	fn new(
		sitting_out: Arc<Mutex<std::collections::HashSet<Seat>>>,
		game_finished: Arc<AtomicBool>,
		quit_signal: Arc<AtomicBool>,
	) -> Self {
		Self {
			action_senders: HashMap::new(),
			conn_to_seat: HashMap::new(),
			sitting_out,
			game_finished,
			quit_signal,
		}
	}

	fn register_player(&mut self, conn_id: ConnectionId, seat: Seat, action_tx: mpsc::Sender<PlayerAction>) {
		self.action_senders.insert(seat, action_tx);
		self.conn_to_seat.insert(conn_id, seat);
	}

	fn remove_player(&mut self, conn_id: ConnectionId) -> Option<Seat> {
		if let Some(seat) = self.conn_to_seat.remove(&conn_id) {
			self.action_senders.remove(&seat);
			self.sitting_out.lock().unwrap_or_else(|e| e.into_inner()).insert(seat);
			Some(seat)
		} else {
			None
		}
	}

	fn submit_action(&self, conn_id: ConnectionId, action: PlayerAction) -> Result<(), String> {
		let seat = self.conn_to_seat.get(&conn_id).ok_or("Player not in game")?;
		let tx = self.action_senders.get(seat).ok_or("No action channel for seat")?;
		tx.send(action).map_err(|_| "Failed to send action".to_string())
	}

	fn is_finished(&self) -> bool {
		self.game_finished.load(Ordering::SeqCst)
	}

	fn has_humans(&self) -> bool {
		!self.conn_to_seat.is_empty()
	}

	fn signal_quit(&self) {
		self.quit_signal.store(true, Ordering::SeqCst);
	}
}

struct AIPlayer {
	id: String,
	name: String,
	strategy: String,
}

struct GameStartInfo {
	config: TableConfig,
	human_players: Vec<(ConnectionId, Seat, String, TcpStream)>, // conn_id, seat, username, stream
	ai_players: Vec<(Seat, String, String, String)>, // seat, id, name, strategy
	player_bank_ids: Vec<String>, // bank ids for all players in seat order
}

struct TableRoom {
	config: TableConfig,
	order: usize,
	players: HashMap<Seat, ConnectionId>,
	ai_players: HashMap<Seat, AIPlayer>,
	ready: HashMap<Seat, bool>,
	status: TableStatus,
	active_game: Option<ActiveGame>,
}

impl TableRoom {
	fn new(config: TableConfig, order: usize) -> Self {
		Self {
			config,
			order,
			players: HashMap::new(),
			ai_players: HashMap::new(),
			ready: HashMap::new(),
			status: TableStatus::Waiting,
			active_game: None,
		}
	}

	fn player_count(&self) -> usize {
		self.players.len() + self.ai_players.len()
	}

	fn find_empty_seat(&self) -> Option<Seat> {
		for i in 0..self.config.max_players {
			let seat = Seat(i);
			if !self.players.contains_key(&seat) && !self.ai_players.contains_key(&seat) {
				return Some(seat);
			}
		}
		None
	}

	fn add_player(&mut self, seat: Seat, conn_id: ConnectionId) {
		self.players.insert(seat, conn_id);
		self.ready.insert(seat, false);
	}

	fn remove_player(&mut self, conn_id: ConnectionId) -> Option<Seat> {
		let seat = self.players.iter()
			.find(|&(_, &id)| id == conn_id)
			.map(|(&seat, _)| seat);
		if let Some(s) = seat {
			self.players.remove(&s);
			self.ready.remove(&s);
			if let Some(ref mut active_game) = self.active_game {
				active_game.remove_player(conn_id);
			}
		}
		seat
	}

	fn add_ai(&mut self, seat: Seat, id: String, name: String, strategy: String) {
		self.ai_players.insert(seat, AIPlayer { id, name, strategy });
		self.ready.insert(seat, true); // AI is always ready
	}

	fn remove_ai(&mut self, seat: Seat) -> bool {
		if self.ai_players.remove(&seat).is_some() {
			self.ready.remove(&seat);
			true
		} else {
			false
		}
	}

	fn set_ready(&mut self, seat: Seat) {
		self.ready.insert(seat, true);
	}

	fn all_ready(&self) -> bool {
		self.player_count() >= self.config.min_players
			&& self.ready.values().all(|&r| r)
	}

	fn to_info(&self) -> TableInfo {
		let blinds = match (self.config.small_blind, self.config.big_blind) {
			(Some(sb), Some(bb)) => format!("${:.0}/${:.0}", sb, bb),
			_ => "N/A".to_string(),
		};
		let buy_in = self.config.effective_buy_in();
		TableInfo {
			id: self.config.id.clone(),
			name: self.config.name.clone(),
			format: self.config.format.to_string(),
			betting: self.config.betting.to_string(),
			blinds,
			buy_in: format!("${:.0}", buy_in),
			players: self.player_count(),
			max_players: self.config.max_players,
			status: self.status,
			config: self.config.clone(),
		}
	}

	fn player_infos(&self, connections: &HashMap<ConnectionId, Connection>) -> Vec<PlayerInfo> {
		let mut infos: Vec<PlayerInfo> = self.players.iter().map(|(&seat, &conn_id)| {
			let username = connections.get(&conn_id)
				.and_then(|c| c.username.clone())
				.unwrap_or_else(|| "Unknown".to_string());
			let ready = self.ready.get(&seat).copied().unwrap_or(false);
			PlayerInfo { seat, username, ready, is_ai: false }
		}).collect();

		for (&seat, ai) in &self.ai_players {
			let ready = self.ready.get(&seat).copied().unwrap_or(true);
			infos.push(PlayerInfo {
				seat,
				username: ai.name.clone(),
				ready,
				is_ai: true,
			});
		}

		infos.sort_by_key(|p| p.seat.0);
		infos
	}

	fn has_username(&self, username: &str, connections: &HashMap<ConnectionId, Connection>) -> bool {
		let username_lower = username.to_lowercase();
		for &conn_id in self.players.values() {
			if let Some(conn) = connections.get(&conn_id) {
				if let Some(ref name) = conn.username {
					if name.to_lowercase() == username_lower {
						return true;
					}
				}
			}
		}
		for ai in self.ai_players.values() {
			if ai.name.to_lowercase() == username_lower {
				return true;
			}
		}
		false
	}
}

pub struct GameServer {
	connections: Arc<Mutex<HashMap<ConnectionId, Connection>>>,
	tables: Arc<Mutex<HashMap<String, TableRoom>>>,
	next_conn_id: Arc<Mutex<ConnectionId>>,
	ai_roster: Arc<Vec<PlayerConfig>>,
	bank: Arc<Mutex<Bank>>,
}

impl GameServer {
	pub fn new() -> Self {
		let tables_config = load_tables().unwrap_or_default();
		let mut tables = HashMap::new();
		for (order, config) in tables_config.into_iter().enumerate() {
			tables.insert(config.id.clone(), TableRoom::new(config, order));
		}

		let ai_roster = load_players_auto().unwrap_or_default();
		let bank = Bank::load().expect("Failed to load bank - ensure config directory exists");

		Self {
			connections: Arc::new(Mutex::new(HashMap::new())),
			tables: Arc::new(Mutex::new(tables)),
			next_conn_id: Arc::new(Mutex::new(1)),
			ai_roster: Arc::new(ai_roster),
			bank: Arc::new(Mutex::new(bank)),
		}
	}

	pub fn run(&self, addr: &str) -> std::io::Result<()> {
		let listener = TcpListener::bind(addr)?;
		println!("Poker server listening on {}", addr);
		self.run_with_listener(listener);
		Ok(())
	}

	pub fn run_with_listener(&self, listener: TcpListener) {
		for stream in listener.incoming() {
			match stream {
				Ok(stream) => {
					let conn_id = {
						let mut id = self.next_conn_id.lock().unwrap_or_else(|e| e.into_inner());
						let current = *id;
						*id += 1;
						current
					};

					let connections = Arc::clone(&self.connections);
					let tables = Arc::clone(&self.tables);
					let ai_roster = Arc::clone(&self.ai_roster);
					let bank = Arc::clone(&self.bank);

					thread::spawn(move || {
						handle_connection(conn_id, stream, connections, tables, ai_roster, bank);
					});
				}
				Err(e) => {
					eprintln!("Connection failed: {}", e);
				}
			}
		}
	}
}

fn handle_connection(
	conn_id: ConnectionId,
	stream: TcpStream,
	connections: Arc<Mutex<HashMap<ConnectionId, Connection>>>,
	tables: Arc<Mutex<HashMap<String, TableRoom>>>,
	ai_roster: Arc<Vec<PlayerConfig>>,
	bank: Arc<Mutex<Bank>>,
) {
	let stream_clone = match stream.try_clone() {
		Ok(s) => s,
		Err(e) => {
			eprintln!("Failed to clone stream for client {}: {}", conn_id, e);
			return;
		}
	};
	let conn = Connection {
		username: None,
		stream: stream_clone,
		current_table: None,
	};

	lock_connections(&connections).insert(conn_id, conn);
	println!("Client {} connected", conn_id);

	let mut reader = stream;
	let mut buf = vec![0u8; 4096];
	let mut pending = Vec::new();

	loop {
		match reader.read(&mut buf) {
			Ok(0) => break,
			Ok(n) => {
				pending.extend_from_slice(&buf[..n]);
				while let Some(msg) = try_decode_message(&mut pending) {
					process_message(conn_id, msg, &connections, &tables, &ai_roster, &bank);
				}
			}
			Err(_) => break,
		}
	}

	// Cleanup on disconnect - lock order: tables first for lookups, connections for removal
	let table_id = {
		let mut conns = lock_connections(&connections);
		let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
		conns.remove(&conn_id);
		table_id
	};

	if let Some(tid) = table_id {
		let (removed_seat, has_active_game) = {
			let mut tables_lock = lock_tables(&tables);
			if let Some(table) = tables_lock.get_mut(&tid) {
				let seat = table.remove_player(conn_id);
				let no_humans = table.players.is_empty();
				let has_active = table.active_game.is_some();

				if no_humans && table.status == TableStatus::Waiting {
					table.ai_players.clear();
					table.ready.clear();
				}

				if let Some(ref mut active_game) = table.active_game {
					active_game.remove_player(conn_id);
					if !active_game.has_humans() {
						active_game.signal_quit();
					}
				}

				(seat, has_active)
			} else {
				(None, false)
			}
		};

		if let Some(seat) = removed_seat {
			// Lock order: tables first, then connections
			let mut tables_lock = lock_tables(&tables);
			let mut conns = lock_connections(&connections);

			let username = "Disconnected".to_string();
			let msg = ServerMessage::PlayerLeftTable { seat, username };
			broadcast_to_table(&tid, &msg, &mut tables_lock, &mut conns);

			if has_active_game {
				let game_event = ServerMessage::GameEvent(GameEvent::PlayerLeft {
					seat,
					reason: LeaveReason::Disconnected,
				});
				broadcast_to_table(&tid, &game_event, &mut tables_lock, &mut conns);
			}

			let table_list = build_table_list(&tables_lock);
			broadcast_lobby_state(&table_list, &mut conns);
		}
	}

	println!("Client {} disconnected", conn_id);
}

const MAX_MESSAGE_SIZE: usize = 65536;
const MAX_USERNAME_LENGTH: usize = 32;
const MAX_TABLE_ID_LENGTH: usize = 64;
const MAX_CHAT_LENGTH: usize = 500;

fn try_decode_message(buf: &mut Vec<u8>) -> Option<ClientMessage> {
	if buf.len() < 4 {
		return None;
	}
	let len = decode_length(buf)? as usize;
	if len > MAX_MESSAGE_SIZE {
		buf.clear();
		return None;
	}
	if buf.len() < 4 + len {
		return None;
	}
	let json = String::from_utf8_lossy(&buf[4..4 + len]).to_string();
	buf.drain(..4 + len);
	serde_json::from_str(&json).ok()
}

fn process_message(
	conn_id: ConnectionId,
	msg: ClientMessage,
	connections: &Arc<Mutex<HashMap<ConnectionId, Connection>>>,
	tables: &Arc<Mutex<HashMap<String, TableRoom>>>,
	ai_roster: &Arc<Vec<PlayerConfig>>,
	bank: &Arc<Mutex<Bank>>,
) {
	match msg {
		ClientMessage::Login { username } => {
			if username.len() > MAX_USERNAME_LENGTH || username.is_empty() {
				let mut conns = lock_connections(&connections);
				if let Some(conn) = conns.get_mut(&conn_id) {
					conn.send(&ServerMessage::Error {
						message: format!("Username must be 1-{} characters", MAX_USERNAME_LENGTH),
					});
				}
				return;
			}
			let mut conns = lock_connections(&connections);
			if let Some(conn) = conns.get_mut(&conn_id) {
				conn.username = Some(username.clone());
				let bankroll = {
					let bank_lock = lock_bank(&bank);
					bank_lock.get_bankroll(&username.to_lowercase())
				};
				conn.send(&ServerMessage::Welcome {
					username: username.clone(),
					message: "Welcome to the poker server!".to_string(),
					bankroll,
				});
			}
		}

		ClientMessage::ListTables => {
			// Lock tables first, do cleanup, then get connections
			let any_cleaned = {
				let mut tables_lock = lock_tables(&tables);
				cleanup_finished_games(&mut tables_lock)
			};

			let tables_lock = lock_tables(&tables);
			let table_list = build_table_list(&tables_lock);
			drop(tables_lock);

			let mut conns = lock_connections(&connections);
			if any_cleaned {
				broadcast_lobby_state(&table_list, &mut conns);
			} else {
				if let Some(conn) = conns.get_mut(&conn_id) {
					conn.send(&ServerMessage::LobbyState { tables: table_list });
				}
			}
		}

		ClientMessage::JoinTable { table_id } => {
			if table_id.len() > MAX_TABLE_ID_LENGTH {
				let mut conns = lock_connections(&connections);
				if let Some(conn) = conns.get_mut(&conn_id) {
					conn.send(&ServerMessage::Error {
						message: "Invalid table ID".to_string(),
					});
				}
				return;
			}
			// Lock order: tables first, then connections
			let mut tables_lock = lock_tables(&tables);
			let mut conns = lock_connections(&connections);

			let username = conns.get(&conn_id)
				.and_then(|c| c.username.clone())
				.unwrap_or_else(|| "Anonymous".to_string());

			// Cleanup finished game if applicable
			let mut cleaned_up = false;
			if let Some(table) = tables_lock.get_mut(&table_id) {
				if table.status == TableStatus::InProgress {
					if let Some(ref game) = table.active_game {
						if game.is_finished() {
							table.status = TableStatus::Waiting;
							table.players.clear();
							table.ai_players.clear();
							table.ready.clear();
							table.active_game = None;
							cleaned_up = true;
						}
					}
				}
			}

			if cleaned_up {
				// Broadcast updated status to all lobby clients
				let table_list = build_table_list(&tables_lock);
				broadcast_lobby_state(&table_list, &mut conns);
			}

			if let Some(table) = tables_lock.get_mut(&table_id) {
				if table.status != TableStatus::Waiting {
					if let Some(conn) = conns.get_mut(&conn_id) {
						conn.send(&ServerMessage::Error {
							message: "Table is not accepting players".to_string(),
						});
					}
					return;
				}

				if table.has_username(&username, &conns) {
					if let Some(conn) = conns.get_mut(&conn_id) {
						conn.send(&ServerMessage::Error {
							message: format!("Player '{}' is already at this table", username),
						});
					}
					return;
				}

				if let Some(seat) = table.find_empty_seat() {
					table.add_player(seat, conn_id);
					let player_infos = table.player_infos(&conns);
					let table_name = table.config.name.clone();
					let min_players = table.config.min_players;
					let max_players = table.config.max_players;

					if let Some(conn) = conns.get_mut(&conn_id) {
						conn.current_table = Some(table_id.clone());
						conn.send(&ServerMessage::TableJoined {
							table_id: table_id.clone(),
							table_name,
							seat,
							players: player_infos,
							min_players,
							max_players,
						});
					}

					let join_msg = ServerMessage::PlayerJoinedTable {
						seat,
						username,
					};
					broadcast_to_table_except(&table_id, conn_id, &join_msg, &mut tables_lock, &mut conns);

					// Broadcast updated lobby state to all clients in table select
					let table_list = build_table_list(&tables_lock);
					broadcast_lobby_state(&table_list, &mut conns);
				} else {
					if let Some(conn) = conns.get_mut(&conn_id) {
						conn.send(&ServerMessage::Error {
							message: "Table is full".to_string(),
						});
					}
				}
			} else {
				if let Some(conn) = conns.get_mut(&conn_id) {
					conn.send(&ServerMessage::Error {
						message: "Table not found".to_string(),
					});
				}
			}
		}

		ClientMessage::LeaveTable => {
			// Lock order: tables first, then connections
			let mut tables_lock = lock_tables(&tables);
			let mut conns = lock_connections(&connections);

			let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
			if let Some(tid) = table_id {
				let (removed_seat, username, has_active_game) = {
					if let Some(table) = tables_lock.get_mut(&tid) {
						if let Some(seat) = table.remove_player(conn_id) {
							let username = conns.get(&conn_id)
								.and_then(|c| c.username.clone())
								.unwrap_or_else(|| "Unknown".to_string());
							let has_active = table.active_game.is_some();

							// If no humans left and game hasn't started, reset table
							if table.players.is_empty() && table.status == TableStatus::Waiting {
								table.ai_players.clear();
								table.ready.clear();
							}

							// If game is active, update active_game and signal quit if no humans left
							if let Some(ref mut active_game) = table.active_game {
								active_game.remove_player(conn_id);
								if !active_game.has_humans() {
									active_game.signal_quit();
								}
							}

							(Some(seat), username, has_active)
						} else {
							(None, String::new(), false)
						}
					} else {
						(None, String::new(), false)
					}
				};

				if let Some(seat) = removed_seat {
					// Notify the leaving player first
					if let Some(conn) = conns.get_mut(&conn_id) {
						conn.send(&ServerMessage::TableLeft);
					}

					// Then notify others at the table
					let msg = ServerMessage::PlayerLeftTable { seat, username };
					broadcast_to_table(&tid, &msg, &mut tables_lock, &mut conns);

					// Send PlayerLeft event if game is active
					if has_active_game {
						let game_event = ServerMessage::GameEvent(GameEvent::PlayerLeft {
							seat,
							reason: LeaveReason::Quit,
						});
						broadcast_to_table(&tid, &game_event, &mut tables_lock, &mut conns);
					}

					// Broadcast updated lobby state to all connected clients
					let table_list = build_table_list(&tables_lock);
					broadcast_lobby_state(&table_list, &mut conns);
				}

				if let Some(conn) = conns.get_mut(&conn_id) {
					conn.current_table = None;
				}
			}
		}

		ClientMessage::Ready => {
			// Lock order: tables first, then connections, then bank
			let mut tables_lock = lock_tables(&tables);
			let mut conns = lock_connections(&connections);

			let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
			if let Some(tid) = table_id {
				let (ready_seat, all_ready) = {
					if let Some(table) = tables_lock.get_mut(&tid) {
						let seat = table.players.iter()
							.find(|&(_, &id)| id == conn_id)
							.map(|(&s, _)| s);
						if let Some(s) = seat {
							table.set_ready(s);
							(Some(s), table.all_ready())
						} else {
							(None, false)
						}
					} else {
						(None, false)
					}
				};

				if let Some(seat) = ready_seat {
					let msg = ServerMessage::PlayerReady { seat };
					broadcast_to_table(&tid, &msg, &mut tables_lock, &mut conns);

					if all_ready {
						let mut bank_lock = lock_bank(&bank);

						// Process buy-ins for all players
						let buy_in_result: Result<(), String> = (|| {
							let table = tables_lock.get(&tid).ok_or("Table not found")?;
							let buy_in = table.config.effective_buy_in();

							// Collect all player ids (humans use username lowercase, AI uses id)
							let mut player_ids: Vec<String> = Vec::new();

							for &cid in table.players.values() {
								if let Some(conn) = conns.get(&cid) {
									let username = conn.username.clone().unwrap_or_else(|| "Unknown".to_string());
									player_ids.push(username.to_lowercase());
								}
							}

							for ai in table.ai_players.values() {
								player_ids.push(ai.id.clone());
							}

							// Try buy-in for each player
							for id in &player_ids {
								bank_lock.buyin(id, buy_in, &table.config.id)
									.map_err(|e| format!("{}", e))?;
							}

						Ok(())
						})();

						if let Err(msg) = buy_in_result {
							// Reset table status and player ready states
							if let Some(table) = tables_lock.get_mut(&tid) {
								table.status = TableStatus::Waiting;
								for ready in table.ready.values_mut() {
									*ready = false;
								}
								// Re-mark AI as ready
								for &seat in table.ai_players.keys() {
									table.ready.insert(seat, true);
								}
							}

							let error_msg = ServerMessage::Error { message: msg };
							broadcast_to_table(&tid, &error_msg, &mut tables_lock, &mut conns);

							// Broadcast updated lobby state
							let table_list = build_table_list(&tables_lock);
							broadcast_lobby_state(&table_list, &mut conns);
							return;
						}

						// Save bank after successful buy-ins
						if let Err(e) = bank_lock.save() {
							eprintln!("Failed to save bank: {}", e);
						}
						drop(bank_lock);

						if let Some(table) = tables_lock.get_mut(&tid) {
							table.status = TableStatus::InProgress;
						}

						// Send GameStarting with the table config
						let table_config = tables_lock.get(&tid)
							.map(|t| t.config.clone())
							.unwrap_or_else(|| panic!("Table {} not found", tid));

						let starting_msg = ServerMessage::GameStarting {
							countdown: 3,
							table_config,
						};
						broadcast_to_table(&tid, &starting_msg, &mut tables_lock, &mut conns);

						// Broadcast updated lobby state so table select shows "In Progress"
						let table_list = build_table_list(&tables_lock);
						broadcast_lobby_state(&table_list, &mut conns);

						// Collect info for game start
						let game_info: Option<GameStartInfo> = {
							if let Some(table) = tables_lock.get(&tid) {
								let mut human_players = Vec::new();
								let mut player_bank_ids: Vec<(Seat, String)> = Vec::new();

								for (&seat, &cid) in &table.players {
									if let Some(conn) = conns.get(&cid) {
										let username = conn.username.clone().unwrap_or_else(|| "Unknown".to_string());
										player_bank_ids.push((seat, username.to_lowercase()));
										if let Ok(stream_clone) = conn.stream.try_clone() {
											human_players.push((cid, seat, username, stream_clone));
										}
									}
								}

								for (&seat, ai) in &table.ai_players {
									player_bank_ids.push((seat, ai.id.clone()));
								}

								// Sort by seat and extract just the ids
								player_bank_ids.sort_by_key(|(seat, _)| seat.0);
								let bank_ids: Vec<String> = player_bank_ids.into_iter()
									.map(|(_, id)| id)
									.collect();

								let ai_players: Vec<(Seat, String, String, String)> = table.ai_players.iter()
									.map(|(&seat, ai)| (seat, ai.id.clone(), ai.name.clone(), ai.strategy.clone()))
									.collect();

								Some(GameStartInfo {
									config: table.config.clone(),
									human_players,
									ai_players,
									player_bank_ids: bank_ids,
								})
							} else {
								None
							}
						};

						// Start game outside of heavy lock usage
						if let Some(info) = game_info {
							let active_game = start_game(info, Arc::clone(bank));
							if let Some(table) = tables_lock.get_mut(&tid) {
								table.active_game = Some(active_game);
							}
						}
					}
				}
			}
		}

		ClientMessage::AddAI { strategy: _ } => {
			// Lock order: tables first, then connections, then bank
			let mut tables_lock = lock_tables(&tables);
			let mut conns = lock_connections(&connections);

			let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
			if let Some(tid) = table_id {
				if let Some(table) = tables_lock.get_mut(&tid) {
					if table.status != TableStatus::Waiting {
						if let Some(conn) = conns.get_mut(&conn_id) {
							conn.send(&ServerMessage::Error {
								message: "Cannot add AI while game in progress".to_string(),
							});
						}
						return;
					}

					if let Some(seat) = table.find_empty_seat() {
						// Collect all used IDs (humans + AI) case-insensitively
						let mut used_ids: Vec<String> = table.ai_players.values()
							.map(|ai| ai.id.to_lowercase())
							.collect();
						for &cid in table.players.values() {
							if let Some(conn) = conns.get(&cid) {
								if let Some(username) = &conn.username {
									used_ids.push(username.to_lowercase());
								}
							}
						}

						let mut available: Vec<_> = ai_roster.iter()
							.filter(|p| !used_ids.contains(&p.id.to_lowercase()))
							.collect();

						use rand::seq::SliceRandom;
						available.shuffle(&mut rand::rng());

						let selected = available.iter()
							.find(|p| rand::random::<f32>() < p.join_probability)
							.copied()
							.or_else(|| available.first().copied());

						if let Some(ai_config) = selected {
							// Ensure AI player has a bank profile
							{
								let mut bank_lock = lock_bank(&bank);
								bank_lock.ensure_exists(&ai_config.id);
								if let Err(e) = bank_lock.save() {
									eprintln!("Failed to save bank after ensuring AI exists: {}", e);
								}
							}

							let name = ai_config.display_name();
							table.add_ai(seat, ai_config.id.clone(), name.clone(), ai_config.strategy.clone());

							let msg = ServerMessage::AIAdded { seat, name };
							broadcast_to_table(&tid, &msg, &mut tables_lock, &mut conns);

							// Broadcast updated lobby state to all clients in table select
							let table_list = build_table_list(&tables_lock);
							broadcast_lobby_state(&table_list, &mut conns);
						} else {
							if let Some(conn) = conns.get_mut(&conn_id) {
								conn.send(&ServerMessage::Error {
									message: "No available AI players".to_string(),
								});
							}
						}
					} else {
						if let Some(conn) = conns.get_mut(&conn_id) {
							conn.send(&ServerMessage::Error {
								message: "Table is full".to_string(),
							});
						}
					}
				}
			}
		}

		ClientMessage::RemoveAI { seat } => {
			// Lock order: tables first, then connections
			let mut tables_lock = lock_tables(&tables);
			let mut conns = lock_connections(&connections);

			let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
			if let Some(tid) = table_id {
				if let Some(table) = tables_lock.get_mut(&tid) {
					if table.status != TableStatus::Waiting {
						if let Some(conn) = conns.get_mut(&conn_id) {
							conn.send(&ServerMessage::Error {
								message: "Cannot remove AI while game in progress".to_string(),
							});
						}
						return;
					}

					if table.remove_ai(seat) {
						let msg = ServerMessage::AIRemoved { seat };
						broadcast_to_table(&tid, &msg, &mut tables_lock, &mut conns);

						// Broadcast updated lobby state to all clients in table select
						let table_list = build_table_list(&tables_lock);
						broadcast_lobby_state(&table_list, &mut conns);
					} else {
						if let Some(conn) = conns.get_mut(&conn_id) {
							conn.send(&ServerMessage::Error {
								message: "No AI at that seat".to_string(),
							});
						}
					}
				}
			}
		}

		ClientMessage::Action { action } => {
			// Lock order: tables first, then connections
			let tables_lock = lock_tables(&tables);
			let conns = lock_connections(&connections);

			let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
			if let Some(tid) = table_id {
				if let Some(table) = tables_lock.get(&tid) {
					if let Some(ref active_game) = table.active_game {
						if let Err(e) = active_game.submit_action(conn_id, action) {
							eprintln!("Action error: {}", e);
						}
					}
				}
			}
		}

		ClientMessage::Chat { text } => {
			if text.len() > MAX_CHAT_LENGTH {
				return;
			}
			// TODO: Broadcast chat
			println!("Chat from {}: {}", conn_id, text);
		}
	}
}

fn broadcast_to_table(
	table_id: &str,
	msg: &ServerMessage,
	tables: &mut HashMap<String, TableRoom>,
	conns: &mut HashMap<ConnectionId, Connection>,
) {
	if let Some(table) = tables.get(table_id) {
		for &conn_id in table.players.values() {
			if let Some(conn) = conns.get_mut(&conn_id) {
				conn.send(msg);
			}
		}
	}
}

fn broadcast_to_table_except(
	table_id: &str,
	exclude: ConnectionId,
	msg: &ServerMessage,
	tables: &mut HashMap<String, TableRoom>,
	conns: &mut HashMap<ConnectionId, Connection>,
) {
	if let Some(table) = tables.get(table_id) {
		for &conn_id in table.players.values() {
			if conn_id != exclude {
				if let Some(conn) = conns.get_mut(&conn_id) {
					conn.send(msg);
				}
			}
		}
	}
}

fn cleanup_finished_games(tables: &mut HashMap<String, TableRoom>) -> bool {
	let mut any_cleaned = false;
	for table in tables.values_mut() {
		if table.status == TableStatus::InProgress {
			if let Some(ref game) = table.active_game {
				if game.is_finished() {
					table.status = TableStatus::Waiting;
					table.players.clear();
					table.ai_players.clear();
					table.ready.clear();
					table.active_game = None;
					any_cleaned = true;
				}
			}
		}
	}
	any_cleaned
}

fn build_table_list(tables: &HashMap<String, TableRoom>) -> Vec<TableInfo> {
	let mut table_list: Vec<(usize, TableInfo)> = tables.values()
		.map(|t| (t.order, t.to_info()))
		.collect();
	table_list.sort_by_key(|(order, _)| *order);
	table_list.into_iter().map(|(_, info)| info).collect()
}

fn broadcast_lobby_state(table_list: &[TableInfo], conns: &mut HashMap<ConnectionId, Connection>) {
	let msg = ServerMessage::LobbyState { tables: table_list.to_vec() };
	for conn in conns.values_mut() {
		if conn.current_table.is_none() {
			conn.send(&msg);
		}
	}
}

fn start_game(info: GameStartInfo, bank: Arc<Mutex<Bank>>) -> ActiveGame {
	let runtime = tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.build()
		.expect("Failed to create tokio runtime for game");
	let runtime_handle = runtime.handle().clone();

	let runner_config = build_runner_config(&info.config);
	let (mut runner, game_handle) = GameRunner::new(runner_config, runtime_handle.clone());

	let game_finished = Arc::new(AtomicBool::new(false));
	let mut active_game = ActiveGame::new(
		Arc::clone(&game_handle.sitting_out),
		Arc::clone(&game_finished),
		Arc::clone(&game_handle.quit_signal),
	);

	// Load strategies for AI players
	let strategies = load_strategies_auto().unwrap_or_default();
	let big_blind = info.config.big_blind.unwrap_or(2.0);

	// Capture config for game end processing
	let game_format = info.config.format;
	let table_id = info.config.id.clone();
	let player_bank_ids = info.player_bank_ids.clone();
	let payouts_config = info.config.payouts.clone();
	let buy_in = info.config.buy_in;

	// Capture delays from config
	let action_delay_ms = info.config.action_delay_ms;
	let street_delay_ms = info.config.street_delay_ms;
	let hand_end_delay_ms = info.config.hand_end_delay_ms;

	// Combine all players and sort by seat for consistent ordering
	enum PlayerSlot {
		Human { conn_id: ConnectionId, name: String, stream: TcpStream },
		AI { name: String, strategy: String },
	}

	let mut all_players: Vec<(Seat, PlayerSlot)> = Vec::new();

	for (conn_id, seat, username, stream) in info.human_players {
		all_players.push((seat, PlayerSlot::Human { conn_id, name: username, stream }));
	}

	for (seat, _id, name, strategy) in info.ai_players {
		all_players.push((seat, PlayerSlot::AI { name, strategy }));
	}

	all_players.sort_by_key(|(seat, _)| seat.0);

	// Collect streams for event forwarding (human players only)
	let mut player_streams: Vec<(Seat, Arc<Mutex<TcpStream>>)> = Vec::new();

	for (idx, (_lobby_seat, slot)) in all_players.into_iter().enumerate() {
		let game_seat = Seat(idx);

		match slot {
			PlayerSlot::Human { conn_id, name, stream } => {
				if let Ok(stream_for_events) = stream.try_clone() {
					player_streams.push((game_seat, Arc::new(Mutex::new(stream_for_events))));
				}

				let (action_tx, action_rx) = mpsc::channel();
				active_game.register_player(conn_id, game_seat, action_tx);

				let player = RemotePlayer::new(game_seat, name, action_rx);
				runner.add_player(Arc::new(player));
			}
			PlayerSlot::AI { name, strategy } => {
				let strat = strategies.get_or_default(&strategy);
				let player = RulesPlayer::new(game_seat, &name, strat, big_blind);
				runner.add_player(Arc::new(player));
			}
		}
	}

	thread::spawn(move || {
		let _rt_guard = runtime.enter();
		runner.run();
	});

	// Forward events to all players with filtering and pacing
	let game_finished_clone = Arc::clone(&game_finished);
	let sitting_out = Arc::clone(&game_handle.sitting_out);
	thread::spawn(move || {
		while let Ok(event) = game_handle.event_rx.recv() {
			// Clone the set so we don't hold the lock during I/O
			let disconnected = sitting_out.lock()
				.unwrap_or_else(|e| e.into_inner())
				.clone();

			for (seat, stream) in &player_streams {
				// Skip players who have disconnected
				if disconnected.contains(seat) {
					continue;
				}
				let filtered = filter_event_for_seat(&event, *seat);
				let msg = ServerMessage::GameEvent(filtered);
				let data = encode_message(&msg);
				if let Ok(mut s) = stream.lock() {
					if let Err(e) = s.write_all(&data) {
						// BrokenPipe is expected when a player disconnects - don't spam logs
						if e.kind() != std::io::ErrorKind::BrokenPipe {
							eprintln!("Failed to send event to seat {}: {}", seat.0, e);
						}
					}

					// Send ActionRequest message to the acting player
					if let GameEvent::ActionRequest { seat: action_seat, valid_actions, .. } = &event {
						if action_seat == seat {
							let action_msg = ServerMessage::ActionRequest {
								valid_actions: valid_actions.clone(),
								time_limit: Some(120),
							};
							let action_data = encode_message(&action_msg);
							if let Err(e) = s.write_all(&action_data) {
								if e.kind() != std::io::ErrorKind::BrokenPipe {
									eprintln!("Failed to send action request to seat {}: {}", seat.0, e);
								}
							}
						}
					}
				}
			}

			// Handle game end - process cashout/prizes
			if let GameEvent::GameEnded { final_standings, .. } = &event {
				use crate::table::GameFormat;

				let mut bank_lock = bank.lock().unwrap_or_else(|e| e.into_inner());

				match game_format {
					GameFormat::Cash => {
						// Cash game: return remaining stacks to players
						for standing in final_standings {
							if let Some(bank_id) = player_bank_ids.get(standing.seat.0) {
								bank_lock.cashout(bank_id, standing.final_stack, &table_id);
							}
						}
					}
					GameFormat::SitNGo => {
						// Tournament: distribute prizes based on finish position
						if let (Some(payout_pcts), Some(bi)) = (&payouts_config, buy_in) {
							let num_players = final_standings.len();
							let payouts = crate::table::calculate_payouts(bi, num_players, payout_pcts);
							for (i, payout) in payouts.iter().enumerate() {
								if let Some(standing) = final_standings.iter().find(|s| s.finish_position == (i + 1) as u8) {
									if let Some(bank_id) = player_bank_ids.get(standing.seat.0) {
										bank_lock.award_prize(bank_id, *payout, i + 1);
									}
								}
							}
						}
					}
				}

				if let Err(e) = bank_lock.save() {
					eprintln!("Failed to save bank after game end: {}", e);
				}

				// Signal that the game has finished
				game_finished_clone.store(true, Ordering::SeqCst);
			}

			// Use delays from table config
			let delay_ms = match &event {
				GameEvent::ActionTaken { .. } => action_delay_ms,
				GameEvent::StreetChanged { .. } => street_delay_ms,
				GameEvent::ShowdownReveal { .. } => 500,
				GameEvent::HandEnded { .. } => hand_end_delay_ms,
				GameEvent::PotAwarded { .. } => 1500,
				_ => 0,
			};

			if delay_ms > 0 {
				thread::sleep(std::time::Duration::from_millis(delay_ms));
			}
		}
	});

	active_game
}

fn filter_event_for_seat(event: &GameEvent, seat: Seat) -> GameEvent {
	match event {
		GameEvent::HoleCardsDealt { seat: dealt_seat, cards: _ } => {
			if *dealt_seat == seat {
				event.clone()
			} else {
				GameEvent::HoleCardsDealt {
					seat: *dealt_seat,
					cards: [
						Card { rank: '?', suit: '?' },
						Card { rank: '?', suit: '?' },
					],
				}
			}
		}
		_ => event.clone(),
	}
}

fn build_runner_config(table: &TableConfig) -> RunnerConfig {
	let (small_blind, big_blind) = table.current_blinds();
	let starting_stack = table.effective_starting_stack();

	RunnerConfig {
		small_blind,
		big_blind,
		starting_stack,
		betting_structure: match table.betting {
			crate::table::BettingStructure::NoLimit => BettingStructure::NoLimit,
			crate::table::BettingStructure::PotLimit => BettingStructure::PotLimit,
			crate::table::BettingStructure::FixedLimit => BettingStructure::FixedLimit,
		},
		blind_clock: None,
		max_raises_per_round: table.max_raises_per_round,
		rake_percent: table.rake_percent,
		rake_cap: table.rake_cap,
		no_flop_no_drop: table.no_flop_no_drop,
		max_hands: None,
		seed: table.seed,
	}
}
