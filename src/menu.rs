use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
	backend::Backend,
	layout::{Constraint, Direction, Layout, Rect},
	style::{Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
	Frame, Terminal,
};

use crate::events::Seat;
use crate::lobby::{LobbyBackend, LobbyCommand, LobbyEvent, LobbyPlayer, TableSummary};
use crate::net::protocol::TableStatus;
use crate::table::TableConfig;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
	#[default]
	Manual,
	Alpha,
	Betting,
	Format,
	StakesAsc,
	StakesDesc,
}

impl SortMode {
	fn next(self) -> Self {
		match self {
			SortMode::Manual => SortMode::Alpha,
			SortMode::Alpha => SortMode::Format,
			SortMode::Format => SortMode::Betting,
			SortMode::Betting => SortMode::StakesAsc,
			SortMode::StakesAsc => SortMode::StakesDesc,
			SortMode::StakesDesc => SortMode::Manual,
		}
	}

	fn prev(self) -> Self {
		match self {
			SortMode::Manual => SortMode::StakesDesc,
			SortMode::Alpha => SortMode::Manual,
			SortMode::Format => SortMode::Alpha,
			SortMode::Betting => SortMode::Format,
			SortMode::StakesAsc => SortMode::Betting,
			SortMode::StakesDesc => SortMode::StakesAsc,
		}
	}

	fn label(self) -> &'static str {
		match self {
			SortMode::Manual => "Manual",
			SortMode::Alpha => "A-Z",
			SortMode::Format => "Format (cash game or tournament)",
			SortMode::Betting => "Betting structure (no limit, pot limit, fixed)",
			SortMode::StakesAsc => "Minimum buy-in (ascending)",
			SortMode::StakesDesc => "Minimum buy-in (descending)",
		}
	}
}

pub enum MenuResult {
	StartGame {
		table: TableConfig,
		players: Vec<LobbyPlayer>,
	},
	NetworkGameStarted {
		seat: Seat,
	},
	Quit,
}

enum MenuState {
	TableSelect,
	Lobby,
}

pub struct Menu<B: LobbyBackend> {
	backend: B,
	state: MenuState,
	host_id: String,

	tables: Vec<TableSummary>,
	sorted_indices: Vec<usize>,
	sort_mode: SortMode,
	table_list_state: ListState,

	current_table_id: Option<String>,
	current_table_name: String,
	min_players: usize,
	max_players: usize,
	players: Vec<LobbyPlayer>,
	lobby_cursor: usize,

	theme: Theme,
	show_info: bool,
	error_message: Option<String>,
}

impl<B: LobbyBackend> Menu<B> {
	pub fn new(backend: B, host_id: String, theme: Theme) -> Self {
		let mut table_list_state = ListState::default();
		table_list_state.select(Some(1)); // Start at 1 to skip header row

		Self {
			backend,
			state: MenuState::TableSelect,
			host_id,
			tables: Vec::new(),
			sorted_indices: Vec::new(),
			sort_mode: SortMode::Manual,
			table_list_state,
			current_table_id: None,
			current_table_name: String::new(),
			min_players: 2,
			max_players: 6,
			players: Vec::new(),
			lobby_cursor: 0,
			theme,
			show_info: false,
			error_message: None,
		}
	}

	pub fn into_backend(self) -> B {
		self.backend
	}

	fn process_events(&mut self) -> Option<MenuResult> {
		while let Some(event) = self.backend.poll() {
			match event {
				LobbyEvent::TablesListed(tables) => {
					self.tables = tables;
					self.sorted_indices = (0..self.tables.len()).collect();
					self.apply_sort();
				}
				LobbyEvent::TableJoined { table_id, table_name, players, min_players, max_players, .. } => {
					self.current_table_id = Some(table_id);
					self.current_table_name = table_name;
					self.players = players;
					self.min_players = min_players;
					self.max_players = max_players;
					self.lobby_cursor = self.players.len();
					self.state = MenuState::Lobby;
				}
				LobbyEvent::PlayerJoined { seat, username, is_ai } => {
					self.players.push(LobbyPlayer {
						seat: Some(seat),
						id: username.to_lowercase(),
						name: username,
						is_host: false,
						is_human: !is_ai,
						is_ready: is_ai,
						strategy: None,
						bankroll: None,
					});
				}
				LobbyEvent::PlayerLeft { seat } => {
					self.players.retain(|p| p.seat != Some(seat));
				}
				LobbyEvent::PlayerReady { seat } => {
					if let Some(p) = self.players.iter_mut().find(|p| p.seat == Some(seat)) {
						p.is_ready = true;
					}
				}
				LobbyEvent::GameStarting => {}
				LobbyEvent::GameReady { table, players } => {
					return Some(MenuResult::StartGame { table, players });
				}
				LobbyEvent::NetworkGameStarted { seat } => {
					return Some(MenuResult::NetworkGameStarted { seat });
				}
				LobbyEvent::Error(msg) => {
					self.error_message = Some(msg);
				}
				LobbyEvent::LeftTable => {
					self.current_table_id = None;
					self.current_table_name.clear();
					self.players.clear();
					self.state = MenuState::TableSelect;
				}
			}
		}
		None
	}

