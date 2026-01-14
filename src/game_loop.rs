use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::events::{GameEvent, Seat};
use crate::net::{GameClient, ServerMessage};
use crate::players::PlayerResponse;
use crate::table::{build_info_lines, TableConfig};
use crate::theme::Theme;
use crate::tui::{GameUI, GameUIAction};

pub enum GameLoopResult {
	ReturnToLobby,
	Quit,
}

pub fn run_game(
	terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
	client: &mut GameClient,
	username: &str,
	theme: Theme,
	theme_name: String,
	table_config: TableConfig,
	num_players: usize,
) -> io::Result<GameLoopResult> {
	// Flush any stale keyboard input
	while event::poll(Duration::from_millis(0))? {
		let _ = event::read();
	}
	let table_info_str = format!("{} {}", table_config.betting, table_config.format);
	let info_lines = build_info_lines(&table_config, num_players, table_config.seed);
	let table_name = table_config.name.clone();

	let mut game_ui = GameUI::new(None, theme.clone(), theme_name.clone());
	game_ui.set_table_info(table_name.clone(), table_info_str.clone(), info_lines.clone());
	let mut game_seat: Option<Seat> = None;

	loop {
		while let Some(msg) = client.try_recv() {
			match msg {
				ServerMessage::GameEvent(event) => {
					if let GameEvent::HandStarted { seats, .. } = &event {
						if game_seat.is_none() {
							let found_seat = seats.iter()
								.find(|s| s.name.eq_ignore_ascii_case(username))
								.map(|s| s.seat);

							if let Some(seat) = found_seat {
								game_seat = Some(seat);
								game_ui = GameUI::new(Some(seat), theme.clone(), theme_name.clone());
								game_ui.set_table_info(table_name.clone(), table_info_str.clone(), info_lines.clone());
							}
						}
					}
					game_ui.apply_event(&event);
				}
				ServerMessage::ActionRequest { valid_actions, .. } => {
					game_ui.enter_action_mode(valid_actions);
				}
				ServerMessage::Error { message } => {
					game_ui.status_message = Some(format!("Error: {}", message));
				}
				_ => {}
			}
		}

		terminal.draw(|f| {
			game_ui.render(f, f.area());
		})?;

		if event::poll(Duration::from_millis(50))? {
			if let Event::Key(key) = event::read()? {
				if key.kind != KeyEventKind::Press {
					continue;
				}

				if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
					let _ = client.leave_table();
					return Ok(GameLoopResult::Quit);
				}

				match game_ui.handle_key(key.code) {
					GameUIAction::Respond(PlayerResponse::Action(action)) => {
						let _ = client.action(action);
					}
					GameUIAction::Quit => {
						let _ = client.leave_table();
						std::thread::sleep(Duration::from_millis(100));
						client.drain();
						return Ok(GameLoopResult::ReturnToLobby);
					}
					_ => {}
				}
			}
		}
	}
}
