use std::sync::Arc;

use tokio::runtime::Runtime;
use transparent_poker::engine::{BettingStructure, GameRunner, RunnerConfig};
use transparent_poker::events::{GameEvent, PlayerAction, Seat};
use transparent_poker::players::TestPlayer;

fn create_runner(config: RunnerConfig) -> (GameRunner, transparent_poker::engine::GameHandle, Runtime) {
	let runtime = Runtime::new().expect("Failed to create tokio runtime");
	let handle = runtime.handle().clone();
	let (runner, game_handle) = GameRunner::new(config, handle);
	(runner, game_handle, runtime)
}

#[test]
fn test_heads_up_game_completes() {
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
		max_hands: Some(50),
		seed: Some(12345),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let alice = Arc::new(
		TestPlayer::new(Seat(0), "Alice")
			.with_default(PlayerAction::Check)
	);
	let bob = Arc::new(
		TestPlayer::new(Seat(1), "Bob")
			.with_default(PlayerAction::Fold)
	);

	runner.add_player(alice);
	runner.add_player(bob);
	runner.run();

	let mut saw_game_ended = false;
	while let Ok(event) = handle.event_rx.try_recv() {
		if matches!(event, GameEvent::GameEnded { .. }) {
			saw_game_ended = true;
		}
	}

	assert!(saw_game_ended, "Game should complete with GameEnded event");
}

#[test]
fn test_raise_cap_enforced() {
	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 1000.0,
		betting_structure: BettingStructure::NoLimit,
		blind_clock: None,
		max_raises_per_round: 2,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: Some(5),
		seed: Some(99999),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let raiser = Arc::new(
		TestPlayer::new(Seat(0), "Raiser")
			.with_actions(vec![
				PlayerAction::Raise { amount: 30.0 },
				PlayerAction::Raise { amount: 90.0 },
				PlayerAction::Raise { amount: 270.0 },
				PlayerAction::Raise { amount: 810.0 },
			])
			.with_default(PlayerAction::Call { amount: 0.0 })
	);
	let caller = Arc::new(
		TestPlayer::new(Seat(1), "Caller")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);

	runner.add_player(raiser);
	runner.add_player(caller);
	runner.run();

	let mut max_raises_in_hand = 0;
	let mut current_raises = 0;
	let mut last_street = None;

	while let Ok(event) = handle.event_rx.try_recv() {
		match event {
			GameEvent::ActionTaken { action, .. } => {
				if matches!(action, PlayerAction::Raise { .. } | PlayerAction::Bet { .. }) {
					current_raises += 1;
				}
			}
			GameEvent::StreetChanged { street, .. } => {
				if last_street != Some(street) {
					max_raises_in_hand = max_raises_in_hand.max(current_raises);
					current_raises = 0;
					last_street = Some(street);
				}
			}
			GameEvent::HandEnded { .. } => {
				max_raises_in_hand = max_raises_in_hand.max(current_raises);
				current_raises = 0;
			}
			_ => {}
		}
	}

	assert!(max_raises_in_hand <= 2, "Raise cap of 2 should be enforced, saw {}", max_raises_in_hand);
}

#[test]
fn test_three_player_game_completes() {
	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 30.0,
		betting_structure: BettingStructure::NoLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: Some(20),
		seed: Some(42),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let alice = Arc::new(
		TestPlayer::new(Seat(0), "Alice")
			.with_default(PlayerAction::AllIn { amount: 30.0 })
	);
	let bob = Arc::new(
		TestPlayer::new(Seat(1), "Bob")
			.with_default(PlayerAction::Fold)
	);
	let carol = Arc::new(
		TestPlayer::new(Seat(2), "Carol")
			.with_default(PlayerAction::Fold)
	);

	runner.add_player(alice);
	runner.add_player(bob);
	runner.add_player(carol);
	runner.run();

	let mut saw_game_ended = false;
	let mut player_count = 0;

	while let Ok(event) = handle.event_rx.try_recv() {
		match event {
			GameEvent::PlayerJoined { .. } => player_count += 1,
			GameEvent::GameEnded { .. } => saw_game_ended = true,
			_ => {}
		}
	}

	assert_eq!(player_count, 3, "Should have 3 players");
	assert!(saw_game_ended, "Game should complete");
}