	fn apply_sort(&mut self) {
		self.sorted_indices = (0..self.tables.len()).collect();

		match self.sort_mode {
			SortMode::Manual => {}
			SortMode::Alpha => {
				self.sorted_indices.sort_by(|&a, &b| {
					self.tables[a].name.to_lowercase().cmp(&self.tables[b].name.to_lowercase())
				});
			}
			SortMode::Format => {
				self.sorted_indices.sort_by(|&a, &b| {
					self.tables[a].format.cmp(&self.tables[b].format)
				});
			}
			SortMode::Betting => {
				self.sorted_indices.sort_by(|&a, &b| {
					self.tables[a].betting.cmp(&self.tables[b].betting)
				});
			}
			SortMode::StakesAsc => {
				self.sorted_indices.sort_by(|&a, &b| {
					self.tables[a].blinds.cmp(&self.tables[b].blinds)
				});
			}
			SortMode::StakesDesc => {
				self.sorted_indices.sort_by(|&a, &b| {
					self.tables[b].blinds.cmp(&self.tables[a].blinds)
				});
			}
		}

		if !self.sorted_indices.is_empty() {
			self.table_list_state.select(Some(1)); // Skip header row
		}
	}

	fn cycle_sort_next(&mut self) {
		self.sort_mode = self.sort_mode.next();
		self.apply_sort();
	}

	fn cycle_sort_prev(&mut self) {
		self.sort_mode = self.sort_mode.prev();
		self.apply_sort();
	}

	fn selected_table_index(&self) -> Option<usize> {
		self.table_list_state.selected().and_then(|display_idx| {
			// display_idx 0 is header, actual tables start at 1
			if display_idx == 0 {
				None
			} else {
				self.sorted_indices.get(display_idx - 1).copied()
			}
		})
	}

	fn can_start(&self) -> bool {
		let count = self.players.len();
		count >= self.min_players && count <= self.max_players
	}

	fn all_ready(&self) -> bool {
		self.players.iter().all(|p| p.is_ready)
	}

