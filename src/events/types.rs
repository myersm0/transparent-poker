use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HandId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seat(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
	pub rank: char,
	pub suit: char,
}

impl Card {
	pub fn new(rank: char, suit: char) -> Self {
		Self { rank, suit }
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Street {
	Preflop,
	Flop,
	Turn,
	River,
	Showdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
	Button,
	SmallBlind,
	BigBlind,
	None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
	GameCreated {
		game_id: GameId,
		config: GameConfig,
	},

	PlayerJoined {
		seat: Seat,
		name: String,
		stack: f32,
		is_human: bool,
	},

	PlayerLeft {
		seat: Seat,
		reason: LeaveReason,
	},

	GameStarted {
		seats: Vec<SeatInfo>,
	},

	HandStarted {
		hand_id: HandId,
		hand_num: u32,
		button: Seat,
		blinds: Blinds,
		seats: Vec<SeatInfo>,
	},

	HoleCardsDealt {
		seat: Seat,
		cards: [Card; 2],
	},

	BlindPosted {
		seat: Seat,
		blind_type: BlindType,
		amount: f32,
	},

	StreetChanged {
		street: Street,
		board: Vec<Card>,
	},

	ActionRequest {
		seat: Seat,
		valid_actions: ValidActions,
		time_limit: Option<u32>,
	},

	ActionTaken {
		seat: Seat,
		action: PlayerAction,
		stack_after: f32,
		pot_after: f32,
	},

	PotAwarded {
		seat: Seat,
		amount: f32,
		hand_description: Option<String>,
		pot_type: PotType,
	},

	ShowdownReveal {
		reveals: Vec<(Seat, [Card; 2])>,
	},

	HandEnded {
		hand_id: HandId,
		results: Vec<HandResult>,
	},

	GameEnded {
		reason: GameEndReason,
		final_standings: Vec<Standing>,
	},

	ChatMessage {
		sender: ChatSender,
		text: String,
	},

	AdminAction {
		seat: Option<Seat>,
		action: AdminActionType,
	},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
	pub betting_structure: BettingStructure,
	pub small_blind: f32,
	pub big_blind: f32,
	pub starting_stack: f32,
	pub max_players: usize,
	pub time_bank: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BettingStructure {
	NoLimit,
	PotLimit,
	FixedLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeatInfo {
	pub seat: Seat,
	pub name: String,
	pub stack: f32,
	pub position: Position,
	pub is_active: bool,
	pub is_human: bool,
	pub is_occupied: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Blinds {
	pub small: f32,
	pub big: f32,
	pub ante: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlindType {
	Small,
	Big,
	Ante,
	Straddle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidActions {
	pub can_fold: bool,
	pub can_check: bool,
	pub call_amount: Option<f32>,
	pub raise_options: Option<RaiseOptions>,
	pub can_all_in: bool,
	pub all_in_amount: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaiseOptions {
	Fixed {
		amount: f32,
	},
	Variable {
		min_raise: f32,
		max_raise: f32,
	},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerAction {
	Fold,
	Check,
	Call { amount: f32 },
	Bet { amount: f32 },
	Raise { amount: f32 },
	AllIn { amount: f32 },
	Timeout,
}

impl PlayerAction {
	pub fn description(&self) -> String {
		match self {
			PlayerAction::Fold => "folds".to_string(),
			PlayerAction::Check => "checks".to_string(),
			PlayerAction::Call { amount } => format!("calls ${:.0}", amount),
			PlayerAction::Bet { amount } => format!("bets ${:.0}", amount),
			PlayerAction::Raise { amount } => format!("raises to ${:.0}", amount),
			PlayerAction::AllIn { amount } => format!("all-in ${:.0}", amount),
			PlayerAction::Timeout => "timed out".to_string(),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PotType {
	Main,
	Side(u8),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandResult {
	pub seat: Seat,
	pub stack_change: f32,
	pub final_stack: f32,
	pub showed_cards: Option<[Card; 2]>,
	pub hand_description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaveReason {
	Quit,
	Disconnected,
	Eliminated,
	Spectating,
	Kicked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEndReason {
	Winner,
	AllPlayersLeft,
	HostTerminated,
	Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Standing {
	pub seat: Seat,
	pub name: String,
	pub final_stack: f32,
	pub finish_position: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatSender {
	System,
	Dealer,
	Player(Seat),
	Spectator(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminActionType {
	Spectate,
	LeaveGame,
	KillGame,
	Pause,
	Resume,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_player_action_description_fold() {
		assert_eq!(PlayerAction::Fold.description(), "folds");
	}

	#[test]
	fn test_player_action_description_check() {
		assert_eq!(PlayerAction::Check.description(), "checks");
	}

	#[test]
	fn test_player_action_description_call() {
		let action = PlayerAction::Call { amount: 50.0 };
		assert_eq!(action.description(), "calls $50");
	}

	#[test]
	fn test_player_action_description_bet() {
		let action = PlayerAction::Bet { amount: 100.0 };
		assert_eq!(action.description(), "bets $100");
	}

	#[test]
	fn test_player_action_description_raise() {
		let action = PlayerAction::Raise { amount: 200.0 };
		assert_eq!(action.description(), "raises to $200");
	}

	#[test]
	fn test_player_action_description_allin() {
		let action = PlayerAction::AllIn { amount: 500.0 };
		assert_eq!(action.description(), "all-in $500");
	}

	#[test]
	fn test_player_action_description_timeout() {
		assert_eq!(PlayerAction::Timeout.description(), "timed out");
	}

	#[test]
	fn test_card_new() {
		let card = Card::new('A', 's');
		assert_eq!(card.rank, 'A');
		assert_eq!(card.suit, 's');
	}

	#[test]
	fn test_valid_actions_default_state() {
		let valid = ValidActions {
			can_fold: true,
			can_check: false,
			call_amount: Some(10.0),
			raise_options: Some(RaiseOptions::Variable {
				min_raise: 20.0,
				max_raise: 100.0,
			}),
			can_all_in: true,
			all_in_amount: 100.0,
		};
		assert!(valid.can_fold);
		assert!(!valid.can_check);
		assert_eq!(valid.call_amount, Some(10.0));
	}

	#[test]
	fn test_seat_equality() {
		assert_eq!(Seat(0), Seat(0));
		assert_ne!(Seat(0), Seat(1));
	}

	#[test]
	fn test_street_equality() {
		assert_eq!(Street::Preflop, Street::Preflop);
		assert_ne!(Street::Preflop, Street::Flop);
	}

	#[test]
	fn test_blinds_struct() {
		let blinds = Blinds {
			small: 5.0,
			big: 10.0,
			ante: Some(1.0),
		};
		assert_eq!(blinds.small, 5.0);
		assert_eq!(blinds.big, 10.0);
		assert_eq!(blinds.ante, Some(1.0));
	}
}
