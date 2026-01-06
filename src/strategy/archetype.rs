use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use serde::Deserialize;
use super::hand_group::HandGroup;
use super::position::Position;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggression {
	Low,
	Medium,
	High,
	VeryHigh,
	Extreme,
}

impl Aggression {
	pub fn raise_frequency(&self) -> f32 {
		match self {
			Aggression::Low => 0.2,
			Aggression::Medium => 0.4,
			Aggression::High => 0.6,
			Aggression::VeryHigh => 0.75,
			Aggression::Extreme => 0.9,
		}
	}
}

impl Default for Aggression {
	fn default() -> Self {
		Aggression::Medium
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BluffFrequency {
	None,
	Low,
	Medium,
	High,
	VeryHigh,
}

impl BluffFrequency {
	pub fn probability(&self) -> f32 {
		match self {
			BluffFrequency::None => 0.0,
			BluffFrequency::Low => 0.1,
			BluffFrequency::Medium => 0.25,
			BluffFrequency::High => 0.4,
			BluffFrequency::VeryHigh => 0.6,
		}
	}
}

impl Default for BluffFrequency {
	fn default() -> Self {
		BluffFrequency::Low
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoldToAggression {
	VeryLow,
	Low,
	Medium,
	High,
	VeryHigh,
}

impl FoldToAggression {
	pub fn fold_frequency(&self) -> f32 {
		match self {
			FoldToAggression::VeryLow => 0.15,
			FoldToAggression::Low => 0.3,
			FoldToAggression::Medium => 0.5,
			FoldToAggression::High => 0.7,
			FoldToAggression::VeryHigh => 0.85,
		}
	}
}

impl Default for FoldToAggression {
	fn default() -> Self {
		FoldToAggression::Medium
	}
}

#[derive(Debug, Clone, Deserialize)]
struct StrategyConfig {
	name: String,
	description: String,
	opens_utg: Vec<String>,
	opens_mp: Vec<String>,
	opens_co: Vec<String>,
	opens_btn: Vec<String>,
	opens_sb: Vec<String>,
	defends_bb: Vec<String>,
	three_bet: Vec<String>,
	cold_call: Vec<String>,
	#[serde(default)]
	aggression: Aggression,
	#[serde(default)]
	bluff_frequency: BluffFrequency,
	#[serde(default = "default_cbet")]
	continuation_bet: f32,
	#[serde(default)]
	fold_to_aggression: FoldToAggression,
}

fn default_cbet() -> f32 {
	0.65
}

#[derive(Debug, Clone)]
pub struct Strategy {
	pub id: String,
	pub name: String,
	pub description: String,
	opens_utg: HashSet<HandGroup>,
	opens_mp: HashSet<HandGroup>,
	opens_co: HashSet<HandGroup>,
	opens_btn: HashSet<HandGroup>,
	opens_sb: HashSet<HandGroup>,
	defends_bb: HashSet<HandGroup>,
	three_bet: HashSet<HandGroup>,
	cold_call: HashSet<HandGroup>,
	pub aggression: Aggression,
	pub bluff_frequency: BluffFrequency,
	pub continuation_bet: f32,
	pub fold_to_aggression: FoldToAggression,
}

impl Strategy {
	fn parse_hand_groups(names: &[String]) -> HashSet<HandGroup> {
		names.iter()
			.filter_map(|s| HandGroup::from_name(s))
			.collect()
	}

	fn from_config(id: &str, config: StrategyConfig) -> Self {
		Self {
			id: id.to_string(),
			name: config.name,
			description: config.description,
			opens_utg: Self::parse_hand_groups(&config.opens_utg),
			opens_mp: Self::parse_hand_groups(&config.opens_mp),
			opens_co: Self::parse_hand_groups(&config.opens_co),
			opens_btn: Self::parse_hand_groups(&config.opens_btn),
			opens_sb: Self::parse_hand_groups(&config.opens_sb),
			defends_bb: Self::parse_hand_groups(&config.defends_bb),
			three_bet: Self::parse_hand_groups(&config.three_bet),
			cold_call: Self::parse_hand_groups(&config.cold_call),
			aggression: config.aggression,
			bluff_frequency: config.bluff_frequency,
			continuation_bet: config.continuation_bet,
			fold_to_aggression: config.fold_to_aggression,
		}
	}

	pub fn opens_for_position(&self, position: Position) -> &HashSet<HandGroup> {
		match position {
			Position::Utg => &self.opens_utg,
			Position::Mp => &self.opens_mp,
			Position::Co => &self.opens_co,
			Position::Btn => &self.opens_btn,
			Position::Sb => &self.opens_sb,
			Position::Bb => &self.defends_bb,
		}
	}

	pub fn should_open(&self, hand_group: HandGroup, position: Position) -> bool {
		self.opens_for_position(position).contains(&hand_group)
	}

	pub fn should_three_bet(&self, hand_group: HandGroup) -> bool {
		self.three_bet.contains(&hand_group)
	}

	pub fn should_cold_call(&self, hand_group: HandGroup) -> bool {
		self.cold_call.contains(&hand_group)
	}

	pub fn should_defend_bb(&self, hand_group: HandGroup) -> bool {
		self.defends_bb.contains(&hand_group)
	}
}

impl Default for Strategy {
	fn default() -> Self {
		Self {
			id: "default".to_string(),
			name: "Default TAG".to_string(),
			description: "Tight-aggressive baseline".to_string(),
			opens_utg: [HandGroup::Premium, HandGroup::Strong].into_iter().collect(),
			opens_mp: [HandGroup::Premium, HandGroup::Strong, HandGroup::Solid].into_iter().collect(),
			opens_co: [HandGroup::Premium, HandGroup::Strong, HandGroup::Solid, HandGroup::Playable].into_iter().collect(),
			opens_btn: [HandGroup::Premium, HandGroup::Strong, HandGroup::Solid, HandGroup::Playable, HandGroup::Speculative].into_iter().collect(),
			opens_sb: [HandGroup::Premium, HandGroup::Strong, HandGroup::Solid].into_iter().collect(),
			defends_bb: [HandGroup::Premium, HandGroup::Strong, HandGroup::Solid, HandGroup::Playable].into_iter().collect(),
			three_bet: [HandGroup::Premium].into_iter().collect(),
			cold_call: [HandGroup::Strong, HandGroup::Solid].into_iter().collect(),
			aggression: Aggression::Medium,
			bluff_frequency: BluffFrequency::Low,
			continuation_bet: 0.65,
			fold_to_aggression: FoldToAggression::Medium,
		}
	}
}

pub struct StrategyStore {
	strategies: HashMap<String, Strategy>,
}

impl StrategyStore {
	pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, String> {
		let content = fs::read_to_string(&path)
			.map_err(|e| format!("Failed to read {}: {}", path.as_ref().display(), e))?;

		let configs: HashMap<String, StrategyConfig> = toml::from_str(&content)
			.map_err(|e| format!("Failed to parse strategies: {}", e))?;

		let strategies = configs.into_iter()
			.map(|(id, config)| (id.clone(), Strategy::from_config(&id, config)))
			.collect();

		Ok(Self { strategies })
	}

	pub fn get(&self, id: &str) -> Option<&Strategy> {
		self.strategies.get(id)
	}

	pub fn get_or_default(&self, id: &str) -> Strategy {
		self.strategies.get(id).cloned().unwrap_or_default()
	}

	pub fn list(&self) -> Vec<&str> {
		self.strategies.keys().map(|s| s.as_str()).collect()
	}
}

impl Default for StrategyStore {
	fn default() -> Self {
		let mut strategies = HashMap::new();
		strategies.insert("default".to_string(), Strategy::default());
		Self { strategies }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_aggression_raise_frequency() {
		assert_eq!(Aggression::Low.raise_frequency(), 0.2);
		assert_eq!(Aggression::Medium.raise_frequency(), 0.4);
		assert_eq!(Aggression::High.raise_frequency(), 0.6);
		assert_eq!(Aggression::VeryHigh.raise_frequency(), 0.75);
		assert_eq!(Aggression::Extreme.raise_frequency(), 0.9);
	}

	#[test]
	fn test_bluff_frequency_probability() {
		assert_eq!(BluffFrequency::None.probability(), 0.0);
		assert_eq!(BluffFrequency::Low.probability(), 0.1);
		assert_eq!(BluffFrequency::Medium.probability(), 0.25);
		assert_eq!(BluffFrequency::High.probability(), 0.4);
		assert_eq!(BluffFrequency::VeryHigh.probability(), 0.6);
	}

	#[test]
	fn test_fold_to_aggression_frequency() {
		assert_eq!(FoldToAggression::VeryLow.fold_frequency(), 0.15);
		assert_eq!(FoldToAggression::Low.fold_frequency(), 0.3);
		assert_eq!(FoldToAggression::Medium.fold_frequency(), 0.5);
		assert_eq!(FoldToAggression::High.fold_frequency(), 0.7);
		assert_eq!(FoldToAggression::VeryHigh.fold_frequency(), 0.85);
	}

	#[test]
	fn test_strategy_default() {
		let strategy = Strategy::default();
		assert_eq!(strategy.id, "default");
		assert!(strategy.opens_utg.contains(&HandGroup::Premium));
		assert!(!strategy.opens_utg.contains(&HandGroup::Speculative));
		assert!(strategy.opens_btn.contains(&HandGroup::Speculative));
	}

	#[test]
	fn test_strategy_should_open() {
		let strategy = Strategy::default();
		assert!(strategy.should_open(HandGroup::Premium, Position::Utg));
		assert!(!strategy.should_open(HandGroup::Speculative, Position::Utg));
		assert!(strategy.should_open(HandGroup::Speculative, Position::Btn));
	}

	#[test]
	fn test_strategy_should_three_bet() {
		let strategy = Strategy::default();
		assert!(strategy.should_three_bet(HandGroup::Premium));
		assert!(!strategy.should_three_bet(HandGroup::Playable));
	}

	#[test]
	fn test_strategy_store_default() {
		let store = StrategyStore::default();
		assert!(store.get("default").is_some());
		assert!(store.get("nonexistent").is_none());
	}

	#[test]
	fn test_strategy_store_get_or_default() {
		let store = StrategyStore::default();
		let strategy = store.get_or_default("nonexistent");
		assert_eq!(strategy.id, "default");
	}
}
