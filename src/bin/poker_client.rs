use std::io::{self, stdout};
use std::time::Duration;

use clap::Parser;
use crossterm::{
	event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
	execute,
	terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
	backend::CrosstermBackend,
	layout::{Constraint, Direction, Layout},
	style::{Color, Modifier, Style},
	widgets::{Block, Borders, List, ListItem, Paragraph},
	Frame, Terminal,
};

use transparent_poker::events::{GameEvent, Seat};
use transparent_poker::net::{GameClient, PlayerInfo, ServerMessage, TableInfo, TableStatus};
use transparent_poker::players::PlayerResponse;
use transparent_poker::theme::Theme;
use transparent_poker::tui::{GameUI, GameUIAction};

#[derive(Parser)]
#[command(name = "poker-client")]
#[command(about = "Connect to a poker server")]
struct Cli {
	#[arg(short, long, default_value = "127.0.0.1:9999")]
	server: String,

	#[arg(short, long)]
	username: Option<String>,

	#[arg(short, long, env = "POKER_THEME")]
	theme: Option<String>,
}

#[derive(PartialEq)]
enum Screen {
	Lobby,
	Table,
	Game,
}

struct App {
	screen: Screen,
	client: GameClient,
	username: String,

	// Lobby state
	tables: Vec<TableInfo>,
	selected_table: usize,

	// Table (waiting room) state
	current_table: Option<String>,
	table_name: String,
	players: Vec<PlayerInfo>,
	is_ready: bool,
	my_seat: Option<Seat>,

	// Game state
	game_ui: Option<GameUI>,
	game_seat: Option<Seat>,
	theme_name: String,
}

impl App {
	fn new(client: GameClient, username: String, theme: Theme, theme_name: String) -> Self {
		Self {
			screen: Screen::Lobby,
			client,
			username,
			tables: Vec::new(),
			selected_table: 0,
			current_table: None,
			table_name: String::new(),
			players: Vec::new(),
			is_ready: false,
			my_seat: None,
			game_ui: Some(GameUI::new(None, theme, theme_name.clone())),
			game_seat: None,
			theme_name,
		}
	}

	fn process_messages(&mut self) {
		while let Some(msg) = self.client.try_recv() {
			match msg {
				ServerMessage::LobbyState { tables } => {
					self.tables = tables;
				}
				ServerMessage::TableJoined { table_id, table_name, seat, players, .. } => {
					self.current_table = Some(table_id);
					self.table_name = table_name;
					self.players = players;
					self.my_seat = Some(seat);
					self.is_ready = false;
					self.screen = Screen::Table;
				}
				ServerMessage::PlayerJoinedTable { seat, username } => {
					self.players.push(PlayerInfo { seat, username, ready: false });
				}
				ServerMessage::PlayerLeftTable { seat, .. } => {
					self.players.retain(|p| p.seat != seat);
				}
				ServerMessage::PlayerReady { seat } => {
					if let Some(p) = self.players.iter_mut().find(|p| p.seat == seat) {
						p.ready = true;
					}
					if Some(seat) == self.my_seat {
						self.is_ready = true;
					}
				}
				ServerMessage::GameStarting { .. } => {
					// Game is about to start
				}
				ServerMessage::GameEvent(event) => {
					self.handle_game_event(event);
				}
				ServerMessage::ActionRequest { valid_actions, .. } => {
					if let Some(ref mut ui) = self.game_ui {
						ui.enter_action_mode(valid_actions);
					}
				}
				ServerMessage::Error { message } => {
					if let Some(ref mut ui) = self.game_ui {
						ui.status_message = Some(format!("Error: {}", message));
					}
				}
				_ => {}
			}
		}
	}

	fn handle_game_event(&mut self, event: GameEvent) {
		// On first HandStarted, determine our game seat by matching username
		if let GameEvent::HandStarted { seats, .. } = &event {
			if self.game_seat.is_none() {
				let found_seat = seats.iter()
					.find(|s| s.name.eq_ignore_ascii_case(&self.username))
					.map(|s| s.seat);
				
				if let Some(seat) = found_seat {
					self.game_seat = Some(seat);
					// Recreate GameUI with correct hero seat
					if let Some(ref old_ui) = self.game_ui {
						let theme = old_ui.theme.clone();
						self.game_ui = Some(GameUI::new(Some(seat), theme, self.theme_name.clone()));
					}
				}
			}
		}

		if let GameEvent::GameCreated { .. } = &event {
			self.screen = Screen::Game;
		}

		if let Some(ref mut ui) = self.game_ui {
			ui.apply_event(&event);
		}
	}
}

