use std::io::{self, stdout};
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use crossterm::{
	event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
	execute,
	terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use rand::seq::SliceRandom;
use ratatui::{
	backend::CrosstermBackend,
	layout::{Constraint, Direction, Layout},
	style::{Modifier, Style},
	widgets::{Block, Borders, Paragraph},
	Frame, Terminal,
};

use transparent_poker::bank::Bank;
use transparent_poker::config::{load_players_auto, load_strategies_auto};
use transparent_poker::engine::{BettingStructure, GameRunner, RunnerConfig};
use transparent_poker::events::{GameEvent, Seat, Standing, ViewUpdater};
use transparent_poker::logging::{self, tui as log};
use transparent_poker::lobby::{LocalBackend, LobbyPlayer, NetworkBackend};
use transparent_poker::menu::{Menu, MenuResult};
use transparent_poker::net::{GameClient, GameServer, ServerMessage};
use transparent_poker::players::{ActionRequest, PlayerResponse, RulesPlayer, TerminalPlayer};
use transparent_poker::table::{load_tables, BlindClock, GameFormat, TableConfig};
use transparent_poker::theme::Theme;
use transparent_poker::tui::{GameUI, GameUIAction, InputEffect, InputState, TableWidget};
use transparent_poker::view::TableView;

#[derive(Parser)]
#[command(name = "poker")]
#[command(about = "Transparent poker - play Texas Hold'em against AI opponents")]
#[command(version)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	#[command(about = "Start the game")]
	Play {
		#[arg(short, long, env = "POKER_USER")]
		#[arg(help = "Player name (must exist in profiles for local, or username for network)")]
		player: Option<String>,

		#[arg(short, long, env = "POKER_THEME")]
		#[arg(help = "Color theme")]
		theme: Option<String>,

		#[arg(long)]
		#[arg(help = "RNG seed for reproducible games")]
		seed: Option<u64>,

		#[arg(short, long)]
		#[arg(help = "Connect to server (e.g., localhost:9999)")]
		server: Option<String>,
	},

	#[command(about = "Run a poker server")]
	Server {
		#[arg(short, long, default_value = "127.0.0.1:9999")]
		#[arg(help = "Address to bind")]
		bind: String,
	},

	#[command(about = "List available color themes")]
	Themes,

	#[command(about = "Register a new player")]
	Register {
		#[arg(help = "Player name")]
		name: String,

		#[arg(short, long, default_value = "10000")]
		#[arg(help = "Starting bankroll")]
		bankroll: f32,
	},

	#[command(about = "List all registered players")]
	Players,

	#[command(about = "Manage player bankroll")]
	Bankroll {
		#[arg(help = "Player name")]
		name: String,

		#[command(subcommand)]
		action: BankrollAction,
	},
}

#[derive(Subcommand)]
enum BankrollAction {
	#[command(about = "Show current bankroll")]
	Show,

	#[command(about = "Set bankroll to specific amount")]
	Set {
		#[arg(help = "New bankroll amount")]
		amount: f32,
	},

	#[command(about = "Add to bankroll")]
	Add {
		#[arg(help = "Amount to add")]
		amount: f32,
	},

	#[command(about = "Subtract from bankroll")]
	Sub {
		#[arg(help = "Amount to subtract")]
		amount: f32,
	},
}

const WINNER_HIGHLIGHT_MS: u64 = 2000;

#[derive(Clone, Copy)]
struct DelayConfig {
	action_ms: u64,
	street_ms: u64,
	hand_end_ms: u64,
}

impl DelayConfig {
	fn from_table(table: &TableConfig) -> Self {
		Self {
			action_ms: table.action_delay_ms,
			street_ms: table.street_delay_ms,
			hand_end_ms: table.hand_end_delay_ms,
		}
	}
}

fn list_themes() {
	let themes = Theme::list_available();
	println!("Available themes:");
	for theme in themes {
		println!("  {}", theme);
	}
	println!("\nUsage: poker play --theme <name>");
	println!("Or set POKER_THEME environment variable");
	println!("\nCustom themes: add .toml files to your config directory's themes/ folder");
}

