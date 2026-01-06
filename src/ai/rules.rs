use crate::strategy::{HandGroup, Position, Strategy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionFacing {
	Unopened,
	Limped,
	SingleRaise,
	ThreeBet,
	FourBetPlus,
}

impl ActionFacing {
	pub fn from_bet_and_blind(current_bet: f32, big_blind: f32) -> Self {
		if current_bet <= big_blind {
			ActionFacing::Unopened
		} else if current_bet <= big_blind * 2.5 {
			ActionFacing::Limped
		} else if current_bet <= big_blind * 8.0 {
			ActionFacing::SingleRaise
		} else if current_bet <= big_blind * 20.0 {
			ActionFacing::ThreeBet
		} else {
			ActionFacing::FourBetPlus
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleDecision {
	Fold,
	Check,
	Call,
	Raise(f32),
}

pub struct Situation {
	pub hand_group: HandGroup,
	pub position: Position,
	pub pot: f32,
	pub to_call: f32,
	pub stack: f32,
	pub big_blind: f32,
	pub current_bet: f32,
	pub is_preflop: bool,
	pub num_raises: u32,
	pub raise_cap: u32,
	pub we_are_preflop_aggressor: bool,
}

impl Situation {
	pub fn action_facing(&self) -> ActionFacing {
		if !self.is_preflop {
			if self.to_call <= 0.0 {
				ActionFacing::Unopened
			} else {
				ActionFacing::SingleRaise
			}
		} else {
			ActionFacing::from_bet_and_blind(self.current_bet, self.big_blind)
		}
	}

	pub fn can_raise(&self) -> bool {
		self.raise_cap == 0 || self.num_raises < self.raise_cap
	}

	pub fn pot_odds(&self) -> f32 {
		if self.to_call <= 0.0 {
			0.0
		} else {
			self.to_call / (self.pot + self.to_call)
		}
	}

	pub fn standard_raise(&self) -> f32 {
		if self.is_preflop {
			(self.current_bet + self.big_blind * 3.0).min(self.stack)
		} else {
			(self.pot * 0.66).min(self.stack)
		}
	}

	pub fn three_bet_size(&self) -> f32 {
		(self.current_bet * 3.0).min(self.stack)
	}
}

pub fn try_preflop_rules(strategy: &Strategy, situation: &Situation) -> Option<RuleDecision> {
	let action = situation.action_facing();

	match action {
		ActionFacing::Unopened => {
			if strategy.should_open(situation.hand_group, situation.position) {
				if situation.can_raise() {
					Some(RuleDecision::Raise(situation.standard_raise()))
				} else {
					Some(RuleDecision::Call)
				}
			} else {
				Some(RuleDecision::Fold)
			}
		}
		ActionFacing::Limped => {
			if strategy.should_open(situation.hand_group, situation.position) && situation.can_raise() {
				Some(RuleDecision::Raise(situation.standard_raise()))
			} else if situation.position == Position::Bb {
				Some(RuleDecision::Check)
			} else if situation.position == Position::Sb && strategy.should_cold_call(situation.hand_group) {
				Some(RuleDecision::Call)
			} else {
				Some(RuleDecision::Fold)
			}
		}
		ActionFacing::SingleRaise => {
			if strategy.should_three_bet(situation.hand_group) && situation.can_raise() {
				Some(RuleDecision::Raise(situation.three_bet_size()))
			} else if strategy.should_cold_call(situation.hand_group) {
				Some(RuleDecision::Call)
			} else if situation.position == Position::Bb && strategy.should_defend_bb(situation.hand_group) {
				Some(RuleDecision::Call)
			} else {
				Some(RuleDecision::Fold)
			}
		}
		ActionFacing::ThreeBet | ActionFacing::FourBetPlus => None,
	}
}

pub fn try_postflop_rules(strategy: &Strategy, situation: &Situation) -> Option<RuleDecision> {
	if situation.to_call <= 0.0 {
		let should_cbet = situation.we_are_preflop_aggressor
			&& rand::random::<f32>() < strategy.continuation_bet;

		if should_cbet && situation.can_raise() {
			let bet_size = (situation.pot * 0.5).max(situation.big_blind).min(situation.stack);
			Some(RuleDecision::Raise(bet_size))
		} else {
			Some(RuleDecision::Check)
		}
	} else {
		let pot_odds = situation.pot_odds();

		if pot_odds < 0.2 {
			Some(RuleDecision::Call)
		} else if pot_odds > 0.4 && rand::random::<f32>() < strategy.fold_to_aggression.fold_frequency() {
			Some(RuleDecision::Fold)
		} else {
			None
		}
	}
}

pub fn try_rules(strategy: &Strategy, situation: &Situation) -> Option<RuleDecision> {
	if situation.is_preflop {
		try_preflop_rules(strategy, situation)
	} else {
		try_postflop_rules(strategy, situation)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::strategy::Strategy;

	fn tag_strategy() -> Strategy {
		Strategy::default()
	}

	fn situation_unopened_btn() -> Situation {
		Situation {
			hand_group: HandGroup::Strong,
			position: Position::Btn,
			pot: 15.0,
			to_call: 0.0,
			stack: 500.0,
			big_blind: 10.0,
			current_bet: 10.0,
			is_preflop: true,
			num_raises: 0,
			raise_cap: 4,
			we_are_preflop_aggressor: false,
		}
	}

	#[test]
	fn test_action_facing_unopened() {
		assert_eq!(ActionFacing::from_bet_and_blind(10.0, 10.0), ActionFacing::Unopened);
	}

	#[test]
	fn test_action_facing_single_raise() {
		assert_eq!(ActionFacing::from_bet_and_blind(30.0, 10.0), ActionFacing::SingleRaise);
	}

	#[test]
	fn test_action_facing_three_bet() {
		assert_eq!(ActionFacing::from_bet_and_blind(90.0, 10.0), ActionFacing::ThreeBet);
	}

	#[test]
	fn test_pot_odds() {
		let mut sit = situation_unopened_btn();
		sit.pot = 100.0;
		sit.to_call = 50.0;
		assert!((sit.pot_odds() - 0.333).abs() < 0.01);
	}

	#[test]
	fn test_can_raise_under_cap() {
		let mut sit = situation_unopened_btn();
		sit.num_raises = 2;
		sit.raise_cap = 4;
		assert!(sit.can_raise());
	}

	#[test]
	fn test_cannot_raise_at_cap() {
		let mut sit = situation_unopened_btn();
		sit.num_raises = 4;
		sit.raise_cap = 4;
		assert!(!sit.can_raise());
	}

	#[test]
	fn test_preflop_open_strong_hand() {
		let strategy = tag_strategy();
		let sit = situation_unopened_btn();
		let decision = try_preflop_rules(&strategy, &sit);
		assert!(matches!(decision, Some(RuleDecision::Raise(_))));
	}

	#[test]
	fn test_preflop_fold_trash() {
		let strategy = tag_strategy();
		let mut sit = situation_unopened_btn();
		sit.hand_group = HandGroup::Trash;
		let decision = try_preflop_rules(&strategy, &sit);
		assert_eq!(decision, Some(RuleDecision::Fold));
	}

	#[test]
	fn test_raise_cap_forces_call() {
		let strategy = tag_strategy();
		let mut sit = situation_unopened_btn();
		sit.num_raises = 4;
		sit.raise_cap = 4;
		let decision = try_preflop_rules(&strategy, &sit);
		assert_eq!(decision, Some(RuleDecision::Call));
	}
}