fn main() -> io::Result<()> {
	let cli = Cli::parse();

	let theme_name = cli.theme.as_deref().unwrap_or("classic").to_string();
	let theme = Theme::load_named(&theme_name).unwrap_or_default();

	println!("Connecting to {}...", cli.server);
	let mut client = GameClient::connect(&cli.server)?;

	let username = cli.username.unwrap_or_else(|| {
		std::env::var("USER")
			.or_else(|_| std::env::var("USERNAME"))
			.unwrap_or_else(|_| "Player".to_string())
	});

	client.login(&username)?;
	std::thread::sleep(Duration::from_millis(100));
	client.list_tables()?;

	let mut app = App::new(client, username, theme, theme_name);

	enable_raw_mode()?;
	let mut stdout = stdout();
	execute!(stdout, EnterAlternateScreen)?;
	let backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(backend)?;

	let result = run_app(&mut terminal, &mut app);

	disable_raw_mode()?;
	execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

	result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
	loop {
		app.process_messages();

		terminal.draw(|f| {
			match app.screen {
				Screen::Lobby => draw_lobby(f, app),
				Screen::Table => draw_table_waiting(f, app),
				Screen::Game => {
					if let Some(ref ui) = app.game_ui {
						ui.render(f, f.area());
					}
				}
			}
		})?;

		if event::poll(Duration::from_millis(50))? {
			if let Event::Key(key) = event::read()? {
				if key.kind != KeyEventKind::Press {
					continue;
				}

				// Global quit
				if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
					return Ok(());
				}
				if key.code == KeyCode::Char('q') && app.screen != Screen::Game {
					return Ok(());
				}

				match app.screen {
					Screen::Lobby => handle_lobby_input(app, key.code),
					Screen::Table => handle_table_input(app, key.code),
					Screen::Game => {
						if let Some(ref mut ui) = app.game_ui {
							match ui.handle_key(key.code) {
								GameUIAction::Respond(PlayerResponse::Action(action)) => {
									let _ = app.client.action(action);
								}
								GameUIAction::Quit => {
									return Ok(());
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

fn handle_lobby_input(app: &mut App, key: KeyCode) {
	match key {
		KeyCode::Up | KeyCode::Char('k') => {
			if app.selected_table > 0 {
				app.selected_table -= 1;
			}
		}
		KeyCode::Down | KeyCode::Char('j') => {
			if app.selected_table < app.tables.len().saturating_sub(1) {
				app.selected_table += 1;
			}
		}
		KeyCode::Enter => {
			if let Some(table) = app.tables.get(app.selected_table) {
				let _ = app.client.join_table(&table.id);
			}
		}
		KeyCode::Char('r') => {
			let _ = app.client.list_tables();
		}
		_ => {}
	}
}

fn handle_table_input(app: &mut App, key: KeyCode) {
	match key {
		KeyCode::Char('r') => {
			if !app.is_ready {
				let _ = app.client.ready();
			}
		}
		KeyCode::Char('l') | KeyCode::Esc => {
			let _ = app.client.leave_table();
			app.current_table = None;
			app.screen = Screen::Lobby;
			let _ = app.client.list_tables();
		}
		_ => {}
	}
}

fn draw_lobby(f: &mut Frame, app: &App) {
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Length(3),
			Constraint::Min(10),
			Constraint::Length(3),
		])
		.split(f.area());

	// Header
	let header = Paragraph::new(format!("♠ ♥ Poker Lobby ♦ ♣  -  {}", app.username))
		.style(Style::default().fg(Color::Green))
		.block(Block::default().borders(Borders::ALL));
	f.render_widget(header, chunks[0]);

	// Table list
	let items: Vec<ListItem> = app.tables.iter().enumerate().map(|(i, t)| {
		let style = if i == app.selected_table {
			Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
		} else {
			Style::default()
		};
		let status = match t.status {
			TableStatus::Waiting => "Waiting",
			TableStatus::InProgress => "In Progress",
			TableStatus::Finished => "Finished",
		};
		let line = format!(
			"{} {} - {} {} ({}/{}) [{}]",
			if i == app.selected_table { ">" } else { " " },
			t.name,
			t.format,
			t.blinds,
			t.players,
			t.max_players,
			status
		);
		ListItem::new(line).style(style)
	}).collect();

	let list = List::new(items)
		.block(Block::default().title("Tables").borders(Borders::ALL));
	f.render_widget(list, chunks[1]);

	// Help
	let help = Paragraph::new("↑↓ Navigate  Enter Join  R Refresh  Q Quit")
		.style(Style::default().fg(Color::DarkGray))
		.block(Block::default().borders(Borders::ALL));
	f.render_widget(help, chunks[2]);
}

fn draw_table_waiting(f: &mut Frame, app: &App) {
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Length(3),
			Constraint::Min(10),
			Constraint::Length(3),
		])
		.split(f.area());

	// Header
	let header = Paragraph::new(format!("Table: {}", app.table_name))
		.style(Style::default().fg(Color::Green))
		.block(Block::default().borders(Borders::ALL));
	f.render_widget(header, chunks[0]);

	// Player list
	let items: Vec<ListItem> = app.players.iter().map(|p| {
		let ready_str = if p.ready { "✓ Ready" } else { "  Waiting" };
		let is_me = Some(p.seat) == app.my_seat;
		let style = if is_me {
			Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
		} else if p.ready {
			Style::default().fg(Color::Green)
		} else {
			Style::default()
		};
		let line = format!(
			"Seat {}: {} {} {}",
			p.seat.0,
			p.username,
			ready_str,
			if is_me { "(you)" } else { "" }
		);
		ListItem::new(line).style(style)
	}).collect();

	let list = List::new(items)
		.block(Block::default().title("Players").borders(Borders::ALL));
	f.render_widget(list, chunks[1]);

	// Help
	let help_text = if app.is_ready {
		"Waiting for other players...  L Leave  Q Quit"
	} else {
		"R Ready  L Leave  Q Quit"
	};
	let help = Paragraph::new(help_text)
		.style(Style::default().fg(Color::DarkGray))
		.block(Block::default().borders(Borders::ALL));
	f.render_widget(help, chunks[2]);
}