fn resolve_player_id(player_arg: Option<&str>, bank: &Bank) -> Result<String, String> {
	if let Some(name) = player_arg {
		if bank.profile_exists(name) {
			return Ok(name.to_string());
		} else {
			return Err(format!(
				"Player '{}' not found. Use 'poker register {}' to create.",
				name, name
			));
		}
	}

	Err("No player specified. Use 'poker play --player <n>' or set POKER_USER.".to_string())
}

struct WinnerInfo {
	seat: Seat,
	amount: f32,
	description: Option<String>,
	awarded_at: Instant,
}

fn build_info_lines(table: &TableConfig, num_players: usize, seed: Option<u64>) -> Vec<String> {
	let mut lines = vec![
		format!("Format: {}", table.format),
		format!("Betting: {}", table.betting),
		String::new(),
	];

	match table.format {
		GameFormat::Cash => {
			if let (Some(sb), Some(bb)) = (table.small_blind, table.big_blind) {
				lines.push(format!("Blinds: ${:.0}/${:.0}", sb, bb));
			}
			if let Some(min) = table.min_buy_in {
				lines.push(format!("Min Buy-in: ${:.0}", min));
			}
			if let Some(max) = table.max_buy_in {
				lines.push(format!("Max Buy-in: ${:.0}", max));
			}
			lines.push(String::new());
			lines.push(format!("Players: {}", num_players));
			if table.rake_percent > 0.0 {
				let rake_str = if let Some(cap) = table.rake_cap {
					format!("Rake: {:.1}% (${:.0} cap)", table.rake_percent * 100.0, cap)
				} else {
					format!("Rake: {:.1}%", table.rake_percent * 100.0)
				};
				lines.push(rake_str);
			}
		}
		GameFormat::SitNGo => {
			if let Some(buyin) = table.buy_in {
				lines.push(format!("Buy-in: ${:.0}", buyin));
			}
			if let Some(stack) = table.starting_stack {
				lines.push(format!("Starting Stack: ${:.0}", stack));
			}
			lines.push(String::new());
			lines.push(format!("Players: {}", num_players));
			if let (Some(payouts), Some(buyin)) = (&table.payouts, table.buy_in) {
				let prize_pool = buyin * num_players as f32;
				let payout_strs: Vec<String> = payouts
					.iter()
					.map(|p| format!("${:.0}", (prize_pool * p).round()))
					.collect();
				lines.push(format!("Payouts: {}", payout_strs.join(", ")));
			}
		}
	}

	if let Some(s) = seed {
		lines.push(String::new());
		lines.push(format!("Seed: {}", s));
	}

	lines
}

// ============================================================================
// Network Play Mode
// ============================================================================

// ============================================================================
// Local Play Mode
// ============================================================================

struct App {
	table_view: TableView,
	view_updater: ViewUpdater,
	input_state: InputState,
	pending_response_tx: Option<mpsc::Sender<PlayerResponse>>,
	status_message: Option<String>,
	last_winners: Vec<WinnerInfo>,
	human_seat: Seat,
	final_standings: Vec<Standing>,
	table_config: TableConfig,
	info_lines: Vec<String>,
	theme: Theme,
	theme_name: String,
	quit_signal: Arc<AtomicBool>,
}

impl App {
	fn new(
		human_seat: Seat,
		table_config: TableConfig,
		num_players: usize,
		effective_seed: Option<u64>,
		theme: Theme,
		theme_name: String,
		quit_signal: Arc<AtomicBool>,
	) -> Self {
		let table_info = format!("{} {}", table_config.betting, table_config.format);
		let table_view = TableView::new().with_table_info(table_config.name.clone(), table_info);
		let info_lines = build_info_lines(&table_config, num_players, effective_seed);
		Self {
			table_view,
			view_updater: ViewUpdater::new(Some(human_seat)),
			input_state: InputState::default(),
			pending_response_tx: None,
			status_message: None,
			last_winners: Vec::new(),
			human_seat,
			final_standings: Vec::new(),
			table_config,
			info_lines,
			theme,
			theme_name,
			quit_signal,
		}
	}

