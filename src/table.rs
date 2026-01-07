use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::logging;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameFormat {
	Cash,
	SitNGo,
}

impl std::fmt::Display for GameFormat {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			GameFormat::Cash => write!(f, "Cash"),
			GameFormat::SitNGo => write!(f, "Sit & Go"),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BettingStructure {
	NoLimit,
	PotLimit,
	FixedLimit,
}

impl std::fmt::Display for BettingStructure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			BettingStructure::NoLimit => write!(f, "No-Limit"),
			BettingStructure::PotLimit => write!(f, "Pot-Limit"),
			BettingStructure::FixedLimit => write!(f, "Fixed-Limit"),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindLevel {
	pub small: f32,
	pub big: f32,
	pub hands: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableConfig {
	pub id: String,
	pub name: String,
	pub format: GameFormat,
	pub betting: BettingStructure,

	#[serde(default)]
	pub small_blind: Option<f32>,
	#[serde(default)]
	pub big_blind: Option<f32>,
	#[serde(default)]
	pub min_buy_in: Option<f32>,
	#[serde(default)]
	pub max_buy_in: Option<f32>,

	#[serde(default)]
	pub buy_in: Option<f32>,
	#[serde(default)]
	pub starting_stack: Option<f32>,
	#[serde(default)]
	pub blind_levels: Option<Vec<BlindLevel>>,
	#[serde(default)]
	pub payouts: Option<Vec<f32>>,

	#[serde(default = "default_min_players")]
	pub min_players: usize,
	#[serde(default = "default_max_players")]
	pub max_players: usize,

	#[serde(default = "default_max_raises")]
	pub max_raises_per_round: u32,

	#[serde(default)]
	pub rake_percent: f32,
	#[serde(default)]
	pub rake_cap: Option<f32>,
	#[serde(default)]
	pub no_flop_no_drop: bool,

	#[serde(default)]
	pub action_timeout_seconds: Option<u32>,
	#[serde(default)]
	pub max_consecutive_timeouts: Option<u32>,

	#[serde(default = "default_action_delay")]
	pub action_delay_ms: u64,
	#[serde(default = "default_street_delay")]
	pub street_delay_ms: u64,
	#[serde(default = "default_hand_end_delay")]
	pub hand_end_delay_ms: u64,
}

fn default_min_players() -> usize {
	2
}

fn default_max_players() -> usize {
	6
}

fn default_max_raises() -> u32 {
	4
}

fn default_action_delay() -> u64 {
	500
}

fn default_street_delay() -> u64 {
	700
}

fn default_hand_end_delay() -> u64 {
	2000
}

impl TableConfig {
	pub fn current_blinds(&self) -> (f32, f32) {
		match self.format {
			GameFormat::Cash => {
				let small = self.small_blind.unwrap_or(1.0);
				let big = self.big_blind.unwrap_or(2.0);
				(small, big)
			}
			GameFormat::SitNGo => {
				if let Some(levels) = &self.blind_levels {
					if let Some(first) = levels.first() {
						return (first.small, first.big);
					}
				}
				(10.0, 20.0)
			}
		}
	}

	pub fn effective_buy_in(&self) -> f32 {
		match self.format {
			GameFormat::Cash => self.min_buy_in.unwrap_or(40.0),
			GameFormat::SitNGo => self.buy_in.unwrap_or(50.0),
		}
	}

	pub fn effective_starting_stack(&self) -> f32 {
		match self.format {
			GameFormat::Cash => self.min_buy_in.unwrap_or(100.0),
			GameFormat::SitNGo => self.starting_stack.unwrap_or(1500.0),
		}
	}

	pub fn summary(&self) -> String {
		match self.format {
			GameFormat::Cash => {
				let (small, big) = self.current_blinds();
				format!("{} ${:.0}/${:.0}", self.betting, small, big)
			}
			GameFormat::SitNGo => {
				format!("{} ${:.0} BI", self.betting, self.effective_buy_in())
			}
		}
	}

	pub fn player_range(&self) -> String {
		if self.min_players == self.max_players {
			format!("{} players", self.min_players)
		} else {
			format!("{}-{} players", self.min_players, self.max_players)
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TablesFile {
	#[serde(default)]
	tables: Vec<TableConfig>,
}

pub fn load_tables() -> Result<Vec<TableConfig>, String> {
	let path = config_path()?;

	if !path.exists() {
		return Ok(default_tables());
	}

	let content = fs::read_to_string(&path)
		.map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

	let file: TablesFile = toml::from_str(&content)
		.map_err(|e| format!("Failed to parse tables config: {}", e))?;

	Ok(file.tables)
}

fn config_path() -> Result<PathBuf, String> {
	if let Some(config_dir) = dirs::config_dir() {
		let user_path = config_dir.join("transparent-poker").join("tables.toml");
		if user_path.exists() {
			return Ok(user_path);
		}
	}
	Ok(PathBuf::from("config/tables.toml"))
}

fn default_tables() -> Vec<TableConfig> {
	vec![
		TableConfig {
			id: "micro-cash".to_string(),
			name: "Micro Stakes Cash".to_string(),
			format: GameFormat::Cash,
			betting: BettingStructure::NoLimit,
			small_blind: Some(1.0),
			big_blind: Some(2.0),
			min_buy_in: Some(40.0),
			max_buy_in: Some(200.0),
			buy_in: None,
			starting_stack: None,
			blind_levels: None,
			payouts: None,
			min_players: 2,
			max_players: 6,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			action_delay_ms: default_action_delay(),
			street_delay_ms: default_street_delay(),
			hand_end_delay_ms: default_hand_end_delay(),
		},
		TableConfig {
			id: "home-sng".to_string(),
			name: "Home Game SnG".to_string(),
			format: GameFormat::SitNGo,
			betting: BettingStructure::NoLimit,
			small_blind: None,
			big_blind: None,
			min_buy_in: None,
			max_buy_in: None,
			buy_in: Some(50.0),
			starting_stack: Some(1500.0),
			blind_levels: Some(vec![
				BlindLevel { small: 10.0, big: 20.0, hands: 10 },
				BlindLevel { small: 15.0, big: 30.0, hands: 10 },
				BlindLevel { small: 25.0, big: 50.0, hands: 10 },
				BlindLevel { small: 50.0, big: 100.0, hands: 10 },
				BlindLevel { small: 100.0, big: 200.0, hands: 10 },
			]),
			payouts: Some(vec![0.65, 0.35]),
			min_players: 3,
			max_players: 6,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: Some(30),
			max_consecutive_timeouts: Some(3),
			action_delay_ms: default_action_delay(),
			street_delay_ms: default_street_delay(),
			hand_end_delay_ms: default_hand_end_delay(),
		},
	]
}

#[derive(Debug, Clone)]
pub struct BlindClock {
	levels: Vec<BlindLevel>,
	current_level: usize,
	hands_at_level: u32,
}

impl BlindClock {
	pub fn new(levels: Vec<BlindLevel>) -> Self {
		Self {
			levels,
			current_level: 0,
			hands_at_level: 0,
		}
	}

	pub fn from_table(config: &TableConfig) -> Option<Self> {
		config.blind_levels.as_ref().map(|levels| Self::new(levels.clone()))
	}

	pub fn current(&self) -> (f32, f32) {
		self.levels
			.get(self.current_level)
			.map(|l| (l.small, l.big))
			.unwrap_or_else(|| {
				self.levels.last().map(|l| (l.small, l.big)).unwrap_or((10.0, 20.0))
			})
	}

	pub fn current_level_num(&self) -> usize {
		self.current_level + 1
	}

	pub fn hands_remaining(&self) -> Option<u32> {
		self.levels.get(self.current_level).map(|l| l.hands.saturating_sub(self.hands_at_level))
	}

	pub fn advance_hand(&mut self) -> bool {
		self.hands_at_level += 1;

		if let Some(level) = self.levels.get(self.current_level) {
			if self.hands_at_level >= level.hands {
				if self.current_level + 1 < self.levels.len() {
					self.current_level += 1;
					self.hands_at_level = 0;

					let (small, big) = self.current();
					logging::log(
						"Engine",
						"BLINDS",
						&format!("level {}: ${:.0}/${:.0}", self.current_level + 1, small, big),
					);
					return true;
				}
			}
		}
		false
	}

	pub fn is_final_level(&self) -> bool {
		self.current_level + 1 >= self.levels.len()
	}
}

pub fn calculate_payouts(buy_in: f32, num_players: usize, payout_percentages: &[f32]) -> Vec<f32> {
	let prize_pool = buy_in * num_players as f32;
	payout_percentages.iter().map(|p| prize_pool * p).collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_blind_clock() {
		let levels = vec![
			BlindLevel { small: 10.0, big: 20.0, hands: 3 },
			BlindLevel { small: 25.0, big: 50.0, hands: 3 },
		];

		let mut clock = BlindClock::new(levels);
		assert_eq!(clock.current(), (10.0, 20.0));
		assert_eq!(clock.current_level_num(), 1);

		assert!(!clock.advance_hand());
		assert!(!clock.advance_hand());
		assert!(clock.advance_hand());

		assert_eq!(clock.current(), (25.0, 50.0));
		assert_eq!(clock.current_level_num(), 2);
	}

	#[test]
	fn test_payouts() {
		let payouts = calculate_payouts(50.0, 6, &[0.65, 0.35]);
		assert_eq!(payouts.len(), 2);
		assert!((payouts[0] - 195.0).abs() < 0.01);
		assert!((payouts[1] - 105.0).abs() < 0.01);
	}

	#[test]
	fn test_blind_clock_hands_remaining() {
		let levels = vec![
			BlindLevel { small: 10.0, big: 20.0, hands: 5 },
		];
		let mut clock = BlindClock::new(levels);
		assert_eq!(clock.hands_remaining(), Some(5));
		clock.advance_hand();
		assert_eq!(clock.hands_remaining(), Some(4));
		clock.advance_hand();
		clock.advance_hand();
		assert_eq!(clock.hands_remaining(), Some(2));
	}

	#[test]
	fn test_blind_clock_is_final_level() {
		let levels = vec![
			BlindLevel { small: 10.0, big: 20.0, hands: 2 },
			BlindLevel { small: 25.0, big: 50.0, hands: 2 },
		];
		let mut clock = BlindClock::new(levels);
		assert!(!clock.is_final_level());
		clock.advance_hand();
		clock.advance_hand();
		assert!(clock.is_final_level());
	}

	#[test]
	fn test_blind_clock_stays_at_final_level() {
		let levels = vec![
			BlindLevel { small: 10.0, big: 20.0, hands: 1 },
			BlindLevel { small: 25.0, big: 50.0, hands: 1 },
		];
		let mut clock = BlindClock::new(levels);
		clock.advance_hand();
		clock.advance_hand();
		clock.advance_hand();
		clock.advance_hand();
		assert_eq!(clock.current(), (25.0, 50.0));
	}

	#[test]
	fn test_game_format_display() {
		assert_eq!(format!("{}", GameFormat::Cash), "Cash");
		assert_eq!(format!("{}", GameFormat::SitNGo), "Sit & Go");
	}

	#[test]
	fn test_betting_structure_display() {
		assert_eq!(format!("{}", BettingStructure::NoLimit), "No-Limit");
		assert_eq!(format!("{}", BettingStructure::PotLimit), "Pot-Limit");
		assert_eq!(format!("{}", BettingStructure::FixedLimit), "Fixed-Limit");
	}

	#[test]
	fn test_table_config_current_blinds_cash() {
		let config = TableConfig {
			id: "test".to_string(),
			name: "Test".to_string(),
			format: GameFormat::Cash,
			betting: BettingStructure::NoLimit,
			small_blind: Some(5.0),
			big_blind: Some(10.0),
			min_buy_in: None,
			max_buy_in: None,
			buy_in: None,
			starting_stack: None,
			blind_levels: None,
			payouts: None,
			min_players: 2,
			max_players: 6,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			action_delay_ms: 500,
			street_delay_ms: 700,
			hand_end_delay_ms: 2000,
		};
		assert_eq!(config.current_blinds(), (5.0, 10.0));
	}

	#[test]
	fn test_table_config_current_blinds_sng() {
		let config = TableConfig {
			id: "test".to_string(),
			name: "Test".to_string(),
			format: GameFormat::SitNGo,
			betting: BettingStructure::NoLimit,
			small_blind: None,
			big_blind: None,
			min_buy_in: None,
			max_buy_in: None,
			buy_in: Some(100.0),
			starting_stack: Some(1500.0),
			blind_levels: Some(vec![
				BlindLevel { small: 15.0, big: 30.0, hands: 10 },
			]),
			payouts: None,
			min_players: 2,
			max_players: 6,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			action_delay_ms: 500,
			street_delay_ms: 700,
			hand_end_delay_ms: 2000,
		};
		assert_eq!(config.current_blinds(), (15.0, 30.0));
	}

	#[test]
	fn test_table_config_effective_buy_in() {
		let cash = TableConfig {
			id: "test".to_string(),
			name: "Test".to_string(),
			format: GameFormat::Cash,
			betting: BettingStructure::NoLimit,
			small_blind: Some(1.0),
			big_blind: Some(2.0),
			min_buy_in: Some(80.0),
			max_buy_in: Some(200.0),
			buy_in: None,
			starting_stack: None,
			blind_levels: None,
			payouts: None,
			min_players: 2,
			max_players: 6,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			action_delay_ms: 500,
			street_delay_ms: 700,
			hand_end_delay_ms: 2000,
		};
		assert_eq!(cash.effective_buy_in(), 80.0);

		let sng = TableConfig {
			id: "test".to_string(),
			name: "Test".to_string(),
			format: GameFormat::SitNGo,
			betting: BettingStructure::NoLimit,
			small_blind: None,
			big_blind: None,
			min_buy_in: None,
			max_buy_in: None,
			buy_in: Some(100.0),
			starting_stack: Some(1500.0),
			blind_levels: None,
			payouts: None,
			min_players: 2,
			max_players: 6,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			action_delay_ms: 500,
			street_delay_ms: 700,
			hand_end_delay_ms: 2000,
		};
		assert_eq!(sng.effective_buy_in(), 100.0);
	}

	#[test]
	fn test_table_config_player_range() {
		let config = TableConfig {
			id: "test".to_string(),
			name: "Test".to_string(),
			format: GameFormat::Cash,
			betting: BettingStructure::NoLimit,
			small_blind: Some(1.0),
			big_blind: Some(2.0),
			min_buy_in: None,
			max_buy_in: None,
			buy_in: None,
			starting_stack: None,
			blind_levels: None,
			payouts: None,
			min_players: 2,
			max_players: 6,
			max_raises_per_round: 4,
			rake_percent: 0.0,
			rake_cap: None,
			no_flop_no_drop: false,
			action_timeout_seconds: None,
			max_consecutive_timeouts: None,
			action_delay_ms: 500,
			street_delay_ms: 700,
			hand_end_delay_ms: 2000,
		};
		assert_eq!(config.player_range(), "2-6 players");

		let heads_up = TableConfig {
			min_players: 2,
			max_players: 2,
			..config
		};
		assert_eq!(heads_up.player_range(), "2 players");
	}

	#[test]
	fn test_payouts_single_winner() {
		let payouts = calculate_payouts(100.0, 5, &[1.0]);
		assert_eq!(payouts.len(), 1);
		assert!((payouts[0] - 500.0).abs() < 0.01);
	}

	#[test]
	fn test_payouts_three_places() {
		let payouts = calculate_payouts(50.0, 9, &[0.50, 0.30, 0.20]);
		assert_eq!(payouts.len(), 3);
		assert!((payouts[0] - 225.0).abs() < 0.01);
		assert!((payouts[1] - 135.0).abs() < 0.01);
		assert!((payouts[2] - 90.0).abs() < 0.01);
	}

	#[test]
	fn test_default_tables_not_empty() {
		let tables = default_tables();
		assert!(!tables.is_empty());
	}

	#[test]
	fn test_default_tables_have_valid_ids() {
		let tables = default_tables();
		for table in tables {
			assert!(!table.id.is_empty());
			assert!(!table.name.is_empty());
		}
	}
}
