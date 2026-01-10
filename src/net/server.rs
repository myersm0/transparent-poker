use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use tokio::sync::oneshot;

use crate::engine::{BettingStructure, GameRunner, RunnerConfig};
use crate::events::{Card, GameEvent, PlayerAction, Seat};
use crate::net::network_player::{submit_action, NetworkPlayer};
use crate::net::protocol::*;
use crate::players::PlayerResponse;
use crate::table::{load_tables, TableConfig};

type ConnectionId = u64;
type PendingActionSender = Arc<Mutex<Option<oneshot::Sender<PlayerResponse>>>>;

struct Connection {
	id: ConnectionId,
	username: Option<String>,
	stream: TcpStream,
	current_table: Option<String>,
}

impl Connection {
	fn send(&mut self, msg: &ServerMessage) {
		let data = encode_message(msg);
		let _ = self.stream.write_all(&data);
	}
}

struct ActiveGame {
	pending_actions: HashMap<Seat, PendingActionSender>,
	conn_to_seat: HashMap<ConnectionId, Seat>,
}

impl ActiveGame {
	fn new() -> Self {
		Self {
			pending_actions: HashMap::new(),
			conn_to_seat: HashMap::new(),
		}
	}

	fn register_player(&mut self, conn_id: ConnectionId, seat: Seat, pending: PendingActionSender) {
		self.pending_actions.insert(seat, pending);
		self.conn_to_seat.insert(conn_id, seat);
	}

	fn submit_action(&self, conn_id: ConnectionId, action: PlayerAction) -> Result<(), String> {
		let seat = self.conn_to_seat.get(&conn_id).ok_or("Player not in game")?;
		let pending = self.pending_actions.get(seat).ok_or("No pending action for seat")?;
		submit_action(pending, action)
	}
}

struct TableRoom {
	config: TableConfig,
	players: HashMap<Seat, ConnectionId>,
	ready: HashMap<Seat, bool>,
	status: TableStatus,
	active_game: Option<ActiveGame>,
}

impl TableRoom {
	fn new(config: TableConfig) -> Self {
		Self {
			config,
			players: HashMap::new(),
			ready: HashMap::new(),
			status: TableStatus::Waiting,
			active_game: None,
		}
	}

	fn player_count(&self) -> usize {
		self.players.len()
	}

	fn find_empty_seat(&self) -> Option<Seat> {
		for i in 0..self.config.max_players {
			let seat = Seat(i);
			if !self.players.contains_key(&seat) {
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
		}
		seat
	}

	fn set_ready(&mut self, seat: Seat) {
		self.ready.insert(seat, true);
	}

	fn all_ready(&self) -> bool {
		self.players.len() >= self.config.min_players
			&& self.ready.values().all(|&r| r)
	}

	fn to_info(&self) -> TableInfo {
		let blinds = match (self.config.small_blind, self.config.big_blind) {
			(Some(sb), Some(bb)) => format!("${:.0}/${:.0}", sb, bb),
			_ => "N/A".to_string(),
		};
		TableInfo {
			id: self.config.id.clone(),
			name: self.config.name.clone(),
			format: self.config.format.to_string(),
			betting: self.config.betting.to_string(),
			blinds,
			players: self.player_count(),
			max_players: self.config.max_players,
			status: self.status,
		}
	}

	fn player_infos(&self, connections: &HashMap<ConnectionId, Connection>) -> Vec<PlayerInfo> {
		self.players.iter().map(|(&seat, &conn_id)| {
			let username = connections.get(&conn_id)
				.and_then(|c| c.username.clone())
				.unwrap_or_else(|| "Unknown".to_string());
			let ready = self.ready.get(&seat).copied().unwrap_or(false);
			PlayerInfo { seat, username, ready }
		}).collect()
	}
}

pub struct GameServer {
	connections: Arc<Mutex<HashMap<ConnectionId, Connection>>>,
	tables: Arc<Mutex<HashMap<String, TableRoom>>>,
	next_conn_id: Arc<Mutex<ConnectionId>>,
}

impl GameServer {
	pub fn new() -> Self {
		let tables_config = load_tables().unwrap_or_default();
		let mut tables = HashMap::new();
		for config in tables_config {
			tables.insert(config.id.clone(), TableRoom::new(config));
		}

		Self {
			connections: Arc::new(Mutex::new(HashMap::new())),
			tables: Arc::new(Mutex::new(tables)),
			next_conn_id: Arc::new(Mutex::new(1)),
		}
	}