#[test]
fn test_all_in_scenario() {
	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 50.0,
		betting_structure: BettingStructure::NoLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: Some(5),
		seed: Some(77777),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let aggressive = Arc::new(
		TestPlayer::new(Seat(0), "Aggressive")
			.with_actions(vec![
				PlayerAction::AllIn { amount: 50.0 },
			])
			.with_default(PlayerAction::Check)
	);
	let caller = Arc::new(
		TestPlayer::new(Seat(1), "Caller")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);

	runner.add_player(aggressive);
	runner.add_player(caller);
	runner.run();

	let mut saw_all_in = false;
	let mut saw_pot_awarded = false;

	while let Ok(event) = handle.event_rx.try_recv() {
		match event {
			GameEvent::ActionTaken { action: PlayerAction::AllIn { .. }, .. } => {
				saw_all_in = true;
			}
			GameEvent::PotAwarded { .. } => {
				saw_pot_awarded = true;
			}
			_ => {}
		}
	}

	assert!(saw_all_in, "Should see all-in action");
	assert!(saw_pot_awarded, "Should see pot awarded");
}

#[test]
fn test_blind_posting() {
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
		max_hands: Some(3),
		seed: Some(11111),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let alice = Arc::new(
		TestPlayer::new(Seat(0), "Alice")
			.with_default(PlayerAction::Fold)
	);
	let bob = Arc::new(
		TestPlayer::new(Seat(1), "Bob")
			.with_default(PlayerAction::Fold)
	);

	runner.add_player(alice);
	runner.add_player(bob);
	runner.run();

	let mut small_blind_posted = false;
	let mut big_blind_posted = false;

	while let Ok(event) = handle.event_rx.try_recv() {
		if let GameEvent::BlindPosted { blind_type, amount, .. } = event {
			match blind_type {
				transparent_poker::events::BlindType::Small => {
					assert!((amount - 5.0).abs() < 0.01, "Small blind should be 5.0");
					small_blind_posted = true;
				}
				transparent_poker::events::BlindType::Big => {
					assert!((amount - 10.0).abs() < 0.01, "Big blind should be 10.0");
					big_blind_posted = true;
				}
				_ => {}
			}
		}
	}

	assert!(small_blind_posted, "Small blind should be posted");
	assert!(big_blind_posted, "Big blind should be posted");
}

#[test]
fn test_hand_started_event() {
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
		max_hands: Some(3),
		seed: Some(22222),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let alice = Arc::new(
		TestPlayer::new(Seat(0), "Alice")
			.with_default(PlayerAction::Fold)
	);
	let bob = Arc::new(
		TestPlayer::new(Seat(1), "Bob")
			.with_default(PlayerAction::Fold)
	);

	runner.add_player(alice);
	runner.add_player(bob);
	runner.run();

	let mut hand_started_count = 0;

	while let Ok(event) = handle.event_rx.try_recv() {
		if matches!(event, GameEvent::HandStarted { .. }) {
			hand_started_count += 1;
		}
	}

	assert!(hand_started_count >= 1, "At least one hand should start");
}

#[test]
fn test_folding_player_loses_blinds() {
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
		max_hands: Some(3),
		seed: Some(33333),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let folder = Arc::new(
		TestPlayer::new(Seat(0), "Folder")
			.with_default(PlayerAction::Fold)
	);
	let winner = Arc::new(
		TestPlayer::new(Seat(1), "Winner")
			.with_default(PlayerAction::Check)
	);

	runner.add_player(folder);
	runner.add_player(winner);
	runner.run();

	let mut pot_awarded_amount = 0.0;

	while let Ok(event) = handle.event_rx.try_recv() {
		if let GameEvent::PotAwarded { amount, .. } = event {
			pot_awarded_amount = amount;
		}
	}

	assert!(pot_awarded_amount > 0.0, "Winner should receive pot");
}

