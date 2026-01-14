use std::sync::Arc;

use transparent_poker::config::load_strategies_auto;
use transparent_poker::engine::{BettingStructure, GameRunner, RunnerConfig};
use transparent_poker::events::Seat;
use transparent_poker::players::{FoldingPlayer, RulesPlayer};

fn main() {
	let strategies = load_strategies_auto()
		.unwrap_or_default();

	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 500.0,
		betting_structure: BettingStructure::NoLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: Some(50),
		seed: None,
		max_seats: None,
	};

	let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
	let runtime_handle = runtime.handle().clone();

	let (mut runner, game_handle) = GameRunner::new(config, runtime_handle);

	let lisa = Arc::new(RulesPlayer::new(
		Seat(0),
		"Lisa",
		strategies.get_or_default("lag"),
		10.0,
	));

	let lonny = Arc::new(RulesPlayer::new(
		Seat(1),
		"Lonny",
		strategies.get_or_default("rock"),
		10.0,
	));

	let foldy = Arc::new(FoldingPlayer::new(Seat(2), "FoldBot"));

	runner.add_player(lisa);
	runner.add_player(lonny);
	runner.add_player(foldy);

	println!("Starting rules-based AI game");
	println!("See logs/poker-*.log for details\n");

	std::thread::spawn(move || {
		runner.run();
	});

	let mut hand_count = 0;
	loop {
		match game_handle.event_rx.recv() {
			Ok(event) => {
				match &event {
					transparent_poker::events::GameEvent::HandStarted { hand_num, .. } => {
						hand_count = *hand_num;
						println!("\n=== Hand {} ===", hand_num);
					}
					transparent_poker::events::GameEvent::ActionTaken { seat, action, pot_after, .. } => {
						println!("  Seat {}: {} (pot: ${:.0})", seat.0, action.description(), pot_after);
					}
					transparent_poker::events::GameEvent::PotAwarded { seat, amount, hand_description, .. } => {
						println!("  → Seat {} wins ${:.0} {:?}", seat.0, amount, hand_description);
					}
					transparent_poker::events::GameEvent::GameEnded { .. } => {
						println!("\nGame ended after {} hands", hand_count);
						break;
					}
					_ => {}
				}
			}
			Err(_) => break,
		}
	}
}