	fn apply_event(&mut self, event: &GameEvent) {
		log::event(&format!("{:?}", event));
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
				self.execute_effect(effect);
			}
			_ => {}
		}
	}

	fn update_winner_highlights(&mut self) {
		let now = Instant::now();
		let highlight_duration = Duration::from_millis(WINNER_HIGHLIGHT_MS);
		self.table_view.winner_seats = self.last_winners
			.iter()
			.filter(|w| now.duration_since(w.awarded_at) < highlight_duration)
			.map(|w| w.seat.0)
			.collect();
	}

	fn enter_action_mode(&mut self, request: ActionRequest) {
		log::event(&format!(
			"entering action mode: seat={} can_fold={} can_check={} call={:?}",
			request.seat.0,
			request.valid_actions.can_fold,
			request.valid_actions.can_check,
			request.valid_actions.call_amount
		));

		self.pending_response_tx = Some(request.response_tx);
		let (state, effect) = InputState::enter_action_mode(request.valid_actions);
		self.input_state = state;
		self.execute_effect(effect);
	}

	fn execute_effect(&mut self, effect: InputEffect) -> bool {
		match effect {
			InputEffect::None => false,

			InputEffect::SetPrompt(prompt) => {
				self.status_message = Some(prompt);
				false
			}

			InputEffect::ClearPrompt => {
				self.status_message = None;
				false
			}

			InputEffect::Respond(response) => {
				if let Some(tx) = self.pending_response_tx.take() {
					log::action(&format!("{:?}", response));
					let _ = tx.send(response);
				}
				self.status_message = None;
				false
			}

			InputEffect::CycleTheme => {
				self.cycle_theme();
				false
			}

			InputEffect::Quit => {
				self.quit_signal.store(true, Ordering::SeqCst);
				true
			}
		}
	}

	fn cycle_theme(&mut self) {
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

	fn handle_input(&mut self, key: KeyCode) -> bool {
		log::input(&format!("{:?}", key));
		let old_state = std::mem::take(&mut self.input_state);
		let (new_state, effect) = old_state.handle_key(key);
		self.input_state = new_state;
		self.execute_effect(effect)
	}
}

fn flush_keyboard_buffer() {
	while event::poll(Duration::from_millis(0)).unwrap_or(false) {
		let _ = event::read();
	}
}

fn interruptible_sleep(duration: Duration) -> io::Result<bool> {
	let deadline = Instant::now() + duration;
	while Instant::now() < deadline {
		let remaining = deadline.saturating_duration_since(Instant::now());
		let poll_time = remaining.min(Duration::from_millis(50));
		if event::poll(poll_time)? {
			if let Event::Key(key) = event::read()? {
				if key.kind == KeyEventKind::Press
					&& (key.code == KeyCode::Char('q') || key.code == KeyCode::Esc)
				{
					return Ok(true);
				}
			}
		}
	}
	Ok(false)
}

fn main() -> io::Result<()> {
	transparent_poker::defaults::ensure_config();
	let cli = Cli::parse();

	match cli.command {
		Commands::Themes => {
			list_themes();
			Ok(())
		}

		Commands::Register { name, bankroll } => {
			cmd_register(&name, bankroll)
		}

		Commands::Players => {
			cmd_list_players()
		}

		Commands::Bankroll { name, action } => {
			cmd_bankroll(&name, action)
		}

		Commands::Server { bind } => {
			cmd_server(&bind)
		}

		Commands::Play { player, theme, seed, server } => {
			if let Some(addr) = server {
				cmd_play_network(player, theme, &addr)
			} else {
				cmd_play_local(player, theme, seed)
			}
		}
	}
}

