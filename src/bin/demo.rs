use std::io;
use crossterm::{
	event::{self, Event, KeyCode, KeyEventKind},
	terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
	ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};

use poker_tui::view::{ActionPrompt, Card, ChatMessage, PlayerStatus, PlayerView, Position, Street, TableView};
use poker_tui::theme::Theme;
use poker_tui::tui::TableWidget;

fn main() -> io::Result<()> {
	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;

	let backend = CrosstermBackend::new(io::stdout());
	let mut terminal = Terminal::new(backend)?;

	let scenarios = build_scenarios();
	let theme = Theme::load(None);
	let mut current = 0;

	loop {
		terminal.draw(|frame| {
			let view = &scenarios[current];
			let widget = TableWidget::new(view, &theme);
			frame.render_widget(widget, frame.area());
		})?;

		if event::poll(std::time::Duration::from_millis(100))? {
			if let Event::Key(key) = event::read()? {
				if key.kind != KeyEventKind::Press {
					continue;
				}
				match key.code {
					KeyCode::Char('q') | KeyCode::Esc => break,
					KeyCode::Right | KeyCode::Char('n') => {
						current = (current + 1) % scenarios.len();
					}
					KeyCode::Left | KeyCode::Char('p') => {
						current = (current + scenarios.len() - 1) % scenarios.len();
					}
					KeyCode::Char('1'..='9') => {
						let idx = key.code.to_string().parse::<usize>().unwrap_or(1) - 1;
						if idx < scenarios.len() {
							current = idx;
						}
					}
					_ => {}
				}
			}
		}
	}

	disable_raw_mode()?;
	io::stdout().execute(LeaveAlternateScreen)?;
	Ok(())
}

fn build_scenarios() -> Vec<TableView> {
	vec![
		scenario_preflop_6_players(),
		scenario_flop_action(),
		scenario_heads_up(),
		scenario_showdown(),
		scenario_full_table_10(),
		scenario_short_handed(),
	]
}

