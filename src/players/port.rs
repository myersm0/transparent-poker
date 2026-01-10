use async_trait::async_trait;
use crate::events::{GameEvent, PlayerAction, Seat, ValidActions};

#[async_trait]
pub trait PlayerPort: Send + Sync {
	async fn request_action(
		&self,
		seat: Seat,
		valid_actions: ValidActions,
		game_state: &GameSnapshot,
	) -> PlayerResponse;

	fn notify(&self, event: &GameEvent);

	fn seat(&self) -> Seat;

	fn name(&self) -> &str;

	fn is_human(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct GameSnapshot {
	pub hand_num: u32,
	pub street: crate::events::Street,
	pub board: Vec<crate::events::Card>,
	pub pot: f32,
	pub seats: Vec<SeatSnapshot>,
	pub hero_cards: Option<[crate::events::Card; 2]>,
	pub action_history: Vec<ActionRecord>,
}

#[derive(Debug, Clone)]
pub struct SeatSnapshot {
	pub seat: Seat,
	pub name: String,
	pub stack: f32,
	pub current_bet: f32,
	pub is_folded: bool,
	pub is_all_in: bool,
	pub position: crate::events::Position,
}

#[derive(Debug, Clone)]
pub struct ActionRecord {
	pub seat: Seat,
	pub street: crate::events::Street,
	pub action: PlayerAction,
}

#[derive(Debug, Clone)]
pub enum PlayerResponse {
	Action(PlayerAction),
	Admin(AdminRequest),
	Timeout,
}

#[derive(Debug, Clone)]
pub enum AdminRequest {
	Spectate,
	LeaveGame,
	KillGame,
}
