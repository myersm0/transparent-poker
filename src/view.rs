use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Card {
	pub rank: char,
	pub suit: char,
}

impl Card {
	pub fn new(rank: char, suit: char) -> Self {
		Self { rank, suit }
	}

	pub fn suit_symbol(&self) -> &'static str {
		match self.suit {
			's' => "♠",
			'h' => "♥",
			'd' => "♦",
			'c' => "♣",
			_ => "?",
		}
	}

	pub fn display(&self) -> String {
		format!("{}{}", self.rank, self.suit_symbol())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStatus {
	Active,
	Folded,
	AllIn,
	SittingOut,
	Eliminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Position {
	Button,
	SmallBlind,
	BigBlind,
	None,
}

impl Position {
	pub fn label(&self) -> &'static str {
		match self {
			Position::Button => "D",
			Position::SmallBlind => "SB",
			Position::BigBlind => "BB",
			Position::None => "",
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerView {
	pub seat: usize,
	pub name: String,
	pub stack: f32,
	pub current_bet: f32,
	pub status: PlayerStatus,
	pub position: Position,
	pub hole_cards: Option<[Card; 2]>,
	pub is_hero: bool,
	pub is_actor: bool,
	pub last_action: Option<String>,
	pub action_fresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Street {
	Preflop,
	Flop,
	Turn,
	River,
	Showdown,
}

impl Street {
	pub fn name(&self) -> &'static str {
		match self {
			Street::Preflop => "Preflop",
			Street::Flop => "Flop",
			Street::Turn => "Turn",
			Street::River => "River",
			Street::Showdown => "Showdown",
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableView {
	pub game_id: Option<String>,
	pub hand_num: u32,
	pub street: Street,
	pub board: Vec<Card>,
	pub pot: f32,
	pub players: Vec<PlayerView>,
	pub blinds: (f32, f32),
	#[serde(default)]
	pub action_prompt: Option<ActionPrompt>,
	#[serde(default)]
	pub chat_messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionPrompt {
	pub to_call: f32,
	pub min_raise: f32,
	pub max_bet: f32,
	pub can_raise: bool,
	pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessage {
	pub sender: String,
	pub text: String,
	#[serde(default)]
	pub is_system: bool,
}

impl TableView {
	pub fn new() -> Self {
		Self {
			game_id: None,
			hand_num: 0,
			street: Street::Preflop,
			board: Vec::new(),
			pot: 0.0,
			players: Vec::new(),
			blinds: (0.0, 0.0),
			action_prompt: None,
			chat_messages: Vec::new(),
		}
	}

	pub fn actor(&self) -> Option<&PlayerView> {
		self.players.iter().find(|p| p.is_actor)
	}

	pub fn hero(&self) -> Option<&PlayerView> {
		self.players.iter().find(|p| p.is_hero)
	}
}

impl Default for TableView {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_card_suit_symbol() {
		assert_eq!(Card::new('A', 's').suit_symbol(), "♠");
		assert_eq!(Card::new('K', 'h').suit_symbol(), "♥");
		assert_eq!(Card::new('Q', 'd').suit_symbol(), "♦");
		assert_eq!(Card::new('J', 'c').suit_symbol(), "♣");
		assert_eq!(Card::new('T', 'x').suit_symbol(), "?");
	}

	#[test]
	fn test_card_display() {
		assert_eq!(Card::new('A', 's').display(), "A♠");
		assert_eq!(Card::new('K', 'h').display(), "K♥");
		assert_eq!(Card::new('2', 'd').display(), "2♦");
	}

	#[test]
	fn test_position_label() {
		assert_eq!(Position::Button.label(), "D");
		assert_eq!(Position::SmallBlind.label(), "SB");
		assert_eq!(Position::BigBlind.label(), "BB");
		assert_eq!(Position::None.label(), "");
	}

	#[test]
	fn test_street_name() {
		assert_eq!(Street::Preflop.name(), "Preflop");
		assert_eq!(Street::Flop.name(), "Flop");
		assert_eq!(Street::Turn.name(), "Turn");
		assert_eq!(Street::River.name(), "River");
		assert_eq!(Street::Showdown.name(), "Showdown");
	}

	#[test]
	fn test_table_view_new() {
		let view = TableView::new();
		assert_eq!(view.hand_num, 0);
		assert_eq!(view.pot, 0.0);
		assert!(view.board.is_empty());
		assert!(view.players.is_empty());
	}

	#[test]
	fn test_table_view_actor() {
		let mut view = TableView::new();
		view.players.push(PlayerView {
			seat: 0,
			name: "Alice".to_string(),
			stack: 100.0,
			current_bet: 0.0,
			status: PlayerStatus::Active,
			position: Position::Button,
			hole_cards: None,
			is_hero: false,
			is_actor: false,
			last_action: None,
			action_fresh: false,
		});
		view.players.push(PlayerView {
			seat: 1,
			name: "Bob".to_string(),
			stack: 100.0,
			current_bet: 0.0,
			status: PlayerStatus::Active,
			position: Position::SmallBlind,
			hole_cards: None,
			is_hero: false,
			is_actor: true,
			last_action: None,
			action_fresh: false,
		});
		
		let actor = view.actor().unwrap();
		assert_eq!(actor.name, "Bob");
	}

	#[test]
	fn test_table_view_hero() {
		let mut view = TableView::new();
		view.players.push(PlayerView {
			seat: 0,
			name: "Human".to_string(),
			stack: 100.0,
			current_bet: 0.0,
			status: PlayerStatus::Active,
			position: Position::Button,
			hole_cards: None,
			is_hero: true,
			is_actor: false,
			last_action: None,
			action_fresh: false,
		});
		
		let hero = view.hero().unwrap();
		assert_eq!(hero.name, "Human");
	}
}
