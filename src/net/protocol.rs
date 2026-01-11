use serde::{Deserialize, Serialize};
use crate::events::{GameEvent, PlayerAction, Seat, ValidActions};
use crate::table::TableConfig;

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
		bankroll: f32,
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
	TableLeft,
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
		table_config: TableConfig,
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
	pub buy_in: String,
	pub players: usize,
	pub max_players: usize,
	pub status: TableStatus,
	pub config: TableConfig,
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_encode_decode_length() {
		let msg = ClientMessage::Login { username: "test".to_string() };
		let encoded = encode_message(&msg);

		let len = decode_length(&encoded).expect("Should decode length");
		assert_eq!(len as usize, encoded.len() - 4);
	}

	#[test]
	fn test_encode_client_message_login() {
		let msg = ClientMessage::Login { username: "Alice".to_string() };
		let encoded = encode_message(&msg);
		let json = std::str::from_utf8(&encoded[4..]).unwrap();

		assert!(json.contains("login"));
		assert!(json.contains("Alice"));
	}

	#[test]
	fn test_encode_client_message_action() {
		let msg = ClientMessage::Action {
			action: PlayerAction::Raise { amount: 100.0 },
		};
		let encoded = encode_message(&msg);
		let json = std::str::from_utf8(&encoded[4..]).unwrap();

		assert!(json.contains("action"));
		assert!(json.contains("Raise")); // PlayerAction variants are capitalized
		assert!(json.contains("100"));
	}

	#[test]
	fn test_encode_server_message_welcome() {
		let msg = ServerMessage::Welcome {
			username: "Bob".to_string(),
			message: "Hello".to_string(),
			bankroll: 1000.0,
		};
		let encoded = encode_message(&msg);
		let json = std::str::from_utf8(&encoded[4..]).unwrap();

		assert!(json.contains("welcome"));
		assert!(json.contains("Bob"));
		assert!(json.contains("Hello"));
	}

	#[test]
	fn test_encode_server_message_table_joined() {
		let msg = ServerMessage::TableJoined {
			table_id: "table1".to_string(),
			table_name: "Test Table".to_string(),
			seat: Seat(2),
			players: vec![
				PlayerInfo {
					seat: Seat(0),
					username: "Alice".to_string(),
					ready: false,
					is_ai: false,
				},
			],
			min_players: 2,
			max_players: 6,
		};
		let encoded = encode_message(&msg);
		let json = std::str::from_utf8(&encoded[4..]).unwrap();

		assert!(json.contains("table_joined"));
		assert!(json.contains("table1"));
		assert!(json.contains("Alice"));
	}

	#[test]
	fn test_roundtrip_client_message() {
		let original = ClientMessage::JoinTable {
			table_id: "test-table".to_string(),
		};
		let encoded = encode_message(&original);
		let json = std::str::from_utf8(&encoded[4..]).unwrap();
		let decoded: ClientMessage = serde_json::from_str(json).unwrap();

		match decoded {
			ClientMessage::JoinTable { table_id } => {
				assert_eq!(table_id, "test-table");
			}
			_ => panic!("Wrong message type"),
		}
	}

	#[test]
	fn test_roundtrip_server_message() {
		use crate::table::{BettingStructure, GameFormat};

		let table_config = TableConfig {
			id: "test".to_string(),
			name: "Test Table".to_string(),
			format: GameFormat::Cash,
			betting: BettingStructure::NoLimit,
			small_blind: Some(1.0),
			big_blind: Some(2.0),
			min_buy_in: Some(40.0),
			max_buy_in: Some(200.0),
			buy_in: None,
			starting_stack: None,
			min_players: 2,
			max_players: 6,
			max_raises_per_round: 4,
			payouts: None,
			blind_levels: None,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			action_delay_ms: 500,
			street_delay_ms: 700,
			hand_end_delay_ms: 2000,
			seed: None,
		};

		let original = ServerMessage::GameStarting {
			countdown: 5,
			table_config: table_config.clone(),
		};
		let encoded = encode_message(&original);
		let json = std::str::from_utf8(&encoded[4..]).unwrap();
		let decoded: ServerMessage = serde_json::from_str(json).unwrap();

		match decoded {
			ServerMessage::GameStarting { countdown, table_config: config } => {
				assert_eq!(countdown, 5);
				assert_eq!(config.name, "Test Table");
			}
			_ => panic!("Wrong message type"),
		}
	}

	#[test]
	fn test_table_status_serialization() {
		use crate::table::{BettingStructure, GameFormat};

		let config = TableConfig {
			id: "test".to_string(),
			name: "Test".to_string(),
			format: GameFormat::Cash,
			betting: BettingStructure::NoLimit,
			small_blind: Some(1.0),
			big_blind: Some(2.0),
			min_buy_in: Some(40.0),
			max_buy_in: Some(200.0),
			buy_in: None,
			starting_stack: None,
			min_players: 2,
			max_players: 6,
			max_raises_per_round: 4,
			payouts: None,
			blind_levels: None,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			action_delay_ms: 500,
			street_delay_ms: 700,
			hand_end_delay_ms: 2000,
			seed: None,
		};

		let info = TableInfo {
			id: "test".to_string(),
			name: "Test".to_string(),
			format: "Cash".to_string(),
			betting: "No-Limit".to_string(),
			blinds: "$1/$2".to_string(),
			buy_in: "$100".to_string(),
			players: 3,
			max_players: 6,
			status: TableStatus::InProgress,
			config,
		};
		let json = serde_json::to_string(&info).unwrap();

		assert!(json.contains("in_progress"));

		let decoded: TableInfo = serde_json::from_str(&json).unwrap();
		assert_eq!(decoded.status, TableStatus::InProgress);
	}

	#[test]
	fn test_player_info_default_is_ai() {
		let json = r#"{"seat":0,"username":"Test","ready":true}"#;
		let info: PlayerInfo = serde_json::from_str(json).unwrap();

		assert!(!info.is_ai); // Default should be false
	}

	#[test]
	fn test_valid_actions_serialization() {
		let valid = ValidActions {
			can_fold: true,
			can_check: false,
			call_amount: Some(10.0),
			raise_options: Some(crate::events::RaiseOptions::Variable {
				min_raise: 20.0,
				max_raise: 100.0,
			}),
			can_all_in: true,
			all_in_amount: 100.0,
		};
		let msg = ServerMessage::ActionRequest {
			valid_actions: valid,
			time_limit: Some(30),
		};
		let encoded = encode_message(&msg);
		let json = std::str::from_utf8(&encoded[4..]).unwrap();
		let decoded: ServerMessage = serde_json::from_str(json).unwrap();

		match decoded {
			ServerMessage::ActionRequest { valid_actions, time_limit } => {
				assert!(valid_actions.can_fold);
				assert!(!valid_actions.can_check);
				assert_eq!(valid_actions.call_amount, Some(10.0));
				assert_eq!(time_limit, Some(30));
			}
			_ => panic!("Wrong message type"),
		}
	}
}
