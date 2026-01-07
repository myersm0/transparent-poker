use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

fn config_paths(filename: &str) -> Vec<PathBuf> {
	let mut paths = Vec::new();

	if let Some(home) = std::env::var_os("HOME") {
		let user_config = PathBuf::from(home).join(".config/poker-terminal").join(filename);
		paths.push(user_config);
	}

	paths.push(PathBuf::from("config").join(filename));

	paths
}

fn find_config(filename: &str) -> Option<PathBuf> {
	config_paths(filename).into_iter().find(|p| p.exists())
}

pub fn resolve_config(filename: &str) -> Result<PathBuf, String> {
	find_config(filename).ok_or_else(|| {
		let searched: Vec<_> = config_paths(filename)
			.iter()
			.map(|p| p.display().to_string())
			.collect();
		format!("Config file '{}' not found. Searched: {}", filename, searched.join(", "))
	})
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerConfig {
	pub id: String,
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default = "default_version")]
	pub version: String,
	#[serde(default = "default_join_probability")]
	pub join_probability: f32,
	pub strategy: String,
	#[serde(default)]
	pub strategy_model: Option<String>,
}

fn default_version() -> String {
	"0.1".to_string()
}

fn default_join_probability() -> f32 {
	0.5
}

impl PlayerConfig {
	pub fn display_name(&self) -> String {
		self.name.clone().unwrap_or_else(|| {
			let mut chars = self.id.chars();
			match chars.next() {
				None => self.id.clone(),
				Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
			}
		})
	}
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayersFile {
	#[serde(rename = "players")]
	pub players: Vec<PlayerConfig>,
}

pub fn load_players<P: AsRef<Path>>(path: P) -> Result<Vec<PlayerConfig>, String> {
	let content = fs::read_to_string(&path)
		.map_err(|e| format!("Failed to read {}: {}", path.as_ref().display(), e))?;

	let file: PlayersFile = toml::from_str(&content)
		.map_err(|e| format!("Failed to parse players config: {}", e))?;

	Ok(file.players)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
	pub id: String,
	pub description: String,
	#[serde(default)]
	pub advisor_cost: u32,
	#[serde(default = "default_max_tokens")]
	pub max_tokens: u32,
	#[serde(default)]
	pub input_cost_per_mtok: f64,
	#[serde(default)]
	pub output_cost_per_mtok: f64,
}

fn default_max_tokens() -> u32 {
	100
}

impl ModelConfig {
	pub fn calculate_cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
		let input = (input_tokens as f64) * self.input_cost_per_mtok / 1_000_000.0;
		let output = (output_tokens as f64) * self.output_cost_per_mtok / 1_000_000.0;
		input + output
	}
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsDefaults {
	pub opponent_execution: String,
	pub opponent_strategy: String,
	pub advisor_quick: String,
	pub advisor_tactical: String,
	pub advisor_deep: String,
	pub pit_boss: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CachingConfig {
	#[serde(default)]
	pub enabled: bool,
	#[serde(default)]
	pub static_prefix_file: Option<String>,
	#[serde(default)]
	pub ttl_minutes: u32,
}

impl Default for CachingConfig {
	fn default() -> Self {
		Self {
			enabled: false,
			static_prefix_file: None,
			ttl_minutes: 5,
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsConfig {
	pub haiku: ModelConfig,
	pub sonnet: ModelConfig,
	pub opus: ModelConfig,
	pub defaults: ModelsDefaults,
	#[serde(default)]
	pub caching: CachingConfig,
}

impl ModelsConfig {
	pub fn get(&self, name: &str) -> Option<&ModelConfig> {
		match name {
			"haiku" => Some(&self.haiku),
			"sonnet" => Some(&self.sonnet),
			"opus" => Some(&self.opus),
			_ => None,
		}
	}

	pub fn execution_model(&self) -> &ModelConfig {
		self.get(&self.defaults.opponent_execution)
			.expect(&format!(
				"Invalid opponent_execution model '{}' in config. Must be haiku, sonnet, or opus.",
				self.defaults.opponent_execution
			))
	}

	pub fn strategy_model(&self) -> &ModelConfig {
		self.get(&self.defaults.opponent_strategy)
			.expect(&format!(
				"Invalid opponent_strategy model '{}' in config. Must be haiku, sonnet, or opus.",
				self.defaults.opponent_strategy
			))
	}
}

pub fn load_models<P: AsRef<Path>>(path: P) -> Result<ModelsConfig, String> {
	let content = fs::read_to_string(&path)
		.map_err(|e| format!("Failed to read {}: {}", path.as_ref().display(), e))?;

	toml::from_str(&content)
		.map_err(|e| format!("Failed to parse models config: {}", e))
}

#[derive(Debug, Clone, Deserialize)]
pub struct StakesConfig {
	pub id: String,
	pub name: String,
	pub small_blind: f32,
	pub big_blind: f32,
	pub buy_in: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GamesDefaults {
	#[serde(default = "default_table_size")]
	pub table_size: usize,
	#[serde(default = "default_model")]
	pub model: String,
	#[serde(default = "default_bankroll")]
	pub starting_bankroll: f32,
	#[serde(default = "default_action_delay")]
	pub action_delay_ms: u64,
}

fn default_table_size() -> usize { 10 }
fn default_model() -> String { "claude-haiku-4-5".to_string() }
fn default_bankroll() -> f32 { 10000.0 }
fn default_action_delay() -> u64 { 500 }

#[derive(Debug, Clone, Deserialize)]
pub struct GamesConfig {
	pub defaults: GamesDefaults,
	pub stakes: Vec<StakesConfig>,
}

pub fn load_games<P: AsRef<Path>>(path: P) -> Result<GamesConfig, String> {
	let content = fs::read_to_string(&path)
		.map_err(|e| format!("Failed to read {}: {}", path.as_ref().display(), e))?;

	toml::from_str(&content)
		.map_err(|e| format!("Failed to parse games config: {}", e))
}

pub fn load_models_auto() -> Result<ModelsConfig, String> {
	let path = resolve_config("models.toml")?;
	load_models(&path)
}

pub fn load_strategies_auto() -> Result<crate::strategy::StrategyStore, String> {
	let path = resolve_config("strategies.toml")?;
	crate::strategy::StrategyStore::load(&path)
}

pub fn load_players_auto() -> Result<Vec<PlayerConfig>, String> {
	let path = resolve_config("players.toml")?;
	load_players(&path)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_player_config_display_name_with_name() {
		let config = PlayerConfig {
			id: "test".to_string(),
			name: Some("Test Player".to_string()),
			version: "0.1".to_string(),
			join_probability: 0.5,
			strategy: "tag".to_string(),
			strategy_model: None,
		};
		assert_eq!(config.display_name(), "Test Player");
	}

	#[test]
	fn test_player_config_display_name_capitalizes_id() {
		let config = PlayerConfig {
			id: "alice".to_string(),
			name: None,
			version: "0.1".to_string(),
			join_probability: 0.5,
			strategy: "tag".to_string(),
			strategy_model: None,
		};
		assert_eq!(config.display_name(), "Alice");
	}

	#[test]
	fn test_player_config_display_name_preserves_rest() {
		let config = PlayerConfig {
			id: "mcDonald".to_string(),
			name: None,
			version: "0.1".to_string(),
			join_probability: 0.5,
			strategy: "tag".to_string(),
			strategy_model: None,
		};
		assert_eq!(config.display_name(), "McDonald");
	}

	#[test]
	fn test_model_config_calculate_cost() {
		let model = ModelConfig {
			id: "haiku".to_string(),
			description: "Fast model".to_string(),
			advisor_cost: 10,
			max_tokens: 100,
			input_cost_per_mtok: 0.25,
			output_cost_per_mtok: 1.25,
		};
		let cost = model.calculate_cost(4000, 1000);
		let expected = (4000.0 * 0.25 / 1_000_000.0) + (1000.0 * 1.25 / 1_000_000.0);
		assert!((cost - expected).abs() < 0.0000001);
	}

	#[test]
	fn test_model_config_calculate_cost_large_values() {
		let model = ModelConfig {
			id: "opus".to_string(),
			description: "Smart model".to_string(),
			advisor_cost: 100,
			max_tokens: 4096,
			input_cost_per_mtok: 15.0,
			output_cost_per_mtok: 75.0,
		};
		let cost = model.calculate_cost(10_000, 2_000);
		let expected = (10_000.0 * 15.0 / 1_000_000.0) + (2_000.0 * 75.0 / 1_000_000.0);
		assert!((cost - expected).abs() < 0.0000001);
	}

	#[test]
	fn test_models_config_get() {
		let config = ModelsConfig {
			haiku: ModelConfig {
				id: "haiku".to_string(),
				description: "Fast".to_string(),
				advisor_cost: 10,
				max_tokens: 100,
				input_cost_per_mtok: 0.25,
				output_cost_per_mtok: 1.25,
			},
			sonnet: ModelConfig {
				id: "sonnet".to_string(),
				description: "Balanced".to_string(),
				advisor_cost: 50,
				max_tokens: 200,
				input_cost_per_mtok: 3.0,
				output_cost_per_mtok: 15.0,
			},
			opus: ModelConfig {
				id: "opus".to_string(),
				description: "Smart".to_string(),
				advisor_cost: 100,
				max_tokens: 400,
				input_cost_per_mtok: 15.0,
				output_cost_per_mtok: 75.0,
			},
			defaults: ModelsDefaults {
				opponent_execution: "haiku".to_string(),
				opponent_strategy: "sonnet".to_string(),
				advisor_quick: "haiku".to_string(),
				advisor_tactical: "sonnet".to_string(),
				advisor_deep: "opus".to_string(),
				pit_boss: "opus".to_string(),
			},
			caching: CachingConfig::default(),
		};

		assert!(config.get("haiku").is_some());
		assert!(config.get("sonnet").is_some());
		assert!(config.get("opus").is_some());
		assert!(config.get("gpt4").is_none());
	}

	#[test]
	fn test_caching_config_default() {
		let config = CachingConfig::default();
		assert!(!config.enabled);
		assert!(config.static_prefix_file.is_none());
		assert_eq!(config.ttl_minutes, 5);
	}

	#[test]
	fn test_default_join_probability() {
		assert_eq!(default_join_probability(), 0.5);
	}

	#[test]
	fn test_default_version() {
		assert_eq!(default_version(), "0.1");
	}

	#[test]
	fn test_default_max_tokens() {
		assert_eq!(default_max_tokens(), 100);
	}
}
