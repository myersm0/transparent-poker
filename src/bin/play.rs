use std::io::{self, stdout};
use std::time::Duration;

use clap::{Parser, Subcommand};
use crossterm::{
	execute,
	terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use transparent_poker::bank::Bank;
use transparent_poker::embedded_server::EmbeddedServer;
use transparent_poker::game_loop;
use transparent_poker::lobby::NetworkBackend;
use transparent_poker::menu::{Menu, MenuResult};
use transparent_poker::net::{GameClient, GameServer};
use transparent_poker::theme::Theme;

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
		#[arg(help = "Player name")]
		player: Option<String>,

		#[arg(short, long, env = "POKER_THEME")]
		#[arg(help = "Color theme")]
		theme: Option<String>,

		#[arg(short, long)]
		#[arg(help = "Connect to server (e.g., localhost:9999)")]
		server: Option<String>,
	},

	#[command(about = "Run a standalone poker server")]
	Serve {
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

		#[arg(short, long, default_value = "1000")]
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

fn main() -> io::Result<()> {
	transparent_poker::defaults::ensure_config();
	let cli = Cli::parse();

	match cli.command {
		Commands::Themes => {
			cmd_themes();
			Ok(())
		}
		Commands::Register { name, bankroll } => cmd_register(&name, bankroll),
		Commands::Players => cmd_list_players(),
		Commands::Bankroll { name, action } => cmd_bankroll(&name, action),
		Commands::Serve { bind } => cmd_serve(&bind),
		Commands::Play { player, theme, server } => cmd_play(player, theme, server),
	}
}

fn cmd_themes() {
	let themes = Theme::list_available();
	println!("Available themes:");
	for theme in themes {
		println!("  {}", theme);
	}
	println!("\nUsage: poker play --theme <name>");
	println!("Or set POKER_THEME environment variable");
}

fn cmd_register(name: &str, bankroll: f32) -> io::Result<()> {
	let mut bank = Bank::load().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

	let normalized = name.to_lowercase();
	if bank.profile_exists(&normalized) {
		eprintln!("Player '{}' already exists.", normalized);
		return Ok(());
	}

	bank.register(&normalized, bankroll);
	bank.save().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

	println!("Registered '{}' with bankroll ${:.0}", normalized, bankroll);
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

fn cmd_serve(bind: &str) -> io::Result<()> {
	println!("Starting poker server on {}...", bind);
	let server = GameServer::new();
	server.run(bind)
}

fn cmd_play(player: Option<String>, theme: Option<String>, server: Option<String>) -> io::Result<()> {
	let theme_name = theme
		.clone()
		.or_else(|| std::env::var("POKER_THEME").ok())
		.unwrap_or_else(|| "classic".to_string());
	let theme = Theme::load_named(&theme_name).unwrap_or_default();

	let username = player.unwrap_or_else(|| {
		std::env::var("USER")
			.or_else(|_| std::env::var("USERNAME"))
			.unwrap_or_else(|_| "Player".to_string())
	});

	let (addr, _embedded) = match server {
		Some(addr) => (addr, None),
		None => {
			let embedded = EmbeddedServer::start()?;
			let addr = embedded.addr();
			(addr, Some(embedded))
		}
	};

	std::thread::sleep(Duration::from_millis(100));

	let mut client = GameClient::connect(&addr)?;
	client.login(&username)?;
	std::thread::sleep(Duration::from_millis(100));

	let backend = NetworkBackend::new(client);
	let mut menu = Menu::new(backend, username.clone(), theme.clone());

	enable_raw_mode()?;
	let mut stdout = stdout();
	execute!(stdout, EnterAlternateScreen, SetTitle("transparent-poker"))?;
	let terminal_backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(terminal_backend)?;

	let result = menu.run(&mut terminal);

	match result {
		Ok(MenuResult::Quit) => {
			disable_raw_mode()?;
			execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
			Ok(())
		}
		Ok(MenuResult::NetworkGameStarted { seat: _, table_config, num_players }) => {
			let mut client = menu.into_backend().into_client();
			game_loop::run_game(&mut terminal, &mut client, &username, theme, theme_name, table_config, num_players)?;
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
