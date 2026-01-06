use std::sync::Arc;

use poker_tui::engine::{BettingStructure, GameRunner, RunnerConfig};
use poker_tui::events::{GameEvent, PlayerAction, Seat};
use poker_tui::players::TestPlayer;

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

	let (mut runner, handle) = GameRunner::new(config);

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

	let (mut runner, handle) = GameRunner::new(config);

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

	let (mut runner, handle) = GameRunner::new(config);

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

	let (mut runner, handle) = GameRunner::new(config);

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

	let (mut runner, handle) = GameRunner::new(config);

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
				poker_tui::events::BlindType::Small => {
					assert!((amount - 5.0).abs() < 0.01, "Small blind should be 5.0");
					small_blind_posted = true;
				}
				poker_tui::events::BlindType::Big => {
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

	let (mut runner, handle) = GameRunner::new(config);

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

	let (mut runner, handle) = GameRunner::new(config);

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

	let (mut runner, handle) = GameRunner::new(config);

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