fn scenario_preflop_6_players() -> TableView {
	TableView {
		game_id: Some("demo-001".to_string()),
		hand_num: 42,
		street: Street::Preflop,
		board: vec![],
		pot: 75.0,
		blinds: (5.0, 10.0),
		action_prompt: Some(ActionPrompt {
			to_call: 25.0,
			min_raise: 50.0,
			max_bet: 500.0,
			can_raise: true,
			message: Some("Lisa raised to $30, Wildcard re-raised to $50".to_string()),
		}),
		chat_messages: vec![
			ChatMessage { sender: "Dealer".to_string(), text: "Hand #42 starting".to_string(), is_system: true },
			ChatMessage { sender: "Chen".to_string(), text: "folds".to_string(), is_system: false },
			ChatMessage { sender: "Lisa".to_string(), text: "raises to $30".to_string(), is_system: false },
			ChatMessage { sender: "Murphy".to_string(), text: "folds".to_string(), is_system: false },
			ChatMessage { sender: "Wildcard".to_string(), text: "re-raises to $50".to_string(), is_system: false },
		],
		table_name: Some("Demo Table".to_string()),
		table_info: Some("No-Limit Cash".to_string()),
		winner_seats: vec![],
		players: vec![
			PlayerView {
				seat: 0,
				name: "Chen".to_string(),
				stack: 485.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("fold".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 1,
				name: "Lisa".to_string(),
				stack: 520.0,
				current_bet: 30.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("raise".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 2,
				name: "Murphy".to_string(),
				stack: 450.0,
				current_bet: 0.0,
				status: PlayerStatus::Folded,
				position: Position::Button,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("fold".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 3,
				name: "Hero".to_string(),
				stack: 500.0,
				current_bet: 5.0,
				status: PlayerStatus::Active,
				position: Position::SmallBlind,
				hole_cards: Some([
					Card::new('A', 's'),
					Card::new('K', 's'),
				]),
				is_hero: true,
				is_actor: true,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 4,
				name: "Ivan".to_string(),
				stack: 490.0,
				current_bet: 10.0,
				status: PlayerStatus::Active,
				position: Position::BigBlind,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 5,
				name: "Wildcard".to_string(),
				stack: 380.0,
				current_bet: 30.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("raise".to_string()),
				action_fresh: false,
			},
		],
	}
}

fn scenario_flop_action() -> TableView {
	TableView {
		game_id: Some("demo-001".to_string()),
		hand_num: 42,
		street: Street::Flop,
		board: vec![
			Card::new('A', 'h'),
			Card::new('7', 's'),
			Card::new('2', 'd'),
		],
		pot: 120.0,
		blinds: (5.0, 10.0),
		action_prompt: None,
		chat_messages: vec![
			ChatMessage { sender: "Dealer".to_string(), text: "Flop: A♥ 7♠ 2♦".to_string(), is_system: true },
			ChatMessage { sender: "Hero".to_string(), text: "checks".to_string(), is_system: false },
			ChatMessage { sender: "Ivan".to_string(), text: "checks".to_string(), is_system: false },
			ChatMessage { sender: "Wildcard".to_string(), text: "checks".to_string(), is_system: false },
		],
		table_name: Some("Demo Table".to_string()),
		table_info: Some("No-Limit Cash".to_string()),
		winner_seats: vec![],
		players: vec![
			PlayerView {
				seat: 0,
				name: "Lisa".to_string(),
				stack: 490.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: true,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 1,
				name: "Hero".to_string(),
				stack: 470.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::SmallBlind,
				hole_cards: Some([
					Card::new('A', 's'),
					Card::new('K', 's'),
				]),
				is_hero: true,
				is_actor: false,
				last_action: Some("check".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 2,
				name: "Ivan".to_string(),
				stack: 460.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::BigBlind,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("check".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 3,
				name: "Wildcard".to_string(),
				stack: 350.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("check".to_string()),
				action_fresh: false,
			},
		],
	}
}

fn scenario_heads_up() -> TableView {
	TableView {
		game_id: Some("demo-001".to_string()),
		hand_num: 87,
		street: Street::River,
		board: vec![
			Card::new('K', 'h'),
			Card::new('Q', 's'),
			Card::new('7', 'd'),
			Card::new('2', 'c'),
			Card::new('9', 'h'),
		],
		pot: 800.0,
		blinds: (25.0, 50.0),
		action_prompt: Some(ActionPrompt {
			to_call: 200.0,
			min_raise: 400.0,
			max_bet: 600.0,
			can_raise: true,
			message: Some("Lisa bets $200 into $600".to_string()),
		}),
		chat_messages: vec![
			ChatMessage { sender: "".to_string(), text: "Heads up for the championship!".to_string(), is_system: true },
			ChatMessage { sender: "Dealer".to_string(), text: "River: 9♥".to_string(), is_system: true },
			ChatMessage { sender: "Lisa".to_string(), text: "bets $200".to_string(), is_system: false },
		],
		table_name: Some("Heads-Up Championship".to_string()),
		table_info: Some("No-Limit SnG".to_string()),
		winner_seats: vec![],
		players: vec![
			PlayerView {
				seat: 0,
				name: "Lisa".to_string(),
				stack: 1200.0,
				current_bet: 200.0,
				status: PlayerStatus::Active,
				position: Position::Button,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("bet 200".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 1,
				name: "Hero".to_string(),
				stack: 600.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::BigBlind,
				hole_cards: Some([
					Card::new('K', 's'),
					Card::new('J', 'h'),
				]),
				is_hero: true,
				is_actor: true,
				last_action: None,
				action_fresh: false,
			},
		],
	}
}

fn scenario_showdown() -> TableView {
	TableView {
		game_id: Some("demo-001".to_string()),
		hand_num: 55,
		street: Street::Showdown,
		board: vec![
			Card::new('A', 'c'),
			Card::new('A', 'd'),
			Card::new('8', 's'),
			Card::new('3', 'h'),
			Card::new('K', 's'),
		],
		pot: 500.0,
		blinds: (10.0, 20.0),
		action_prompt: None,
		chat_messages: vec![
			ChatMessage { sender: "Dealer".to_string(), text: "Showdown".to_string(), is_system: true },
			ChatMessage { sender: "Chen".to_string(), text: "shows K♥ Q♥ - Two Pair".to_string(), is_system: false },
			ChatMessage { sender: "Hero".to_string(), text: "shows A♥ J♠ - Three of a Kind".to_string(), is_system: false },
			ChatMessage { sender: "".to_string(), text: "Hero wins $500 with Trip Aces!".to_string(), is_system: true },
		],
		table_name: Some("Demo Table".to_string()),
		table_info: Some("No-Limit Cash".to_string()),
		winner_seats: vec![1],
		players: vec![
			PlayerView {
				seat: 0,
				name: "Chen".to_string(),
				stack: 300.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::Button,
				hole_cards: Some([
					Card::new('K', 'h'),
					Card::new('Q', 'h'),
				]),
				is_hero: false,
				is_actor: false,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 1,
				name: "Hero".to_string(),
				stack: 750.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::SmallBlind,
				hole_cards: Some([
					Card::new('A', 'h'),
					Card::new('J', 's'),
				]),
				is_hero: true,
				is_actor: false,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 2,
				name: "Lisa".to_string(),
				stack: 200.0,
				current_bet: 0.0,
				status: PlayerStatus::Folded,
				position: Position::BigBlind,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: None,
				action_fresh: false,
			},
		],
	}
}

fn scenario_full_table_10() -> TableView {
	TableView {
		game_id: Some("demo-002".to_string()),
		hand_num: 1,
		street: Street::Preflop,
		board: vec![],
		pot: 75.0,
		blinds: (5.0, 10.0),
		action_prompt: Some(ActionPrompt {
			to_call: 20.0,
			min_raise: 40.0,
			max_bet: 495.0,
			can_raise: true,
			message: None,
		}),
		chat_messages: vec![
			ChatMessage { sender: "".to_string(), text: "10-max table (← → to cycle)".to_string(), is_system: true },
			ChatMessage { sender: "Alice".to_string(), text: "folds".to_string(), is_system: false },
			ChatMessage { sender: "Bob".to_string(), text: "raises to $30".to_string(), is_system: false },
			ChatMessage { sender: "Carol".to_string(), text: "folds".to_string(), is_system: false },
		],
		table_name: Some("Full Ring".to_string()),
		table_info: Some("No-Limit Cash 10-max".to_string()),
		winner_seats: vec![],
		players: vec![
			PlayerView {
				seat: 0,
				name: "Alice".to_string(),
				stack: 500.0,
				current_bet: 0.0,
				status: PlayerStatus::Folded,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("fold".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 1,
				name: "Bob".to_string(),
				stack: 470.0,
				current_bet: 30.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("raise".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 2,
				name: "Carol".to_string(),
				stack: 500.0,
				current_bet: 0.0,
				status: PlayerStatus::Folded,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("fold".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 3,
				name: "Dan".to_string(),
				stack: 470.0,
				current_bet: 30.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("call".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 4,
				name: "Eve".to_string(),
				stack: 500.0,
				current_bet: 0.0,
				status: PlayerStatus::Folded,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("fold".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 5,
				name: "Frank".to_string(),
				stack: 500.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: true,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 6,
				name: "Grace".to_string(),
				stack: 500.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::None,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 7,
				name: "Hank".to_string(),
				stack: 500.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::Button,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 8,
				name: "Hero".to_string(),
				stack: 495.0,
				current_bet: 5.0,
				status: PlayerStatus::Active,
				position: Position::SmallBlind,
				hole_cards: Some([
					Card::new('Q', 'h'),
					Card::new('Q', 'd'),
				]),
				is_hero: true,
				is_actor: false,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 9,
				name: "Ivy".to_string(),
				stack: 490.0,
				current_bet: 10.0,
				status: PlayerStatus::Active,
				position: Position::BigBlind,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: None,
				action_fresh: false,
			},
		],
	}
}

fn scenario_short_handed() -> TableView {
	TableView {
		game_id: Some("demo-003".to_string()),
		hand_num: 23,
		street: Street::Turn,
		board: vec![
			Card::new('J', 'c'),
			Card::new('T', 'h'),
			Card::new('4', 's'),
			Card::new('2', 'd'),
		],
		pot: 240.0,
		blinds: (10.0, 20.0),
		action_prompt: Some(ActionPrompt {
			to_call: 80.0,
			min_raise: 160.0,
			max_bet: 420.0,
			can_raise: true,
			message: Some("Andy bets $80".to_string()),
		}),
		chat_messages: vec![
			ChatMessage { sender: "Dealer".to_string(), text: "Turn: 2♦".to_string(), is_system: true },
			ChatMessage { sender: "Andy".to_string(), text: "bets $80".to_string(), is_system: false },
			ChatMessage { sender: "".to_string(), text: "q to quit, ← → to cycle".to_string(), is_system: true },
		],
		table_name: Some("Short Handed".to_string()),
		table_info: Some("No-Limit Cash 3-max".to_string()),
		winner_seats: vec![],
		players: vec![
			PlayerView {
				seat: 0,
				name: "Andy".to_string(),
				stack: 680.0,
				current_bet: 80.0,
				status: PlayerStatus::Active,
				position: Position::Button,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("bet 80".to_string()),
				action_fresh: false,
			},
			PlayerView {
				seat: 1,
				name: "Hero".to_string(),
				stack: 420.0,
				current_bet: 0.0,
				status: PlayerStatus::Active,
				position: Position::SmallBlind,
				hole_cards: Some([
					Card::new('Q', 's'),
					Card::new('J', 's'),
				]),
				is_hero: true,
				is_actor: true,
				last_action: None,
				action_fresh: false,
			},
			PlayerView {
				seat: 2,
				name: "Tom".to_string(),
				stack: 550.0,
				current_bet: 0.0,
				status: PlayerStatus::Folded,
				position: Position::BigBlind,
				hole_cards: None,
				is_hero: false,
				is_actor: false,
				last_action: Some("fold".to_string()),
				action_fresh: false,
			},
		],
	}
}
