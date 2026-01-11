use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use ratatui::{
	layout::{Constraint, Direction, Layout, Rect},
	style::{Modifier, Style},
	widgets::{Block, Borders, Paragraph},
	Frame,
};

use crate::events::{GameEvent, Seat, Standing, ValidActions};
use crate::players::PlayerResponse;
use crate::theme::Theme;
use crate::tui::input::{InputEffect, InputState};
use crate::tui::widgets::TableWidget;
use crate::view::TableView;
use crate::events::ViewUpdater;

const WINNER_HIGHLIGHT_MS: u64 = 5000;

#[derive(Clone)]
pub struct WinnerInfo {
	pub seat: Seat,
	pub amount: f32,
	pub description: Option<String>,
	pub awarded_at: Instant,
}

pub enum GameUIAction {
	None,
	Respond(PlayerResponse),
	Quit,
}

pub struct GameUI {
	pub table_view: TableView,
	view_updater: ViewUpdater,
	pub input_state: InputState,
	pub status_message: Option<String>,
	pub last_winners: Vec<WinnerInfo>,
	pub final_standings: Vec<Standing>,
	_hero_seat: Option<Seat>,
	pub theme: Theme,
	theme_name: String,
	pub info_title: String,
	pub info_lines: Vec<String>,
}

impl GameUI {
	pub fn new(hero_seat: Option<Seat>, theme: Theme, theme_name: String) -> Self {
		Self {
			table_view: TableView::new(),
			view_updater: ViewUpdater::new(hero_seat),
			input_state: InputState::default(),
			status_message: None,
			last_winners: Vec::new(),
			final_standings: Vec::new(),
			_hero_seat: hero_seat,
			theme,
			theme_name,
			info_title: String::new(),
			info_lines: Vec::new(),
		}
	}

	pub fn set_table_info(&mut self, title: String, info: String, info_lines: Vec<String>) {
		self.table_view = self.table_view.clone().with_table_info(title.clone(), info);
		self.info_title = title;
		self.info_lines = info_lines;
	}

	pub fn apply_event(&mut self, event: &GameEvent) {
		self.view_updater.apply(&mut self.table_view, event);

		match event {
			GameEvent::HandStarted { .. } => {
				self.last_winners.clear();
				self.table_view.winner_seats.clear();
			}
			GameEvent::PotAwarded { seat, amount, hand_description, .. } => {
				self.last_winners.push(WinnerInfo {
					seat: *seat,
					amount: *amount,
					description: hand_description.clone(),
					awarded_at: Instant::now(),
				});
				if !self.table_view.winner_seats.contains(&seat.0) {
					self.table_view.winner_seats.push(seat.0);
				}
			}
			GameEvent::GameEnded { final_standings, .. } => {
				self.final_standings = final_standings.clone();
				let (state, effect) = InputState::enter_game_over();
				self.input_state = state;
				self.apply_effect(effect);
			}
			_ => {}
		}
	}

	pub fn enter_action_mode(&mut self, valid_actions: ValidActions) {
		let (state, effect) = InputState::enter_action_mode(valid_actions);
		self.input_state = state;
		self.apply_effect(effect);
	}

	pub fn handle_key(&mut self, key: KeyCode) -> GameUIAction {
		let old_state = std::mem::take(&mut self.input_state);
		let (new_state, effect) = old_state.handle_key(key);
		self.input_state = new_state;
		self.process_effect(effect)
	}

	fn apply_effect(&mut self, effect: InputEffect) {
		match effect {
			InputEffect::SetPrompt(prompt) => {
				self.status_message = Some(prompt);
			}
			InputEffect::ClearPrompt => {
				self.status_message = None;
			}
			InputEffect::CycleTheme => {
				self.cycle_theme();
			}
			_ => {}
		}
	}