fn cmd_register(name: &str, bankroll: f32) -> io::Result<()> {
	let mut bank = Bank::load().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

	if bank.profile_exists(name) {
		eprintln!("Player '{}' already exists.", name);
		return Ok(());
	}

	bank.register(name, bankroll);
	bank.save().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

	println!("Registered '{}' with bankroll ${:.0}", name, bankroll);
	Ok(())
}

fn cmd_list_players() -> io::Result<()> {
	let bank = Bank::load().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

	let players = bank.list_players();
	if players.is_empty() {
		println!("No registered players.");
		println!("Use 'poker register <name>' to create one.");
		return Ok(());
	}

	println!("{:<20} {:>12}", "Player", "Bankroll");
	println!("{}", "-".repeat(34));
	for (name, profile) in players {
		println!("{:<20} ${:>11.0}", name, profile.bankroll);
	}

	Ok(())
}

fn cmd_bankroll(name: &str, action: BankrollAction) -> io::Result<()> {
	let mut bank = Bank::load().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

	if !bank.profile_exists(name) {
		eprintln!("Player '{}' not found. Use 'poker register {}' first.", name, name);
		return Ok(());
	}

	match action {
		BankrollAction::Show => {
			let balance = bank.get_bankroll(name);
			println!("{}: ${:.0}", name, balance);
		}
		BankrollAction::Set { amount } => {
			let current = bank.get_bankroll(name);
			if amount > current {
				bank.credit(name, amount - current);
			} else {
				bank.debit(name, current - amount)
					.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
			}
			bank.save().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
			println!("{}: ${:.0}", name, amount);
		}
		BankrollAction::Add { amount } => {
			bank.credit(name, amount);
			bank.save().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
			let new_balance = bank.get_bankroll(name);
			println!("{}: ${:.0} (+{:.0})", name, new_balance, amount);
		}
		BankrollAction::Sub { amount } => {
			bank.debit(name, amount)
				.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
			bank.save().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
			let new_balance = bank.get_bankroll(name);
			println!("{}: ${:.0} (-{:.0})", name, new_balance, amount);
		}
	}

	Ok(())
}

fn cmd_server(bind: &str) -> io::Result<()> {
	println!("Starting poker server on {}...", bind);
	let server = GameServer::new();
	server.run(bind)
}

fn cmd_play_network(player: Option<String>, theme: Option<String>, addr: &str) -> io::Result<()> {
	let theme_name = theme
		.clone()
		.or_else(|| std::env::var("POKER_THEME").ok())
		.unwrap_or_else(|| "classic".to_string());
	let theme = Theme::load_named(&theme_name).unwrap_or_default();

	println!("Connecting to {}...", addr);
	let mut client = GameClient::connect(addr)?;

	let username = player.unwrap_or_else(|| {
		std::env::var("USER")
			.or_else(|_| std::env::var("USERNAME"))
			.unwrap_or_else(|_| "Player".to_string())
	});

	client.login(&username)?;
	std::thread::sleep(Duration::from_millis(100));

	let backend = NetworkBackend::new(client);
	let mut menu = Menu::new(backend, username.clone(), theme.clone());

	enable_raw_mode()?;
	let mut stdout = stdout();
	execute!(stdout, EnterAlternateScreen)?;
	let backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(backend)?;

	let result = menu.run(&mut terminal);

	match result {
		Ok(MenuResult::Quit) => {
			disable_raw_mode()?;
			execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
			Ok(())
		}
		Ok(MenuResult::NetworkGameStarted { seat }) => {
			let mut client = menu.into_backend().into_client();
			run_network_game(&mut terminal, &mut client, seat, &username, theme, theme_name)?;
			disable_raw_mode()?;
			execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
			Ok(())
		}
		Ok(MenuResult::StartGame { .. }) => {
			disable_raw_mode()?;
			execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
			Ok(())
		}
		Err(e) => {
			disable_raw_mode()?;
			execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
			Err(e)
		}
	}
}