	pub fn run(&self, addr: &str) -> std::io::Result<()> {
		let listener = TcpListener::bind(addr)?;
		println!("Poker server listening on {}", addr);

		for stream in listener.incoming() {
			match stream {
				Ok(stream) => {
					let conn_id = {
						let mut id = self.next_conn_id.lock().unwrap();
						let current = *id;
						*id += 1;
						current
					};

					let connections = Arc::clone(&self.connections);
					let tables = Arc::clone(&self.tables);

					thread::spawn(move || {
						handle_connection(conn_id, stream, connections, tables);
					});
				}
				Err(e) => {
					eprintln!("Connection failed: {}", e);
				}
			}
		}
		Ok(())
	}
}

fn handle_connection(
	conn_id: ConnectionId,
	stream: TcpStream,
	connections: Arc<Mutex<HashMap<ConnectionId, Connection>>>,
	tables: Arc<Mutex<HashMap<String, TableRoom>>>,
) {
	let stream_clone = stream.try_clone().unwrap();
	let conn = Connection {
		id: conn_id,
		username: None,
		stream: stream_clone,
		current_table: None,
	};

	connections.lock().unwrap().insert(conn_id, conn);
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
					process_message(conn_id, msg, &connections, &tables);
				}
			}
			Err(_) => break,
		}
	}

	// Cleanup on disconnect
	let table_id = {
		let mut conns = connections.lock().unwrap();
		let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
		conns.remove(&conn_id);
		table_id
	};

	if let Some(tid) = table_id {
		let mut tables = tables.lock().unwrap();
		if let Some(table) = tables.get_mut(&tid) {
			if let Some(seat) = table.remove_player(conn_id) {
				let username = "Disconnected".to_string();
				let msg = ServerMessage::PlayerLeftTable { seat, username };
				broadcast_to_table(&tid, &msg, &mut tables, &mut connections.lock().unwrap());
			}
		}
	}

	println!("Client {} disconnected", conn_id);
}

