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

use transparent_poker::events::{GameEvent, Seat, ViewUpdater};
use transparent_poker::net::{GameClient, PlayerInfo, ServerMessage, TableInfo};
use transparent_poker::theme::Theme;
use transparent_poker::tui::{InputEffect, InputState, TableWidget};
use transparent_poker::view::TableView;

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
	Login,
	Lobby,
	Table,
	Game,
}

struct App {
	screen: Screen,
	client: GameClient,
	username: String,
	theme: Theme,

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
	table_view: TableView,
	view_updater: Option<ViewUpdater>,
	input_state: InputState,
	status_message: Option<String>,
}

impl App {
	fn new(client: GameClient, username: String, theme: Theme) -> Self {
		Self {
			screen: Screen::Lobby,
			client,
			username,
			theme,
			tables: Vec::new(),
			selected_table: 0,
			current_table: None,
			table_name: String::new(),
			players: Vec::new(),
			is_ready: false,
			my_seat: None,
			table_view: TableView::new(),
			view_updater: None,
			input_state: InputState::default(),
			status_message: None,
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
					self.status_message = Some("Game starting...".to_string());
				}
				ServerMessage::GameEvent(event) => {
					self.handle_game_event(event);
				}
				ServerMessage::ActionRequest { valid_actions, .. } => {
					let (state, effect) = InputState::enter_action_mode(valid_actions);
					self.input_state = state;
					self.execute_effect(effect);
				}
				ServerMessage::Error { message } => {
					self.status_message = Some(format!("Error: {}", message));
				}
				_ => {}
			}
		}
	}

	fn handle_game_event(&mut self, event: GameEvent) {
		match &event {
			GameEvent::GameCreated { .. } => {
				self.screen = Screen::Game;
				self.view_updater = Some(ViewUpdater::new(self.my_seat));
				self.table_view = TableView::new();
			}
			GameEvent::HandStarted { .. } => {
				// Don't reset table_view - ViewUpdater handles resetting fields
				// This preserves chat history from previous hand
				self.input_state = InputState::default();
			}
			_ => {}
		}

		if let Some(ref updater) = self.view_updater {
			updater.apply(&mut self.table_view, &event);
		}
	}

	fn execute_effect(&mut self, effect: InputEffect) {
		match effect {
			InputEffect::None => {}
			InputEffect::SetPrompt(prompt) => {
				self.status_message = Some(prompt);
			}
			InputEffect::ClearPrompt => {
				self.status_message = None;
			}
			InputEffect::Respond(response) => {
				if let transparent_poker::players::PlayerResponse::Action(action) = response {
					let _ = self.client.action(action);
				}
				self.status_message = None;
				self.input_state = InputState::default();
			}
			InputEffect::CycleTheme => {
				// Could implement theme cycling
			}
			InputEffect::Quit => {
				// Handle quit - back to lobby or exit
			}
		}
	}
}

fn main() -> io::Result<()> {
	let cli = Cli::parse();

	let theme = Theme::load_named(cli.theme.as_deref().unwrap_or("classic"))
		.unwrap_or_default();

	println!("Connecting to {}...", cli.server);
	let mut client = GameClient::connect(&cli.server)?;

	let username = cli.username.unwrap_or_else(|| {
		std::env::var("USER")
			.or_else(|_| std::env::var("USERNAME"))
			.unwrap_or_else(|_| "Player".to_string())
	});

	client.login(&username)?;
	// Wait for welcome
	std::thread::sleep(Duration::from_millis(100));

	client.list_tables()?;

	let mut app = App::new(client, username, theme);

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
				Screen::Login => draw_login(f, app),
				Screen::Lobby => draw_lobby(f, app),
				Screen::Table => draw_table_waiting(f, app),
				Screen::Game => draw_game(f, app),
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
					Screen::Login => {}
					Screen::Lobby => handle_lobby_input(app, key.code),
					Screen::Table => handle_table_input(app, key.code),
					Screen::Game => handle_game_input(app, key.code),
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

fn handle_game_input(app: &mut App, key: KeyCode) {
	let (new_state, effect) = app.input_state.clone().handle_key(key);
	app.input_state = new_state;
	app.execute_effect(effect);
}

fn draw_login(f: &mut Frame, _app: &App) {
	let block = Block::default().title("Login").borders(Borders::ALL);
	f.render_widget(block, f.area());
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
			transparent_poker::net::TableStatus::Waiting => "Waiting",
			transparent_poker::net::TableStatus::InProgress => "In Progress",
			transparent_poker::net::TableStatus::Finished => "Finished",
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

fn draw_game(f: &mut Frame, app: &App) {
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Min(20),
			Constraint::Length(3),
		])
		.split(f.area());

	// Table
	let table_widget = TableWidget::new(&app.table_view, &app.theme);
	f.render_widget(table_widget, chunks[0]);

	// Action bar
	let action_text = app.status_message.clone().unwrap_or_default();
	let action_bar = Paragraph::new(action_text)
		.style(Style::default().fg(Color::Yellow))
		.block(Block::default().borders(Borders::ALL).title("Action"));
	f.render_widget(action_bar, chunks[1]);
}
