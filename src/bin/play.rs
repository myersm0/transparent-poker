use std::env;
use std::io::{self, stdout};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crossterm::{
	event::{self, Event, KeyCode, KeyEventKind},
	execute,
	terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use rand::seq::SliceRandom;
use ratatui::{
	backend::CrosstermBackend,
	layout::{Constraint, Direction, Layout},
	style::{Color, Modifier, Style},
	widgets::{Block, Borders, Paragraph},
	Frame, Terminal,
};

use poker_tui::bank::Bank;
use poker_tui::config::{load_players_auto, load_strategies_auto};
use poker_tui::engine::{BettingStructure, GameRunner, RunnerConfig};
use poker_tui::events::{GameEvent, PlayerAction, RaiseOptions, Seat, Standing, ValidActions, ViewUpdater};
use poker_tui::logging::{self, tui as log};
use poker_tui::menu::{LobbyPlayer, Menu, MenuResult};
use poker_tui::players::{ActionRequest, PlayerResponse, RulesPlayer, TerminalPlayer};
use poker_tui::table::{load_tables, BlindClock, GameFormat, TableConfig};
use poker_tui::tui::TableWidget;
use poker_tui::view::TableView;

fn parse_player_arg() -> Option<String> {
	let args: Vec<String> = env::args().collect();
	let mut i = 1;
	while i < args.len() {
		if args[i] == "--player" || args[i] == "-p" {
			if i + 1 < args.len() {
				return Some(args[i + 1].clone());
			}
		} else if args[i].starts_with("--player=") {
			return Some(args[i].trim_start_matches("--player=").to_string());
		}
		i += 1;
	}
	None
}

fn resolve_player_id(bank: &mut Bank) -> Result<String, String> {
	if let Some(name) = parse_player_arg() {
		if bank.profile_exists(&name) {
			return Ok(name);
		} else {
			return Err(format!(
				"Player '{}' not found in profiles.toml. Use 'poker register {}' to create.",
				name, name
			));
		}
	}

	if let Ok(name) = env::var("POKER_USER") {
		if bank.profile_exists(&name) {
			return Ok(name);
		} else {
			return Err(format!(
				"POKER_USER='{}' not found in profiles.toml. Use 'poker register {}' to create.",
				name, name
			));
		}
	}

	Err("No player specified. Use --player <name> or set POKER_USER environment variable.".to_string())
}

const SPEED_LEVELS: &[u64] = &[0, 100, 250, 500, 1000, 2000];
const HAND_END_MULTIPLIER: u64 = 4;
const MIN_ACTION_DELAY_MS: u64 = 500;

enum InputMode {
	Watching,
	SelectingAction { valid: ValidActions, response_tx: mpsc::Sender<PlayerResponse> },
	EnteringRaise { valid: ValidActions, response_tx: mpsc::Sender<PlayerResponse>, amount: f32, min: f32, max: f32 },
	GameOver,
}

struct WinnerInfo {
	seat: Seat,
	amount: f32,
	description: Option<String>,
	highlight_until: Option<Instant>,
}

const WINNER_HIGHLIGHT_DURATION_MS: u64 = 2000;

struct App {
	table_view: TableView,
	view_updater: ViewUpdater,
	input_mode: InputMode,
	status_message: Option<String>,
	speed_level: usize,
	delay_until: Option<Instant>,
	last_winners: Vec<WinnerInfo>,
	hand_just_ended: bool,
	human_seat: Seat,
	final_standings: Vec<Standing>,
	winner_seats: Vec<usize>,
}

impl App {
	fn new(human_seat: Seat, table_name: String, table_info: String) -> Self {
		let table_view = TableView::new().with_table_info(table_name, table_info);
		Self {
			table_view,
			view_updater: ViewUpdater::new(Some(human_seat)),
			input_mode: InputMode::Watching,
			status_message: None,
			speed_level: 3,
			delay_until: None,
			last_winners: Vec::new(),
			hand_just_ended: false,
			human_seat,
			final_standings: Vec::new(),
			winner_seats: Vec::new(),
		}
	}

	fn action_delay(&self) -> Duration {
		let base = SPEED_LEVELS[self.speed_level];
		Duration::from_millis(base.max(MIN_ACTION_DELAY_MS))
	}

	fn hand_end_delay(&self) -> Duration {
		let base = SPEED_LEVELS[self.speed_level];
		Duration::from_millis(base.max(MIN_ACTION_DELAY_MS) * HAND_END_MULTIPLIER)
	}

	fn set_delay(&mut self, duration: Duration) {
		if duration.as_millis() > 0 {
			self.delay_until = Some(Instant::now() + duration);
		}
	}

	fn is_delayed(&self) -> bool {
		self.delay_until.map(|t| Instant::now() < t).unwrap_or(false)
	}

	fn update_winner_highlights(&mut self) {
		let now = Instant::now();
		self.winner_seats.retain(|seat| {
			self.last_winners.iter().any(|w| {
				w.seat.0 == *seat && w.highlight_until.map(|t| now < t).unwrap_or(false)
			})
		});
		self.table_view.winner_seats = self.winner_seats.clone();
	}

	fn apply_event(&mut self, event: &GameEvent) {
		log::event(&format!("{:?}", event));
		self.view_updater.apply(&mut self.table_view, event);

		match event {
			GameEvent::HandStarted { .. } => {
				self.last_winners.clear();
				self.winner_seats.clear();
				self.hand_just_ended = false;
			}
			GameEvent::StreetChanged { .. } => {
				self.set_delay(self.action_delay());
			}
			GameEvent::ActionRequest { seat, .. } => {
				if seat.0 != self.human_seat.0 {
					self.set_delay(self.action_delay());
				}
			}
			GameEvent::ActionTaken { .. } => {
				self.set_delay(self.action_delay());
			}
			GameEvent::PotAwarded { seat, amount, hand_description, .. } => {
				let highlight_until = Some(Instant::now() + Duration::from_millis(WINNER_HIGHLIGHT_DURATION_MS));
				self.last_winners.push(WinnerInfo {
					seat: *seat,
					amount: *amount,
					description: hand_description.clone(),
					highlight_until,
				});
				if !self.winner_seats.contains(&seat.0) {
					self.winner_seats.push(seat.0);
				}
			}
			GameEvent::HandEnded { .. } => {
				self.hand_just_ended = true;
				self.set_delay(self.hand_end_delay());
			}
			GameEvent::GameEnded { final_standings, .. } => {
				self.final_standings = final_standings.clone();
				self.input_mode = InputMode::GameOver;
				self.status_message = Some("Game Over! Press 'q' to quit.".to_string());
			}
			_ => {}
		}
	}

	fn start_action_input(&mut self, request: ActionRequest) {
		log::event(&format!(
			"action_request seat={} can_fold={} can_check={} call={:?}",
			request.seat.0,
			request.valid_actions.can_fold,
			request.valid_actions.can_check,
			request.valid_actions.call_amount
		));

		let prompt = self.build_action_prompt(&request.valid_actions);
		self.status_message = Some(prompt);

		self.input_mode = InputMode::SelectingAction {
			valid: request.valid_actions,
			response_tx: request.response_tx,
		};
	}

	fn build_action_prompt(&self, valid: &ValidActions) -> String {
		let mut parts = Vec::new();

		if valid.can_check {
			parts.push("[Enter] check".to_string());
			if valid.raise_options.is_some() {
				parts.push("[b]et".to_string());
			}
		} else if let Some(amt) = valid.call_amount {
			parts.push(format!("[c]all ${:.0}", amt));
			if valid.can_fold {
				parts.push("[f]old".to_string());
			}
		}

		if valid.raise_options.is_some() && !valid.can_check {
			parts.push("[r]aise".to_string());
		}

		if valid.can_all_in {
			parts.push("[a]ll-in".to_string());
		}

		parts.join("  ")
	}

	fn speed_status(&self) -> String {
		let ms = SPEED_LEVELS[self.speed_level].max(MIN_ACTION_DELAY_MS);
		format!("Speed: {}ms [↑/↓]", ms)
	}

	fn handle_key(&mut self, key: KeyCode) -> bool {
		if key == KeyCode::Up && self.speed_level > 0 {
			self.speed_level -= 1;
			return false;
		}
		if key == KeyCode::Down && self.speed_level < SPEED_LEVELS.len() - 1 {
			self.speed_level += 1;
			return false;
		}

		let mode = std::mem::replace(&mut self.input_mode, InputMode::Watching);

		match mode {
			InputMode::Watching => {
				self.input_mode = InputMode::Watching;
				if key == KeyCode::Char('q') || key == KeyCode::Esc {
					return true;
				}
			}
			InputMode::SelectingAction { valid, response_tx } => {
				log::input(&format!("{:?}", key));
				match key {
					KeyCode::Char('f') => {
						if valid.can_fold {
							log::action("FOLD");
							let _ = response_tx.send(PlayerResponse::Action(PlayerAction::Fold));
							self.input_mode = InputMode::Watching;
							self.status_message = None;
						} else {
							let prompt = self.build_action_prompt(&valid);
							self.status_message = Some(format!("Can't fold (no bet). {}", prompt));
							self.input_mode = InputMode::SelectingAction { valid, response_tx };
						}
					}
					KeyCode::Enter => {
						if valid.can_check {
							log::action("CHECK");
							let _ = response_tx.send(PlayerResponse::Action(PlayerAction::Check));
							self.input_mode = InputMode::Watching;
							self.status_message = None;
						} else {
							self.input_mode = InputMode::SelectingAction { valid, response_tx };
						}
					}
					KeyCode::Char('c') => {
						if let Some(amount) = valid.call_amount {
							log::action(&format!("CALL ${:.0}", amount));
							let _ = response_tx.send(PlayerResponse::Action(PlayerAction::Call { amount }));
							self.input_mode = InputMode::Watching;
							self.status_message = None;
						} else if valid.can_check {
							log::action("CHECK");
							let _ = response_tx.send(PlayerResponse::Action(PlayerAction::Check));
							self.input_mode = InputMode::Watching;
							self.status_message = None;
						} else {
							self.input_mode = InputMode::SelectingAction { valid, response_tx };
						}
					}
					KeyCode::Char('b') => {
						if valid.can_check {
							if let Some(ref raise_opts) = valid.raise_options {
								let bet_amount = match raise_opts {
									RaiseOptions::Fixed { amount } => *amount,
									RaiseOptions::Variable { min_raise, max_raise } => {
										(min_raise * 1.5).min(*max_raise)
									}
								};
								log::action(&format!("BET ${:.0}", bet_amount));
								let _ = response_tx.send(PlayerResponse::Action(PlayerAction::Bet { amount: bet_amount }));
								self.input_mode = InputMode::Watching;
								self.status_message = None;
							} else {
								self.input_mode = InputMode::SelectingAction { valid, response_tx };
							}
						} else {
							self.input_mode = InputMode::SelectingAction { valid, response_tx };
						}
					}
					KeyCode::Char('r') => {
						if let Some(ref raise_opts) = valid.raise_options {
							let (min, max) = match raise_opts {
								RaiseOptions::Fixed { amount } => (*amount, *amount),
								RaiseOptions::Variable { min_raise, max_raise } => (*min_raise, *max_raise),
							};
							self.status_message = Some(format!(
								"Raise: ${:.0} (min ${:.0}, max ${:.0}) [←/→] adjust [Enter] confirm [Esc] cancel",
								min, min, max
							));
							self.input_mode = InputMode::EnteringRaise {
								valid,
								response_tx,
								amount: min,
								min,
								max,
							};
						} else {
							self.input_mode = InputMode::SelectingAction { valid, response_tx };
						}
					}
					KeyCode::Char('a') if valid.can_all_in => {
						log::action(&format!("ALL-IN ${:.0}", valid.all_in_amount));
						let _ = response_tx.send(PlayerResponse::Action(PlayerAction::AllIn { amount: valid.all_in_amount }));
						self.input_mode = InputMode::Watching;
						self.status_message = None;
					}
					KeyCode::Char('q') | KeyCode::Esc => {
						return true;
					}
					_ => {
						self.input_mode = InputMode::SelectingAction { valid, response_tx };
					}
				}
			}
			InputMode::EnteringRaise { valid, response_tx, amount, min, max } => {
				match key {
					KeyCode::Left => {
						let step = (max - min) / 10.0;
						let new_amount = (amount - step.max(1.0)).max(min);
						self.status_message = Some(format!(
							"Raise: ${:.0} (min ${:.0}, max ${:.0}) [←/→] adjust [Enter] confirm [Esc] cancel",
							new_amount, min, max
						));
						self.input_mode = InputMode::EnteringRaise {
							valid,
							response_tx,
							amount: new_amount,
							min,
							max,
						};
					}
					KeyCode::Right => {
						let step = (max - min) / 10.0;
						let new_amount = (amount + step.max(1.0)).min(max);
						self.status_message = Some(format!(
							"Raise: ${:.0} (min ${:.0}, max ${:.0}) [←/→] adjust [Enter] confirm [Esc] cancel",
							new_amount, min, max
						));
						self.input_mode = InputMode::EnteringRaise {
							valid,
							response_tx,
							amount: new_amount,
							min,
							max,
						};
					}
					KeyCode::Enter => {
						log::action(&format!("RAISE ${:.0}", amount));
						let _ = response_tx.send(PlayerResponse::Action(PlayerAction::Raise { amount }));
						self.input_mode = InputMode::Watching;
						self.status_message = None;
					}
					KeyCode::Esc => {
						let prompt = self.build_action_prompt(&valid);
						self.status_message = Some(prompt);
						self.input_mode = InputMode::SelectingAction { valid, response_tx };
					}
					_ => {
						self.input_mode = InputMode::EnteringRaise { valid, response_tx, amount, min, max };
					}
				}
			}
			InputMode::GameOver => {
				self.input_mode = InputMode::GameOver;
				if key == KeyCode::Char('q') || key == KeyCode::Esc || key == KeyCode::Enter {
					return true;
				}
			}
		}
		false
	}
}

fn flush_keyboard_buffer() {
	while event::poll(Duration::from_millis(0)).unwrap_or(false) {
		let _ = event::read();
	}
}

fn main() -> io::Result<()> {
	enable_raw_mode()?;
	let mut stdout = stdout();
	execute!(stdout, EnterAlternateScreen)?;
	let backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(backend)?;

	let result = run_app(&mut terminal);

	disable_raw_mode()?;
	execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

	if let Err(e) = result {
		eprintln!("Error: {}", e);
	}

	Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
	let tables = load_tables().unwrap_or_default();
	let mut bank = Bank::load().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
	let roster = load_players_auto().unwrap_or_default();

	let host_id = resolve_player_id(&mut bank).map_err(|e| {
		io::Error::new(io::ErrorKind::InvalidInput, e)
	})?;
	bank.set_host(&host_id, true);

	let mut menu = Menu::new(tables, bank, host_id.clone(), roster);

	match menu.run(terminal)? {
		MenuResult::Quit => return Ok(()),
		MenuResult::StartGame { table, players } => {
			let mut bank = Bank::load().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
			let standings = run_game(terminal, table.clone(), players, &mut bank, &host_id)?;

			match table.format {
				GameFormat::Cash => {
					for standing in &standings {
						bank.cashout(&standing.name, standing.final_stack, &table.id);
					}
				}
				GameFormat::SitNGo => {
					let buy_in = table.buy_in.unwrap_or(0.0);
					let num_players = standings.len();
					if let Some(payout_pcts) = &table.payouts {
						let payouts = poker_tui::table::calculate_payouts(buy_in, num_players, payout_pcts);
						for (i, payout) in payouts.iter().enumerate() {
							if let Some(standing) = standings.iter().find(|s| s.finish_position == (i + 1) as u8) {
								bank.award_prize(&standing.name, *payout, i + 1);
							}
						}
					}
				}
			}

			bank.save().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
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
		poker_tui::table::BettingStructure::NoLimit => BettingStructure::NoLimit,
		poker_tui::table::BettingStructure::PotLimit => BettingStructure::PotLimit,
		poker_tui::table::BettingStructure::FixedLimit => BettingStructure::FixedLimit,
	};

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
		seed: None,
	};

	let (mut runner, game_handle) = GameRunner::new(config);

	let mut shuffled_players = lobby_players.clone();
	shuffled_players.shuffle(&mut rand::rng());

	let seating_log: Vec<String> = shuffled_players
		.iter()
		.enumerate()
		.map(|(i, p)| format!("{} -> Seat {}", p.id, i))
		.collect();
	logging::log("Engine", "SEATING", &seating_log.join(", "));

	let mut human_seat = None;
	let mut human_handle = None;

	let strategies = load_strategies_auto().unwrap_or_default();

	for (seat_idx, lobby_player) in shuffled_players.iter().enumerate() {
		let seat = Seat(seat_idx);

		if lobby_player.is_human {
			let (player, handle) = TerminalPlayer::new(seat, &lobby_player.id);
			runner.add_player(Arc::new(player));
			human_seat = Some(seat);
			human_handle = Some(handle);
		} else {
			let strategy_id = lobby_player.strategy.as_deref().unwrap_or("tag");
			let strategy = strategies.get_or_default(strategy_id);

			let player = Arc::new(RulesPlayer::new(
				seat,
				&lobby_player.id,
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

	let table_info = format!("{} {}", table.betting, table.format);
	let mut app = App::new(human_seat, table.name.clone(), table_info);
	log::event("game started");

	let mut pending_events: Vec<GameEvent> = Vec::new();

	loop {
		app.update_winner_highlights();
		terminal.draw(|f| draw_ui(f, &app))?;

		if event::poll(Duration::from_millis(50))? {
			if let Event::Key(key) = event::read()? {
				if key.kind == KeyEventKind::Press {
					match &app.input_mode {
						InputMode::Watching | InputMode::GameOver => {
							if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
								log::event("user quit");
								break;
							}
							if key.code == KeyCode::Up || key.code == KeyCode::Down {
								app.handle_key(key.code);
							}
						}
						_ => {
							if app.handle_key(key.code) {
								log::event("user quit from action");
								break;
							}
						}
					}
				}
			}
		}

		while let Ok(event) = game_handle.event_rx.try_recv() {
			if matches!(event, GameEvent::ActionRequest { .. }) {
				app.apply_event(&event);
			} else {
				pending_events.push(event);
			}
		}

		if !app.is_delayed() && !pending_events.is_empty() {
			let event = pending_events.remove(0);
			app.apply_event(&event);
		}

		if !app.is_delayed() {
			if let Ok(request) = human_handle.action_rx.try_recv() {
				flush_keyboard_buffer();
				std::thread::sleep(Duration::from_millis(10));
				flush_keyboard_buffer();
				app.start_action_input(request);
			}
		}
	}

	Ok(app.final_standings)
}

fn draw_ui(frame: &mut Frame, app: &App) {
	let area = frame.area();

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

	let table_widget = TableWidget::new(&app.table_view);
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

	let winner_style = if app.hand_just_ended {
		Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
	} else {
		Style::default().fg(Color::DarkGray)
	};

	let winner = Paragraph::new(winner_text)
		.style(winner_style)
		.block(Block::default().borders(Borders::ALL).title("Result"));
	frame.render_widget(winner, winner_area);

	let (status_text, status_title, status_style, border_style) = match &app.input_mode {
		InputMode::SelectingAction { .. } | InputMode::EnteringRaise { .. } => (
			app.status_message.clone().unwrap_or_default(),
			" Your Turn ",
			Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
			Style::default().fg(Color::Yellow),
		),
		InputMode::GameOver => (
			app.status_message.clone().unwrap_or_else(|| "Game Over!".to_string()),
			" Game Over ",
			Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
			Style::default().fg(Color::Green),
		),
		InputMode::Watching => (
			app.speed_status(),
			" Status ",
			Style::default().fg(Color::DarkGray),
			Style::default().fg(Color::DarkGray),
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