	pub fn run<Back: Backend>(&mut self, terminal: &mut Terminal<Back>) -> io::Result<MenuResult> {
		// Flush any stale keyboard input from previous session
		flush_keyboard_buffer();

		self.backend.send(LobbyCommand::ListTables);

		loop {
			if let Some(result) = self.process_events() {
				return Ok(result);
			}

			terminal.draw(|f| self.draw(f))?;

			if event::poll(std::time::Duration::from_millis(50))? {
				if let Event::Key(key) = event::read()? {
					if key.kind == KeyEventKind::Press {
						self.error_message = None;

						if self.show_info {
							self.show_info = false;
							continue;
						}

						match &self.state {
							MenuState::TableSelect => {
								match key.code {
									KeyCode::Char('q') => {
										return Ok(MenuResult::Quit);
									}
									KeyCode::Up => {
										self.move_table_selection(-1);
									}
									KeyCode::Down => {
										self.move_table_selection(1);
									}
									KeyCode::Left => {
										self.cycle_sort_prev();
									}
									KeyCode::Right => {
										self.cycle_sort_next();
									}
									KeyCode::Char('i') => {
										self.show_info = true;
									}
									KeyCode::Enter => {
										if let Some(idx) = self.selected_table_index() {
											let table_id = self.tables[idx].id.clone();
											self.backend.send(LobbyCommand::JoinTable(table_id));
										}
									}
									_ => {}
								}
							}
							MenuState::Lobby => {
								match key.code {
									KeyCode::Esc => {
										self.backend.send(LobbyCommand::LeaveTable);
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
										let max = self.players.len();
										if self.lobby_cursor < max {
											self.lobby_cursor += 1;
										}
									}
									KeyCode::Char(' ') | KeyCode::Char('a') => {
										self.backend.send(LobbyCommand::AddAI);
									}
									KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
										if let Some(player) = self.players.get(self.lobby_cursor) {
											if !player.is_host && !player.is_human {
												if let Some(seat) = player.seat {
													self.backend.send(LobbyCommand::RemoveAI(seat));
												}
											}
										}
									}
									KeyCode::Enter => {
										if self.can_start() {
											self.backend.send(LobbyCommand::Ready);
											if self.all_ready() {
												self.backend.send(LobbyCommand::StartGame);
											}
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
	}

	fn move_table_selection(&mut self, delta: i32) {
		let len = self.sorted_indices.len();
		if len == 0 {
			return;
		}
		// Selection range is 1..=len (0 is header)
		let current = self.table_list_state.selected().unwrap_or(1) as i32;
		let new = ((current - 1 + delta).rem_euclid(len as i32) + 1) as usize;
		self.table_list_state.select(Some(new));
	}

	fn draw(&self, frame: &mut Frame) {
		match &self.state {
			MenuState::TableSelect => self.draw_table_select(frame),
			MenuState::Lobby => self.draw_lobby(frame),
		}

		if self.show_info {
			self.draw_info_popup(frame);
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

		let host_bankroll = self.backend.get_bankroll(&self.host_id);
		let header_text = if host_bankroll > 0.0 {
			format!(
				"  Transparent Poker                            Bankroll: ${:.0}",
				host_bankroll
			)
		} else {
			"  Transparent Poker".to_string()
		};
		let header = Paragraph::new(header_text)
			.style(Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD))
			.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(header, chunks[0]);

		// Column header
		let header_line = Line::from(vec![
			Span::styled(
				format!("{:<24}", "Table"),
				Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD),
			),
			Span::styled(
				format!("{:<11}", "Status"),
				Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD),
			),
			Span::styled(
				format!("{:<6}", "Type"),
				Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD),
			),
			Span::styled(
				format!("{:<7}", "Limit"),
				Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD),
			),
			Span::styled(
				format!("{:<10}", "Stakes"),
				Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD),
			),
			Span::styled(
				format!("{:<8}", "Buy-in"),
				Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD),
			),
			Span::styled(
				"Players",
				Style::default().fg(self.theme.menu_title()).add_modifier(Modifier::BOLD),
			),
		]);

		let mut items: Vec<ListItem> = vec![ListItem::new(header_line)];

		items.extend(self
			.sorted_indices
			.iter()
			.map(|&idx| {
				let t = &self.tables[idx];
				let (status_text, status_color) = match t.status {
					TableStatus::Waiting => ("Open", self.theme.stack()),
					TableStatus::InProgress => ("In Progress", self.theme.bet()),
					TableStatus::Finished => ("Finished", self.theme.menu_unselected()),
				};
				let format_abbrev = match t.format.as_str() {
					"Sit & Go" => "SnG",
					other => other,
				};
				let betting_abbrev = match t.betting.as_str() {
					"No-Limit" => "NL",
					"Pot-Limit" => "PL",
					"Fixed-Limit" => "Fixed",
					other => other,
				};
				let line = Line::from(vec![
					Span::styled(
						format!("{:<24}", truncate_str(&t.name, 24)),
						Style::default().fg(self.theme.menu_text()),
					),
					Span::styled(
						format!("{:<11}", status_text),
						Style::default().fg(status_color),
					),
					Span::styled(
						format!("{:<6}", format_abbrev),
						Style::default().fg(self.theme.menu_unselected()),
					),
					Span::styled(
						format!("{:<7}", betting_abbrev),
						Style::default().fg(self.theme.menu_highlight()),
					),
					Span::styled(
						format!("{:>10}", t.blinds),
						Style::default().fg(self.theme.menu_highlight()),
					),
					Span::styled(
						format!("{:>8}", t.buy_in),
						Style::default().fg(self.theme.bet()),
					),
					Span::styled(
						format!("{:>3}/{:<3}", t.players, t.max_players),
						Style::default().fg(self.theme.menu_unselected()),
					),
				]);
				ListItem::new(line)
			}));

		let title = format!(" SELECT TABLE (sort: {}) ", self.sort_mode.label());
		let list = List::new(items)
			.block(
				Block::default()
					.title(title)
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

		let help = Paragraph::new("  [↑/↓] Select  [←/→] Sort  [Enter] Open Lobby  [i] Info  [q] Quit")
			.style(Style::default().fg(self.theme.menu_unselected()))
			.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(help, chunks[2]);
	}

	fn draw_lobby(&self, frame: &mut Frame) {
		let area = frame.area();

		let bg = Block::default().style(Style::default().bg(self.theme.background()));
		frame.render_widget(bg, area);

		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(3),
				Constraint::Min(8),
				Constraint::Length(3),
			])
			.split(area);

		let header_text = if let Some(ref err) = self.error_message {
			format!("  TABLE: {} - {}", self.current_table_name, err)
		} else {
			format!("  TABLE: {}", self.current_table_name)
		};
		let header_color = if self.error_message.is_some() {
			self.theme.status_quit()
		} else {
			self.theme.menu_title()
		};
		let header = Paragraph::new(header_text)
			.style(Style::default().fg(header_color).add_modifier(Modifier::BOLD))
			.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(header, chunks[0]);

		let player_lines = self.build_player_list();
		let player_list = Paragraph::new(player_lines)
			.block(
				Block::default()
					.title(format!(
						" PLAYERS ({}/{}) ",
						self.players.len(),
						self.max_players
					))
					.borders(Borders::ALL)
					.border_style(Style::default().fg(self.theme.menu_border())),
			);
		frame.render_widget(player_list, chunks[1]);

		let can_start = self.can_start();
		let help_text = if can_start {
			"  [Enter] Start game  [a] Add AI player  [d] Remove player  [Esc] Back  [q] Quit"
		} else {
			Box::leak(format!(
				"  Need {} more players  [a] Add AI  [Esc] Back  [q] Quit",
				self.min_players.saturating_sub(self.players.len())
			).into_boxed_str())
		};
		let help = Paragraph::new(help_text)
			.style(Style::default().fg(self.theme.menu_unselected()))
			.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.theme.menu_border())));
		frame.render_widget(help, chunks[2]);
	}

	fn draw_info_popup(&self, frame: &mut Frame) {
		let Some(idx) = self.selected_table_index() else {
			return;
		};
		let table = &self.tables[idx];

		let info_str = if let Some(config) = self.backend.table_config(&table.id) {
			match toml::to_string_pretty(&config) {
				Ok(s) => s,
				Err(_) => "Failed to serialize table config".to_string(),
			}
		} else {
			format!(
				"ID: {}\nFormat: {}\nBetting: {}\nBlinds: {}\nBuy-in: {}\nPlayers: {}/{}",
				table.id, table.format, table.betting, table.blinds, table.buy_in, table.players, table.max_players
			)
		};

		let area = frame.area();
		let popup_width = (area.width * 2 / 3).min(60);
		let popup_height = (area.height * 2 / 3).min(20);
		let popup_x = (area.width - popup_width) / 2;
		let popup_y = (area.height - popup_height) / 2;
		let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

		frame.render_widget(Clear, popup_area);

		let popup = Paragraph::new(info_str)
			.style(Style::default().fg(self.theme.menu_text()))
			.wrap(Wrap { trim: false })
			.block(
				Block::default()
					.title(format!(" {} ", table.name))
					.borders(Borders::ALL)
					.border_style(Style::default().fg(self.theme.menu_highlight()))
					.style(Style::default().bg(self.theme.background())),
			);
		frame.render_widget(popup, popup_area);
	}

	fn build_player_list(&self) -> Vec<Line<'static>> {
		let mut lines = Vec::new();

		for (i, player) in self.players.iter().enumerate() {
			let cursor = if i == self.lobby_cursor { "> " } else { "  " };
			let host_tag = if player.is_host { " (host)" } else { "" };
			let ready_tag = if player.is_ready { " ✓" } else { "" };

			let bankroll_str = if let Some(br) = player.bankroll {
				format!("${:.0}", br)
			} else {
				String::new()
			};

			let name_color = if player.is_host {
				self.theme.menu_host_marker()
			} else if !player.is_human {
				self.theme.menu_ai_marker()
			} else {
				self.theme.menu_text()
			};

			lines.push(Line::from(vec![
				Span::raw(cursor),
				Span::styled(
					format!("{:<20}", format!("{}{}{}", player.name, host_tag, ready_tag)),
					Style::default().fg(name_color),
				),
				Span::styled(
					format!("{:<12}", bankroll_str),
					Style::default().fg(self.theme.bet()),
				),
			]));
		}

		if self.players.len() < self.max_players {
			let cursor = if self.lobby_cursor == self.players.len() {
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

fn flush_keyboard_buffer() {
	while event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
		let _ = event::read();
	}
}

fn truncate_str(s: &str, max_len: usize) -> String {
	if s.len() <= max_len {
		s.to_string()
	} else if max_len <= 1 {
		"…".to_string()
	} else {
		// Find a safe char boundary
		let mut end = max_len - 1;
		while end > 0 && !s.is_char_boundary(end) {
			end -= 1;
		}
		format!("{}…", &s[..end])
	}
}
