use serde::{Deserialize, Serialize};
use crate::events::{GameEvent, PlayerAction, Seat, ValidActions};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
	Login {
		username: String,
	},
	ListTables,
	JoinTable {
		table_id: String,
	},
	LeaveTable,
	Ready,
	AddAI {
		strategy: Option<String>,
	},
	RemoveAI {
		seat: Seat,
	},
	Action {
		#[serde(flatten)]
		action: PlayerAction,
	},
	Chat {
		text: String,
	},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
	Welcome {
		username: String,
		message: String,
	},
	Error {
		message: String,
	},
	LobbyState {
		tables: Vec<TableInfo>,
	},
	TableJoined {
		table_id: String,
		table_name: String,
		seat: Seat,
		players: Vec<PlayerInfo>,
		min_players: usize,
		max_players: usize,
	},
	PlayerJoinedTable {
		seat: Seat,
		username: String,
	},
	PlayerLeftTable {
		seat: Seat,
		username: String,
	},
	PlayerReady {
		seat: Seat,
	},
	AIAdded {
		seat: Seat,
		name: String,
	},
	AIRemoved {
		seat: Seat,
	},
	GameStarting {
		countdown: u32,
	},
	GameEvent(GameEvent),
	ActionRequest {
		valid_actions: ValidActions,
		time_limit: Option<u32>,
	},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
	pub id: String,
	pub name: String,
	pub format: String,
	pub betting: String,
	pub blinds: String,
	pub players: usize,
	pub max_players: usize,
	pub status: TableStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableStatus {
	Waiting,
	InProgress,
	Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
	pub seat: Seat,
	pub username: String,
	pub ready: bool,
	#[serde(default)]
	pub is_ai: bool,
}

pub fn encode_message<T: Serialize>(msg: &T) -> Vec<u8> {
	let json = serde_json::to_string(msg).unwrap();
	let len = json.len() as u32;
	let mut buf = len.to_be_bytes().to_vec();
	buf.extend(json.as_bytes());
	buf
}

pub fn decode_length(buf: &[u8]) -> Option<u32> {
	if buf.len() < 4 {
		return None;
	}
	Some(u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]))
}
