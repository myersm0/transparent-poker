use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
	backend::Backend,
	layout::{Constraint, Direction, Layout},
	style::{Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
	Frame, Terminal,
};

use crate::bank::Bank;
use crate::config::PlayerConfig;
use crate::table::{GameFormat, TableConfig};
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct LobbyPlayer {
	pub id: String,
	pub is_host: bool,
	pub is_human: bool,
	pub strategy: Option<String>,
}

pub enum MenuResult {
	StartGame {
		table: TableConfig,
		players: Vec<LobbyPlayer>,
	},
	Quit,
}

enum MenuState {
	TableSelect,
	Lobby,
}

pub struct Menu {
	state: MenuState,
	tables: Vec<TableConfig>,
	table_list_state: ListState,
	bank: Bank,
	host_id: String,
	roster: Vec<PlayerConfig>,

	selected_table: Option<TableConfig>,
	lobby_players: Vec<LobbyPlayer>,
	lobby_cursor: usize,
	theme: Theme,
}

impl Menu {
	pub fn new(tables: Vec<TableConfig>, bank: Bank, host_id: String, roster: Vec<PlayerConfig>, theme: Theme) -> Self {
		let mut table_list_state = ListState::default();
		table_list_state.select(Some(0));

		let mut menu = Self {
			state: MenuState::TableSelect,
			tables,
			table_list_state,
			bank,
			host_id: host_id.clone(),
			roster,
			selected_table: None,
			lobby_players: Vec::new(),
			lobby_cursor: 0,
			theme,
		};

		menu.lobby_players.push(LobbyPlayer {
			id: host_id,
			is_host: true,
			is_human: true,
			strategy: None,
		});

		menu
	}

	pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<MenuResult> {
		loop {
			terminal.draw(|f| self.draw(f))?;

			if event::poll(std::time::Duration::from_millis(100))? {
				if let Event::Key(key) = event::read()? {
					if key.kind != KeyEventKind::Press {
						continue;
					}

					match &self.state {
						MenuState::TableSelect => {
							match key.code {
								KeyCode::Char('q') | KeyCode::Esc => {
									return Ok(MenuResult::Quit);
								}
								KeyCode::Up => {
									self.move_table_selection(-1);
								}
								KeyCode::Down => {
									self.move_table_selection(1);
								}
								KeyCode::Enter => {
									if let Some(idx) = self.table_list_state.selected() {
										if idx < self.tables.len() {
											self.selected_table = Some(self.tables[idx].clone());
											self.auto_fill_lobby();
											self.state = MenuState::Lobby;
											self.lobby_cursor = self.lobby_players.len();
										}
									}
								}
								_ => {}
							}
						}
						MenuState::Lobby => {
							match key.code {
								KeyCode::Esc => {
									self.state = MenuState::TableSelect;
									self.selected_table = None;
								}
								KeyCode::Char('q') => {
									return Ok(MenuResult::Quit);
								}
								KeyCode::Up => {
									if self.lobby_cursor > 0 {
										self.lobby_cursor -= 1;
									}
								}
								KeyCode::Down => {
									let max = self.lobby_players.len();
									if self.lobby_cursor < max {
										self.lobby_cursor += 1;
									}
								}
								KeyCode::Enter => {
									if self.lobby_cursor == self.lobby_players.len() {
										self.add_next_ai();
									} else if self.can_start() {
										return Ok(MenuResult::StartGame {
											table: self.selected_table.clone().expect("table selected in lobby state"),
											players: self.lobby_players.clone(),
										});
									}
								}
								KeyCode::Char('a') => {
									self.add_next_ai();
								}
								KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
									self.remove_player_at_cursor();
								}
								KeyCode::Char('s') => {
									if self.can_start() {
										return Ok(MenuResult::StartGame {
											table: self.selected_table.clone().expect("table selected in lobby state"),
											players: self.lobby_players.clone(),
										});
									}
								}
								_ => {}
							}
						}
					}
				}
			}
		}
	}

	fn move_table_selection(&mut self, delta: i32) {
		let len = self.tables.len();
		if len == 0 {
			return;
		}
		let current = self.table_list_state.selected().unwrap_or(0) as i32;
		let new = (current + delta).rem_euclid(len as i32) as usize;
		self.table_list_state.select(Some(new));
	}

	fn add_next_ai(&mut self) {
		if let Some(table) = &self.selected_table {
			if self.lobby_players.len() >= table.max_players {
				return;
			}
		}

		let used_ids: Vec<String> = self.lobby_players.iter().map(|p| p.id.clone()).collect();
		if let Some(player_config) = self.roster.iter().find(|p| !used_ids.contains(&p.display_name())) {
			self.lobby_players.push(LobbyPlayer {
				id: player_config.display_name(),
				is_host: false,
				is_human: false,
				strategy: Some(player_config.strategy.clone()),
			});
			self.lobby_cursor = self.lobby_players.len();
		}
	}

	fn auto_fill_lobby(&mut self) {
		let max_players = self.selected_table.as_ref().map(|t| t.max_players).unwrap_or(6);
		let used_ids: Vec<String> = self.lobby_players.iter().map(|p| p.id.clone()).collect();

		for player_config in &self.roster {
			if self.lobby_players.len() >= max_players {
				break;
			}
			if used_ids.contains(&player_config.display_name()) {
				continue;
			}
			if rand::random::<f32>() < player_config.join_probability {
				self.lobby_players.push(LobbyPlayer {
					id: player_config.display_name(),
					is_host: false,
					is_human: false,
					strategy: Some(player_config.strategy.clone()),
				});
			}
		}
		self.lobby_cursor = self.lobby_players.len();
	}

	fn remove_player_at_cursor(&mut self) {
		if self.lobby_cursor < self.lobby_players.len() {
			let player = &self.lobby_players[self.lobby_cursor];
			if !player.is_host {
				self.lobby_players.remove(self.lobby_cursor);
				if self.lobby_cursor > 0 && self.lobby_cursor >= self.lobby_players.len() {
					self.lobby_cursor = self.lobby_players.len();
				}
			}
		}
	}

	fn can_start(&self) -> bool {
		if let Some(table) = &self.selected_table {
			let count = self.lobby_players.len();
			count >= table.min_players && count <= table.max_players
		} else {
			false
		}
	}

	fn draw(&self, frame: &mut Frame) {
		match &self.state {
			MenuState::TableSelect => self.draw_table_select(frame),
			MenuState::Lobby => self.draw_lobby(frame),
		}
	}

	fn draw_table_select(&self, frame: &mut Frame) {
		let area = frame.area();

		let bg = Block::default().style(Style::default().bg(self.theme.background()));
		frame.render_widget(bg, area);

		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(3),
				Constraint::Min(10),
				Constraint::Length(3),
			])
			.split(area);

		let host_bankroll = self.bank.get_bankroll(&self.host_id);
		let header = Paragraph::new(format!(
			"  POKER TUI                                    Bankroll: ${:.0}",
			host_bankroll
		))
		.style(Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD))
		.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(header, chunks[0]);

		let items: Vec<ListItem> = self
			.tables
			.iter()
			.map(|t| {
				let line = Line::from(vec![
					Span::styled(
						format!("{:<25}", t.name),
						Style::default().fg(self.theme.menu_text()),
					),
					Span::styled(
						format!("{:<18}", t.summary()),
						Style::default().fg(self.theme.menu_highlight()),
					),
					Span::styled(t.player_range(), Style::default().fg(self.theme.menu_unselected())),
				]);
				ListItem::new(line)
			})
			.collect();

		let list = List::new(items)
			.block(
				Block::default()
					.title(" SELECT TABLE ")
					.borders(Borders::ALL)
					.border_style(Style::default().fg(self.theme.menu_border())),
			)
			.highlight_style(
				Style::default()
					.fg(self.theme.menu_selected())
					.bg(self.theme.menu_selected_bg())
					.add_modifier(Modifier::BOLD),
			)
			.highlight_symbol("> ");

		frame.render_stateful_widget(list, chunks[1], &mut self.table_list_state.clone());

		let help = Paragraph::new("  [↑/↓] Select  [Enter] Open Lobby  [q] Quit")
			.style(Style::default().fg(self.theme.menu_unselected()))
			.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(help, chunks[2]);
	}

	fn draw_lobby(&self, frame: &mut Frame) {
		let area = frame.area();
		let table = self.selected_table.as_ref().expect("table selected in lobby state");

		let bg = Block::default().style(Style::default().bg(self.theme.background()));
		frame.render_widget(bg, area);

		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(3),
				Constraint::Length(8),
				Constraint::Min(8),
				Constraint::Length(3),
			])
			.split(area);

		let header = Paragraph::new(format!("  TABLE: {}", table.name))
			.style(Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD))
			.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(header, chunks[0]);

		let info = self.build_table_info(table);
		let info_widget = Paragraph::new(info)
			.style(Style::default().fg(self.theme.menu_text()))
			.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(info_widget, chunks[1]);

		let player_lines = self.build_player_list(table);
		let player_list = Paragraph::new(player_lines)
			.block(
				Block::default()
					.title(format!(
						" PLAYERS ({}/{}) ",
						self.lobby_players.len(),
						table.max_players
					))
					.borders(Borders::ALL)
					.border_style(Style::default().fg(self.theme.menu_border())),
			);
		frame.render_widget(player_list, chunks[2]);

		let can_start = self.can_start();
		let help_text = if can_start {
			"  [s] Start Game  [a] Add AI  [d] Remove  [Esc] Back  [q] Quit"
		} else {
			format!(
				"  Need {} more players  [a] Add AI  [Esc] Back  [q] Quit",
				table.min_players.saturating_sub(self.lobby_players.len())
			)
			.leak()
		};
		let help = Paragraph::new(help_text)
			.style(Style::default().fg(self.theme.menu_unselected()))
			.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(help, chunks[3]);
	}

	fn build_table_info(&self, table: &TableConfig) -> Vec<Line<'static>> {
		let mut lines = vec![
			Line::from(format!("  Format:   {}", table.format)),
			Line::from(format!("  Betting:  {} Hold'em", table.betting)),
		];

		match table.format {
			GameFormat::Cash => {
				let (small, big) = table.current_blinds();
				lines.push(Line::from(format!("  Blinds:   ${:.0}/${:.0}", small, big)));
				lines.push(Line::from(format!(
					"  Buy-in:   ${:.0} - ${:.0}",
					table.min_buy_in.unwrap_or(0.0),
					table.max_buy_in.unwrap_or(0.0)
				)));
			}
			GameFormat::SitNGo => {
				lines.push(Line::from(format!(
					"  Buy-in:   ${:.0}",
					table.buy_in.unwrap_or(0.0)
				)));
				lines.push(Line::from(format!(
					"  Stack:    {:.0} chips",
					table.starting_stack.unwrap_or(0.0)
				)));
				if let Some(payouts) = &table.payouts {
					let payout_str: Vec<String> = payouts
						.iter()
						.enumerate()
						.map(|(i, p)| format!("{}: {:.0}%", ordinal(i + 1), p * 100.0))
						.collect();
					lines.push(Line::from(format!("  Payouts:  {}", payout_str.join(" | "))));
				}
			}
		}

		lines
	}

	fn build_player_list(&self, table: &TableConfig) -> Vec<Line<'static>> {
		let mut lines = Vec::new();

		for (i, player) in self.lobby_players.iter().enumerate() {
			let cursor = if i == self.lobby_cursor { "> " } else { "  " };
			let host_tag = if player.is_host { " (host)" } else { "" };
			let bankroll = self.bank.get_bankroll(&player.id);

			let can_afford = match table.format {
				GameFormat::Cash => bankroll >= table.min_buy_in.unwrap_or(0.0),
				GameFormat::SitNGo => bankroll >= table.buy_in.unwrap_or(0.0),
			};

			let status = if can_afford { "Ready" } else { "Broke!" };
			let status_color = if can_afford { self.theme.stack() } else { self.theme.status_quit() };

			let name_color = if player.is_host {
				self.theme.menu_host_marker()
			} else if !player.is_human {
				self.theme.menu_ai_marker()
			} else {
				self.theme.menu_text()
			};

			lines.push(Line::from(vec![
				Span::raw(cursor),
				Span::styled(format!("{:<20}", format!("{}{}", player.id, host_tag)), Style::default().fg(name_color)),
				Span::styled(format!("${:<12.0}", bankroll), Style::default().fg(self.theme.bet())),
				Span::styled(status, Style::default().fg(status_color)),
			]));
		}

		if self.lobby_players.len() < table.max_players {
			let cursor = if self.lobby_cursor == self.lobby_players.len() {
				"> "
			} else {
				"  "
			};
			lines.push(Line::from(vec![Span::styled(
				format!("{}+ Add player...", cursor),
				Style::default().fg(self.theme.menu_unselected()),
			)]));
		}

		lines
	}
}

fn ordinal(n: usize) -> String {
	match n {
		1 => "1st".to_string(),
		2 => "2nd".to_string(),
		3 => "3rd".to_string(),
		_ => format!("{}th", n),
	}
}
