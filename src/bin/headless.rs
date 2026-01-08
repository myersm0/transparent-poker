use std::sync::Arc;

use transparent_poker::engine::{BettingStructure, GameRunner, RunnerConfig};
use transparent_poker::events::GameEvent;
use transparent_poker::players::{CallingPlayer, FoldingPlayer, TestPlayer};
use transparent_poker::events::{PlayerAction, Seat};

fn main() {
	println!("=== Poker Engine Headless Test ===\n");

	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 100.0,
		betting_structure: BettingStructure::NoLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: None,
		seed: None,
	};

	let (mut runner, handle) = GameRunner::new(config);

	let alice = Arc::new(
		TestPlayer::new(Seat(0), "Alice")
			.with_actions(vec![
				PlayerAction::Raise { amount: 30.0 },
				PlayerAction::Call { amount: 0.0 },
				PlayerAction::Bet { amount: 20.0 },
				PlayerAction::Check,
			])
			.with_default(PlayerAction::Check),
	);

	let bob = Arc::new(CallingPlayer::new(Seat(1), "Bob"));
	let carol = Arc::new(FoldingPlayer::new(Seat(2), "Carol"));

	runner.add_player(alice);
	runner.add_player(bob);
	runner.add_player(carol);

	std::thread::spawn(move || {
		runner.run();
	});

	let mut hand_count = 0;
	let mut event_count = 0;

	while let Ok(event) = handle.event_rx.recv() {
		event_count += 1;

		match &event {
			GameEvent::GameCreated { game_id, .. } => {
				println!("[GAME] Created game {:?}", game_id);
			}
			GameEvent::GameStarted { seats } => {
				println!("[GAME] Started with {} players:", seats.len());
				for s in seats {
					println!("       Seat {}: {} (${:.0})", s.seat.0, s.name, s.stack);
				}
			}
			GameEvent::HandStarted { hand_num, button, blinds, .. } => {
				hand_count += 1;
				println!("\n[HAND #{}] Button: Seat {}, Blinds: ${:.0}/${:.0}",
					hand_num, button.0, blinds.small, blinds.big);
			}
			GameEvent::HoleCardsDealt { seat, cards } => {
				println!("  [DEAL] Seat {} gets {}{} {}{}",
					seat.0,
					cards[0].rank, cards[0].suit,
					cards[1].rank, cards[1].suit);
			}
			GameEvent::BlindPosted { seat, blind_type, amount } => {
				println!("  [BLIND] Seat {} posts {:?} ${:.0}", seat.0, blind_type, amount);
			}
			GameEvent::StreetChanged { street, board } => {
				let board_str: String = board
					.iter()
					.map(|c| format!("{}{}", c.rank, c.suit))
					.collect::<Vec<_>>()
					.join(" ");
				println!("  [STREET] {:?} - Board: {}", street, if board_str.is_empty() { "-".to_string() } else { board_str });
			}
			GameEvent::ActionTaken { seat, action, stack_after, pot_after } => {
				println!("  [ACTION] Seat {}: {} (stack: ${:.0}, pot: ${:.0})",
					seat.0, action.description(), stack_after, pot_after);
			}
			GameEvent::PotAwarded { seat, amount, hand_description, .. } => {
				let desc = hand_description.as_deref().unwrap_or("no showdown");
				println!("  [AWARD] Seat {} wins ${:.0} ({})", seat.0, amount, desc);
			}
			GameEvent::HandEnded { results, .. } => {
				println!("  [HAND END] Results:");
				for r in results {
					let change = if r.stack_change >= 0.0 {
						format!("+${:.0}", r.stack_change)
					} else {
						format!("-${:.0}", -r.stack_change)
					};
					println!("       Seat {}: {} (now ${:.0})", r.seat.0, change, r.final_stack);
				}
			}
			GameEvent::GameEnded { reason, final_standings } => {
				println!("\n[GAME OVER] Reason: {:?}", reason);
				println!("Final standings:");
				for s in final_standings {
					println!("  {}. {} - ${:.0}", s.finish_position, s.name, s.final_stack);
				}
				break;
			}
			GameEvent::ChatMessage { sender, text } => {
				match sender {
					transparent_poker::events::ChatSender::System => println!("  [SYS] {}", text),
					transparent_poker::events::ChatSender::Dealer => println!("  [DEALER] {}", text),
					transparent_poker::events::ChatSender::Player(seat) => println!("  [CHAT] Seat {}: {}", seat.0, text),
					transparent_poker::events::ChatSender::Spectator(name) => println!("  [SPEC] {}: {}", name, text),
				}
			}
			_ => {}
		}
	}

	println!("\n=== Summary ===");
	println!("Hands played: {}", hand_count);
	println!("Events emitted: {}", event_count);
}