#[test]
fn test_stack_conservation() {
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
		max_hands: Some(10),
		seed: Some(44444),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let alice = Arc::new(
		TestPlayer::new(Seat(0), "Alice")
			.with_default(PlayerAction::Check)
	);
	let bob = Arc::new(
		TestPlayer::new(Seat(1), "Bob")
			.with_default(PlayerAction::Check)
	);

	runner.add_player(alice);
	runner.add_player(bob);
	runner.run();

	let mut final_stacks = Vec::new();

	while let Ok(event) = handle.event_rx.try_recv() {
		if let GameEvent::GameEnded { final_standings, .. } = event {
			final_stacks = final_standings.iter().map(|s| s.final_stack).collect();
		}
	}

	let total: f32 = final_stacks.iter().sum();
	assert!((total - 200.0).abs() < 0.01, "Total stacks should be conserved (200), got {}", total);
}

#[test]
fn test_fixed_limit_betting() {
	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 200.0,
		betting_structure: BettingStructure::FixedLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: Some(5),
		seed: Some(55555),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let bettor = Arc::new(
		TestPlayer::new(Seat(0), "Bettor")
			.with_actions(vec![
				PlayerAction::Raise { amount: 20.0 }, // Should be capped to fixed amount
			])
			.with_default(PlayerAction::Check)
	);
	let caller = Arc::new(
		TestPlayer::new(Seat(1), "Caller")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);

	runner.add_player(bettor);
	runner.add_player(caller);
	runner.run();

	// Game should complete without errors
	let mut saw_game_ended = false;
	while let Ok(event) = handle.event_rx.try_recv() {
		if matches!(event, GameEvent::GameEnded { .. }) {
			saw_game_ended = true;
		}
	}
	assert!(saw_game_ended, "Fixed-limit game should complete");
}

#[test]
fn test_pot_limit_betting() {
	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 200.0,
		betting_structure: BettingStructure::PotLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: Some(5),
		seed: Some(66666),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let bettor = Arc::new(
		TestPlayer::new(Seat(0), "Bettor")
			.with_actions(vec![
				PlayerAction::Raise { amount: 35.0 }, // Pot-sized raise
			])
			.with_default(PlayerAction::Check)
	);
	let caller = Arc::new(
		TestPlayer::new(Seat(1), "Caller")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);

	runner.add_player(bettor);
	runner.add_player(caller);
	runner.run();

	let mut saw_game_ended = false;
	while let Ok(event) = handle.event_rx.try_recv() {
		if matches!(event, GameEvent::GameEnded { .. }) {
			saw_game_ended = true;
		}
	}
	assert!(saw_game_ended, "Pot-limit game should complete");
}

#[test]
fn test_multiway_pot() {
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
		max_hands: Some(3),
		seed: Some(88888),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	// Everyone calls, creating a multiway pot
	let p1 = Arc::new(
		TestPlayer::new(Seat(0), "P1")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);
	let p2 = Arc::new(
		TestPlayer::new(Seat(1), "P2")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);
	let p3 = Arc::new(
		TestPlayer::new(Seat(2), "P3")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);
	let p4 = Arc::new(
		TestPlayer::new(Seat(3), "P4")
			.with_default(PlayerAction::Fold)
	);

	runner.add_player(p1);
	runner.add_player(p2);
	runner.add_player(p3);
	runner.add_player(p4);
	runner.run();

	let mut player_count = 0;
	let mut saw_pot_awarded = false;

	while let Ok(event) = handle.event_rx.try_recv() {
		match event {
			GameEvent::PlayerJoined { .. } => player_count += 1,
			GameEvent::PotAwarded { .. } => saw_pot_awarded = true,
			_ => {}
		}
	}

	assert_eq!(player_count, 4, "Should have 4 players");
	assert!(saw_pot_awarded, "Should award pot");
}

#[test]
fn test_side_pot_creation() {
	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 50.0,
		betting_structure: BettingStructure::NoLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: Some(1),
		seed: Some(99991),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	// Player with less chips goes all-in, others call
	let short_stack = Arc::new(
		TestPlayer::new(Seat(0), "ShortStack")
			.with_actions(vec![PlayerAction::AllIn { amount: 50.0 }])
			.with_default(PlayerAction::Check)
	);
	let caller1 = Arc::new(
		TestPlayer::new(Seat(1), "Caller1")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);
	let caller2 = Arc::new(
		TestPlayer::new(Seat(2), "Caller2")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);

	runner.add_player(short_stack);
	runner.add_player(caller1);
	runner.add_player(caller2);
	runner.run();

	let mut pot_awards = 0;
	while let Ok(event) = handle.event_rx.try_recv() {
		if matches!(event, GameEvent::PotAwarded { .. }) {
			pot_awards += 1;
		}
	}

	// Should complete without crashing (side pot logic)
	assert!(pot_awards >= 1, "Should award at least one pot");
}