fn run_network_game(
	terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
	client: &mut GameClient,
	_initial_seat: Seat,
	username: &str,
	theme: Theme,
	theme_name: String,
) -> io::Result<()> {
	let mut game_ui = GameUI::new(None, theme.clone(), theme_name.clone());
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
					return Ok(());
				}

				match game_ui.handle_key(key.code) {
					GameUIAction::Respond(PlayerResponse::Action(action)) => {
						let _ = client.action(action);
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

fn cmd_play_local(player: Option<String>, theme: Option<String>, seed: Option<u64>) -> io::Result<()> {
	let theme_name = theme
		.clone()
		.or_else(|| std::env::var("POKER_THEME").ok())
		.unwrap_or_else(|| "dark".to_string());
	let theme = Theme::load(theme.as_deref());
	let bank = Bank::load().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

	let host_id = resolve_player_id(player.as_deref(), &bank).map_err(|e| {
		io::Error::new(io::ErrorKind::InvalidInput, e)
	})?;

	enable_raw_mode()?;
	let mut stdout = stdout();
	execute!(stdout, EnterAlternateScreen)?;
	let backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(backend)?;

	let result = run_app(&mut terminal, theme, theme_name, host_id, seed);

	disable_raw_mode()?;
	execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

	if let Err(e) = result {
		eprintln!("Error: {}", e);
	}

	Ok(())
}

fn run_app(
	terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
	theme: Theme,
	theme_name: String,
	host_id: String,
	cli_seed: Option<u64>,
) -> io::Result<()> {
	let tables = load_tables().unwrap_or_default();
	let bank = Bank::load().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
	let roster = load_players_auto().unwrap_or_default();

	let backend = LocalBackend::new(tables, roster, bank, host_id.clone());
	let mut menu = Menu::new(backend, host_id.clone(), theme.clone());

	match menu.run(terminal)? {
		MenuResult::Quit => return Ok(()),
		MenuResult::NetworkGameStarted { .. } => return Ok(()),
		MenuResult::StartGame { table, players } => {
			let mut backend = menu.into_backend();

			// Map display names to bank ids
			let name_to_id: std::collections::HashMap<String, String> = players.iter()
				.map(|p| (p.name.clone(), p.id.clone()))
				.collect();

			let standings = run_game(terminal, table.clone(), players, backend.bank_mut(), &host_id, theme, theme_name, cli_seed)?;

			match table.format {
				GameFormat::Cash => {
					for standing in &standings {
						let bank_id = name_to_id.get(&standing.name).unwrap_or(&standing.name);
						backend.bank_mut().cashout(bank_id, standing.final_stack, &table.id);
					}
				}
				GameFormat::SitNGo => {
					let buy_in = table.buy_in.unwrap_or(0.0);
					let num_players = standings.len();
					if let Some(payout_pcts) = &table.payouts {
						let payouts = transparent_poker::table::calculate_payouts(buy_in, num_players, payout_pcts);
						for (i, payout) in payouts.iter().enumerate() {
							if let Some(standing) = standings.iter().find(|s| s.finish_position == (i + 1) as u8) {
								let bank_id = name_to_id.get(&standing.name).unwrap_or(&standing.name);
								backend.bank_mut().award_prize(bank_id, *payout, i + 1);
							}
						}
					}
				}
			}

			backend.bank_mut().save().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
		}
	}

	Ok(())
}

fn run_game(
	terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
	table: TableConfig,
	lobby_players: Vec<LobbyPlayer>,
	bank: &mut Bank,
	_host_id: &str,
	theme: Theme,
	theme_name: String,
	cli_seed: Option<u64>,
) -> io::Result<Vec<Standing>> {
	let (small_blind, big_blind) = table.current_blinds();
	let starting_stack = table.effective_starting_stack();
	let buy_in = table.effective_buy_in();

	for player in &lobby_players {
		bank.buyin(&player.id, buy_in, &table.id)
			.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
	}

	let blind_clock = BlindClock::from_table(&table);

	let betting_structure = match table.betting {
		transparent_poker::table::BettingStructure::NoLimit => BettingStructure::NoLimit,
		transparent_poker::table::BettingStructure::PotLimit => BettingStructure::PotLimit,
		transparent_poker::table::BettingStructure::FixedLimit => BettingStructure::FixedLimit,
	};

	let seed = cli_seed.or(table.seed);
	if let Some(s) = seed {
		logging::log("Engine", "SEED", &format!("{}", s));
	}

	let config = RunnerConfig {
		small_blind,
		big_blind,
		starting_stack,
		betting_structure,
		blind_clock,
		max_raises_per_round: table.max_raises_per_round,
		rake_percent: table.rake_percent,
		rake_cap: table.rake_cap,
		no_flop_no_drop: table.no_flop_no_drop,
		max_hands: None,
		seed,
	};

	let runtime = tokio::runtime::Runtime::new()
		.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
	let runtime_handle = runtime.handle().clone();

	let (mut runner, game_handle) = GameRunner::new(config, runtime_handle);

	let mut shuffled_players = lobby_players.clone();
	if let Some(s) = seed {
		use rand::SeedableRng;
		let mut rng = rand::rngs::StdRng::seed_from_u64(s);
		shuffled_players.shuffle(&mut rng);
	} else {
		shuffled_players.shuffle(&mut rand::rng());
	}

	let seating_log: Vec<String> = shuffled_players
		.iter()
		.enumerate()
		.map(|(i, p)| format!("{} -> Seat {}", p.name, i))
		.collect();
	logging::log("Engine", "SEATING", &seating_log.join(", "));

	let mut human_seat = None;
	let mut human_handle = None;

	let strategies = load_strategies_auto().unwrap_or_default();

	for (seat_idx, lobby_player) in shuffled_players.iter().enumerate() {
		let seat = Seat(seat_idx);

		if lobby_player.is_human {
			let (player, handle) = TerminalPlayer::new(seat, &lobby_player.name);
			runner.add_player(Arc::new(player));
			human_seat = Some(seat);
			human_handle = Some(handle);
		} else {
			let strategy_id = lobby_player.strategy.as_deref().unwrap_or("tag");
			let strategy = strategies.get_or_default(strategy_id);

			let player = Arc::new(RulesPlayer::new(
				seat,
				&lobby_player.name,
				strategy,
				big_blind,
			));
			runner.add_player(player);
		}
	}

	let human_seat = human_seat.expect("No human player found");
	let human_handle = human_handle.expect("No human handle found");

	std::thread::spawn(move || {
		runner.run();
	});

	let mut app = App::new(
		human_seat,
		table.clone(),
		lobby_players.len(),
		seed,
		theme,
		theme_name,
		Arc::clone(&game_handle.quit_signal),
	);
	let delays = DelayConfig::from_table(&table);
	log::event("game started");

	loop {
		app.update_winner_highlights();
		terminal.draw(|f| draw_ui(f, &app))?;

		if app.input_state.is_awaiting_input() {
			if let Event::Key(key) = event::read()? {
				if key.kind == KeyEventKind::Press {
					if app.handle_input(key.code) {
						log::event("user quit from action");
						break;
					}
				}
			}
			continue;
		}

		if app.input_state.is_game_over() {
			if event::poll(Duration::from_millis(100))? {
				if let Event::Key(key) = event::read()? {
					if key.kind == KeyEventKind::Press {
						if app.handle_input(key.code) {
							log::event("user quit from game over");
							break;
						}
					}
				}
			}
			continue;
		}

		if event::poll(Duration::from_millis(0))? {
			if let Event::Key(key) = event::read()? {
				if key.kind == KeyEventKind::Press {
					if app.handle_input(key.code) {
						log::event("user quit while watching");
						break;
					}
				}
			}
		}

		match game_handle.event_rx.recv_timeout(Duration::from_millis(50)) {
			Ok(event) => {
				app.status_message = None;

				let is_human_turn = matches!(
					&event,
					GameEvent::ActionRequest { seat, .. } if seat.0 == app.human_seat.0
				);

				let delay_ms = get_event_delay(&event, app.human_seat, delays);

				app.apply_event(&event);

				if is_human_turn {
					match human_handle.action_rx.recv_timeout(Duration::from_millis(100)) {
						Ok(request) => {
							flush_keyboard_buffer();
							app.enter_action_mode(request);
						}
						Err(_) => {
							log::event("failed to receive action request for human turn");
						}
					}
				} else if delay_ms > 0 {
					if interruptible_sleep(Duration::from_millis(delay_ms))? {
						log::event("user quit during delay");
						app.quit_signal.store(true, Ordering::SeqCst);
						break;
					}
				}
			}
			Err(mpsc::RecvTimeoutError::Timeout) => {}
			Err(mpsc::RecvTimeoutError::Disconnected) => {
				log::event("game engine disconnected");
				break;
			}
		}
	}

	Ok(app.final_standings)
}

fn get_event_delay(event: &GameEvent, human_seat: Seat, delays: DelayConfig) -> u64 {
	match event {
		GameEvent::ActionRequest { seat, .. } => {
			if seat.0 == human_seat.0 { 0 } else { 0 }
		}
		GameEvent::ActionTaken { .. } => delays.action_ms,
		GameEvent::StreetChanged { .. } => delays.street_ms,
		GameEvent::HandEnded { .. } => delays.hand_end_ms,
		_ => 0,
	}
}

fn draw_ui(frame: &mut Frame, app: &App) {
	let area = frame.area();

	let bg = Block::default().style(Style::default().bg(app.theme.background()));
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

	let table_widget = TableWidget::new(&app.table_view, &app.theme)
		.with_info(&app.table_config.name, &app.info_lines);
	frame.render_widget(table_widget, table_area);

	let winner_text = if app.last_winners.is_empty() {
		String::new()
	} else {
		app.last_winners
			.iter()
			.map(|w| {
				let name = app.table_view.players
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

	let has_recent_winner = app.last_winners.iter().any(|w| {
		Instant::now().duration_since(w.awarded_at) < Duration::from_millis(WINNER_HIGHLIGHT_MS)
	});

	let winner_style = if has_recent_winner {
		Style::default().fg(app.theme.winner_border()).add_modifier(Modifier::BOLD)
	} else {
		Style::default().fg(app.theme.status_watching())
	};

	let winner = Paragraph::new(winner_text)
		.style(winner_style)
		.block(Block::default().borders(Borders::ALL).title("Result"));
	frame.render_widget(winner, winner_area);

	let (status_text, status_title, status_style, border_style) = match &app.input_state {
		InputState::AwaitingAction { .. } | InputState::EnteringRaise { .. } => (
			app.status_message.clone().unwrap_or_default(),
			" Your Turn ",
			Style::default().fg(app.theme.status_your_turn()).add_modifier(Modifier::BOLD),
			Style::default().fg(app.theme.status_your_turn_border()),
		),
		InputState::GameOver => (
			app.status_message.clone().unwrap_or_else(|| "Game Over!".to_string()),
			" Game Over ",
			Style::default().fg(app.theme.status_game_over()).add_modifier(Modifier::BOLD),
			Style::default().fg(app.theme.status_game_over_border()),
		),
		InputState::Watching => (
			app.status_message.clone().unwrap_or_else(|| "[t] theme  [q] quit".to_string()),
			" Status ",
			Style::default().fg(app.theme.status_watching()),
			Style::default().fg(app.theme.status_watching_border()),
		),
	};

	let status = Paragraph::new(status_text)
		.style(status_style)
		.block(
			Block::default()
				.borders(Borders::ALL)
				.border_style(border_style)
				.title(status_title)
		);
	frame.render_widget(status, status_area);
}
