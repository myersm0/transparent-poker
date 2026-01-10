use async_trait::async_trait;
use crate::events::{GameEvent, PlayerAction, Seat, ValidActions};
use crate::players::{GameSnapshot, PlayerPort, PlayerResponse};

pub struct RemotePlayerConfig {
	pub server_url: String,
	pub auth_token: String,
	pub opponent_id: String,
}

pub struct RemotePlayer {
	seat: Seat,
	name: String,
	#[allow(dead_code)]
	config: RemotePlayerConfig,
}

impl RemotePlayer {
	pub fn new(seat: Seat, name: &str, config: RemotePlayerConfig) -> Self {
		Self {
			seat,
			name: name.to_string(),
			config,
		}
	}
}

#[async_trait]
impl PlayerPort for RemotePlayer {
	async fn request_action(
		&self,
		_seat: Seat,
		valid_actions: ValidActions,
		_game_state: &GameSnapshot,
	) -> PlayerResponse {
		// TODO: implement async server call
		// let response = reqwest::Client::new()
		//     .post(&format!("{}/v1/action", self.config.server_url))
		//     .bearer_auth(&self.config.auth_token)
		//     .json(&json!({
		//         "opponent_id": self.config.opponent_id,
		//         "game_state": game_state,
		//         "valid_actions": valid_actions,
		//     }))
		//     .send()
		//     .await?
		//     .json::<ActionResponse>()
		//     .await?;

		let action = if valid_actions.can_check {
			PlayerAction::Check
		} else {
			PlayerAction::Fold
		};

		PlayerResponse::Action(action)
	}

	fn notify(&self, _event: &GameEvent) {
		// TODO: optionally send events to server for opponent state tracking
	}

	fn seat(&self) -> Seat {
		self.seat
	}

	fn name(&self) -> &str {
		&self.name
	}

	fn is_human(&self) -> bool {
		false
	}
}