#[test]
fn test_game_with_rake() {
	let config = RunnerConfig {
		small_blind: 5.0,
		big_blind: 10.0,
		starting_stack: 100.0,
		betting_structure: BettingStructure::NoLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.05, // 5% rake
		rake_cap: Some(5.0), // $5 cap
		no_flop_no_drop: true,
		max_hands: Some(5),
		seed: Some(11112),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let p1 = Arc::new(
		TestPlayer::new(Seat(0), "P1")
			.with_default(PlayerAction::Call { amount: 0.0 })
	);
	let p2 = Arc::new(
		TestPlayer::new(Seat(1), "P2")
			.with_default(PlayerAction::Fold)
	);

	runner.add_player(p1);
	runner.add_player(p2);
	runner.run();

	let mut final_stacks = Vec::new();
	while let Ok(event) = handle.event_rx.try_recv() {
		if let GameEvent::GameEnded { final_standings, .. } = event {
			final_stacks = final_standings.iter().map(|s| s.final_stack).collect();
		}
	}

	let total: f32 = final_stacks.iter().sum();
	// With rake, total should be less than starting (200)
	// But with no_flop_no_drop and folding preflop, no rake is taken
	assert!(total <= 200.0, "Total with rake should be <= starting");
}

#[test]
fn test_elimination_order() {
	let config = RunnerConfig {
		small_blind: 25.0,
		big_blind: 50.0,
		starting_stack: 100.0,
		betting_structure: BettingStructure::NoLimit,
		blind_clock: None,
		max_raises_per_round: 4,
		rake_percent: 0.0,
		rake_cap: None,
		no_flop_no_drop: false,
		max_hands: Some(20),
		seed: Some(22223),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	// High blinds relative to stack = quick eliminations
	let p1 = Arc::new(
		TestPlayer::new(Seat(0), "P1")
			.with_default(PlayerAction::AllIn { amount: 100.0 })
	);
	let p2 = Arc::new(
		TestPlayer::new(Seat(1), "P2")
			.with_default(PlayerAction::Fold)
	);
	let p3 = Arc::new(
		TestPlayer::new(Seat(2), "P3")
			.with_default(PlayerAction::Fold)
	);

	runner.add_player(p1);
	runner.add_player(p2);
	runner.add_player(p3);
	runner.run();

	let mut standings = Vec::new();
	while let Ok(event) = handle.event_rx.try_recv() {
		if let GameEvent::GameEnded { final_standings, .. } = event {
			standings = final_standings;
		}
	}

	// Check finish positions are assigned
	assert!(!standings.is_empty(), "Should have standings");
	let positions: Vec<u8> = standings.iter().map(|s| s.finish_position).collect();
	assert!(positions.contains(&1), "Should have 1st place");
}

#[test]
fn test_button_rotation() {
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
		max_hands: Some(4),
		seed: Some(33334),
	};

	let (mut runner, handle, _runtime) = create_runner(config);

	let p1 = Arc::new(TestPlayer::new(Seat(0), "P1").with_default(PlayerAction::Fold));
	let p2 = Arc::new(TestPlayer::new(Seat(1), "P2").with_default(PlayerAction::Fold));
	let p3 = Arc::new(TestPlayer::new(Seat(2), "P3").with_default(PlayerAction::Fold));

	runner.add_player(p1);
	runner.add_player(p2);
	runner.add_player(p3);
	runner.run();

	let mut buttons = Vec::new();
	while let Ok(event) = handle.event_rx.try_recv() {
		if let GameEvent::HandStarted { button, .. } = event {
			buttons.push(button);
		}
	}

	// Button should rotate (not all same seat)
	assert!(buttons.len() >= 2, "Should have multiple hands");
	if buttons.len() >= 3 {
		// With 3+ hands and 3 players, button should have moved
		let unique: std::collections::HashSet<_> = buttons.iter().collect();
		assert!(unique.len() > 1, "Button should rotate between players");
	}
}