	fn process_effect(&mut self, effect: InputEffect) -> GameUIAction {
		match effect {
			InputEffect::None => GameUIAction::None,
			InputEffect::SetPrompt(prompt) => {
				self.status_message = Some(prompt);
				GameUIAction::None
			}
			InputEffect::ClearPrompt => {
				self.status_message = None;
				GameUIAction::None
			}
			InputEffect::Respond(response) => {
				self.status_message = None;
				GameUIAction::Respond(response)
			}
			InputEffect::CycleTheme => {
				self.cycle_theme();
				GameUIAction::None
			}
			InputEffect::Quit => GameUIAction::Quit,
		}
	}

	pub fn cycle_theme(&mut self) {
		let available = Theme::list_available();
		if available.is_empty() {
			return;
		}

		let current_idx = available
			.iter()
			.position(|name| name == &self.theme_name)
			.unwrap_or(0);

		let next_idx = (current_idx + 1) % available.len();
		let next_name = &available[next_idx];

		if let Ok(new_theme) = Theme::load_named(next_name) {
			self.theme = new_theme;
			self.theme_name = next_name.clone();
			if !self.input_state.is_awaiting_input() {
				self.status_message = Some(format!("Theme: {}", next_name));
			}
		}
	}

	pub fn render(&self, frame: &mut Frame, area: Rect) {
		let bg = Block::default().style(Style::default().bg(self.theme.background()));
		frame.render_widget(bg, area);

		let layout = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Min(20),
				Constraint::Length(3),
				Constraint::Length(3),
			])
			.split(area);

		let table_area = layout[0];
		let winner_area = layout[1];
		let status_area = layout[2];

		let table_widget = TableWidget::new(&self.table_view, &self.theme)
			.with_info(&self.info_title, &self.info_lines);
		frame.render_widget(table_widget, table_area);

		// Winner display
		let winner_text = if self.last_winners.is_empty() {
			String::new()
		} else {
			self.last_winners
				.iter()
				.map(|w| {
					let name = self.table_view.players
						.iter()
						.find(|p| p.seat == w.seat.0)
						.map(|p| p.name.as_str())
						.unwrap_or("???");
					if let Some(desc) = &w.description {
						format!("{} wins ${:.0} ({})", name, w.amount, desc)
					} else {
						format!("{} wins ${:.0}", name, w.amount)
					}
				})
				.collect::<Vec<_>>()
				.join(" | ")
		};

		let has_recent_winner = self.last_winners.iter().any(|w| {
			Instant::now().duration_since(w.awarded_at) < Duration::from_millis(WINNER_HIGHLIGHT_MS)
		});

		let winner_style = if has_recent_winner {
			Style::default().fg(self.theme.winner_border()).add_modifier(Modifier::BOLD)
		} else {
			Style::default().fg(self.theme.status_watching())
		};

		let winner = Paragraph::new(winner_text)
			.style(winner_style)
			.block(Block::default().borders(Borders::ALL).title("Result"));
		frame.render_widget(winner, winner_area);

		// Status bar
		let (status_text, status_title, status_style, border_style) = match &self.input_state {
			InputState::AwaitingAction { .. } | InputState::EnteringRaise { .. } => (
				self.status_message.clone().unwrap_or_default(),
				" Your Turn ",
				Style::default().fg(self.theme.status_your_turn()).add_modifier(Modifier::BOLD),
				Style::default().fg(self.theme.status_your_turn_border()),
			),
			InputState::GameOver => (
				self.status_message.clone().unwrap_or_else(|| "Game Over!".to_string()),
				" Game Over ",
				Style::default().fg(self.theme.status_game_over()).add_modifier(Modifier::BOLD),
				Style::default().fg(self.theme.status_game_over_border()),
			),
			_ => (
				self.status_message.clone().unwrap_or_else(|| "Watching...".to_string()),
				" Status ",
				Style::default().fg(self.theme.status_watching()),
				Style::default().fg(self.theme.status_watching_border()),
			),
		};

		let status = Paragraph::new(status_text)
			.style(status_style)
			.block(
				Block::default()
					.borders(Borders::ALL)
					.border_style(border_style)
					.title(status_title),
			);
		frame.render_widget(status, status_area);
	}
}