fn try_decode_message(buf: &mut Vec<u8>) -> Option<ClientMessage> {
	if buf.len() < 4 {
		return None;
	}
	let len = decode_length(buf)? as usize;
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
) {
	match msg {
		ClientMessage::Login { username } => {
			let mut conns = connections.lock().unwrap();
			if let Some(conn) = conns.get_mut(&conn_id) {
				conn.username = Some(username.clone());
				conn.send(&ServerMessage::Welcome {
					username: username.clone(),
					message: "Welcome to the poker server!".to_string(),
				});
			}
		}

		ClientMessage::ListTables => {
			let tables_lock = tables.lock().unwrap();
			let table_list: Vec<TableInfo> = tables_lock.values()
				.map(|t| t.to_info())
				.collect();
			let mut conns = connections.lock().unwrap();
			if let Some(conn) = conns.get_mut(&conn_id) {
				conn.send(&ServerMessage::LobbyState { tables: table_list });
			}
		}

		ClientMessage::JoinTable { table_id } => {
			let mut conns = connections.lock().unwrap();
			let mut tables_lock = tables.lock().unwrap();

			let username = conns.get(&conn_id)
				.and_then(|c| c.username.clone())
				.unwrap_or_else(|| "Anonymous".to_string());

			if let Some(table) = tables_lock.get_mut(&table_id) {
				if table.status != TableStatus::Waiting {
					if let Some(conn) = conns.get_mut(&conn_id) {
						conn.send(&ServerMessage::Error {
							message: "Table is not accepting players".to_string(),
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
			let mut conns = connections.lock().unwrap();
			let mut tables_lock = tables.lock().unwrap();

			let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
			if let Some(tid) = table_id {
				if let Some(table) = tables_lock.get_mut(&tid) {
					if let Some(seat) = table.remove_player(conn_id) {
						let username = conns.get(&conn_id)
							.and_then(|c| c.username.clone())
							.unwrap_or_else(|| "Unknown".to_string());

						let msg = ServerMessage::PlayerLeftTable { seat, username };
						broadcast_to_table(&tid, &msg, &mut tables_lock, &mut conns);
					}
				}
				if let Some(conn) = conns.get_mut(&conn_id) {
					conn.current_table = None;
				}
			}
		}

		ClientMessage::Ready => {
			let mut conns = connections.lock().unwrap();
			let mut tables_lock = tables.lock().unwrap();

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
						if let Some(table) = tables_lock.get_mut(&tid) {
							table.status = TableStatus::InProgress;
						}
						let starting_msg = ServerMessage::GameStarting { countdown: 3 };
						broadcast_to_table(&tid, &starting_msg, &mut tables_lock, &mut conns);

						// Collect info for game start
						let game_info: Option<(TableConfig, Vec<(ConnectionId, Seat, String, TcpStream)>)> = {
							if let Some(table) = tables_lock.get(&tid) {
								let mut player_data = Vec::new();
								for (&seat, &cid) in &table.players {
									if let Some(conn) = conns.get(&cid) {
										let username = conn.username.clone().unwrap_or_else(|| "Unknown".to_string());
										if let Ok(stream_clone) = conn.stream.try_clone() {
											player_data.push((cid, seat, username, stream_clone));
										}
									}
								}
								Some((table.config.clone(), player_data))
							} else {
								None
							}
						};

						// Start game outside of heavy lock usage
						if let Some((config, player_data)) = game_info {
							let active_game = start_game(config, player_data);
							if let Some(table) = tables_lock.get_mut(&tid) {
								table.active_game = Some(active_game);
							}
						}
					}
				}
			}
		}

		ClientMessage::Action { action } => {
			let conns = connections.lock().unwrap();
			let tables_lock = tables.lock().unwrap();

			let table_id = conns.get(&conn_id).and_then(|c| c.current_table.clone());
			if let Some(tid) = table_id {
				if let Some(table) = tables_lock.get(&tid) {
					if let Some(ref active_game) = table.active_game {
						if let Err(e) = active_game.submit_action(conn_id, action) {
							println!("Action error: {}", e);
						}
					}
				}
			}
		}

		ClientMessage::Chat { text } => {
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

fn start_game(
	config: TableConfig,
	player_data: Vec<(ConnectionId, Seat, String, TcpStream)>,
) -> ActiveGame {
	let mut active_game = ActiveGame::new();

	let runtime = tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.build()
		.unwrap();
	let runtime_handle = runtime.handle().clone();

	let runner_config = build_runner_config(&config);
	let (mut runner, game_handle) = GameRunner::new(runner_config, runtime_handle);

	// Collect streams for event forwarding
	let mut player_streams: Vec<(Seat, Arc<Mutex<TcpStream>>)> = Vec::new();

	for (conn_id, seat, username, stream) in player_data {
		// Clone stream for event forwarding
		if let Ok(stream_for_events) = stream.try_clone() {
			player_streams.push((seat, Arc::new(Mutex::new(stream_for_events))));
		}

		let player = NetworkPlayer::new(seat, username, stream);
		let pending = player.pending_action_sender();
		active_game.register_player(conn_id, seat, pending);
		runner.add_player(Arc::new(player));
	}

	thread::spawn(move || {
		let _rt_guard = runtime.enter();
		runner.run();
		println!("Game ended");
	});

	// Forward events to all players with filtering and pacing
	thread::spawn(move || {
		while let Ok(event) = game_handle.event_rx.recv() {
			// Add delay after certain events
			let delay_ms = match &event {
				GameEvent::ActionTaken { .. } => 300,
				GameEvent::StreetChanged { .. } => 800,
				GameEvent::HandEnded { .. } => 3000,
				GameEvent::PotAwarded { .. } => 1000,
				_ => 0,
			};

			for (seat, stream) in &player_streams {
				let filtered = filter_event_for_seat(&event, *seat);
				let msg = ServerMessage::GameEvent(filtered);
				let data = encode_message(&msg);
				if let Ok(mut s) = stream.lock() {
					let _ = s.write_all(&data);
				}
			}

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
