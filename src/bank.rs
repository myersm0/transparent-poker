use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::logging;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
	pub bankroll: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfilesFile {
	#[serde(default = "default_bankroll")]
	default_bankroll: f32,
	#[serde(default)]
	profiles: HashMap<String, PlayerProfile>,
}

fn default_bankroll() -> f32 {
	1000.0
}

impl Default for ProfilesFile {
	fn default() -> Self {
		Self {
			default_bankroll: default_bankroll(),
			profiles: HashMap::new(),
		}
	}
}

#[derive(Debug)]
pub struct InsufficientFunds {
	pub player_id: String,
	pub required: f32,
	pub available: f32,
}

impl std::fmt::Display for InsufficientFunds {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{} has insufficient funds: needs ${:.2}, has ${:.2}",
			self.player_id, self.required, self.available
		)
	}
}

impl std::error::Error for InsufficientFunds {}

pub struct Bank {
	profiles: HashMap<String, PlayerProfile>,
	default_bankroll: f32,
	path: PathBuf,
}

fn normalize_id(id: &str) -> String {
	id.to_lowercase()
}

impl Bank {
	pub fn load() -> Result<Self, String> {
		let path = Self::config_path()?;

		let file = if path.exists() {
			let content = fs::read_to_string(&path)
				.map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
			toml::from_str(&content)
				.map_err(|e| format!("Failed to parse profiles: {}", e))?
		} else {
			ProfilesFile::default()
		};

		// Normalize all profile keys to lowercase
		let profiles: HashMap<String, PlayerProfile> = file.profiles
			.into_iter()
			.map(|(k, v)| (normalize_id(&k), v))
			.collect();

		Ok(Self {
			profiles,
			default_bankroll: file.default_bankroll,
			path,
		})
	}

	#[cfg(test)]
	pub fn new_for_testing(profiles: HashMap<String, PlayerProfile>) -> Self {
		Self {
			profiles,
			default_bankroll: 1000.0,
			path: PathBuf::from("/tmp/test.toml"),
		}
	}

	fn config_path() -> Result<PathBuf, String> {
		if let Some(config_dir) = dirs::config_dir() {
			let dir = config_dir.join("transparent-poker");
			fs::create_dir_all(&dir)
				.map_err(|e| format!("Failed to create config dir: {}", e))?;
			Ok(dir.join("profiles.toml"))
		} else {
			Ok(PathBuf::from("config/profiles.toml"))
		}
	}

	pub fn get(&self, id: &str) -> PlayerProfile {
		let id = normalize_id(id);
		self.profiles.get(&id).cloned().unwrap_or(PlayerProfile {
			bankroll: self.default_bankroll,
		})
	}

	pub fn get_bankroll(&self, id: &str) -> f32 {
		let id = normalize_id(id);
		self.profiles
			.get(&id)
			.map(|p| p.bankroll)
			.unwrap_or(self.default_bankroll)
	}

	pub fn ensure_exists(&mut self, id: &str) {
		let id = normalize_id(id);
		if !self.profiles.contains_key(&id) {
			self.profiles.insert(
				id,
				PlayerProfile {
					bankroll: self.default_bankroll,
				},
			);
		}
	}

	pub fn register(&mut self, id: &str, bankroll: f32) {
		let id = normalize_id(id);
		self.profiles.insert(
			id.clone(),
			PlayerProfile { bankroll },
		);
		logging::log("Bank", "REGISTER", &format!("{}: ${:.2}", id, bankroll));
	}

	pub fn debit(&mut self, id: &str, amount: f32) -> Result<(), InsufficientFunds> {
		let id = normalize_id(id);
		self.ensure_exists(&id);
		let profile = self.profiles.get_mut(&id).expect("profile exists after ensure_exists");

		if profile.bankroll < amount {
			return Err(InsufficientFunds {
				player_id: id,
				required: amount,
				available: profile.bankroll,
			});
		}

		profile.bankroll -= amount;
		logging::log("Bank", "DEBIT", &format!("{}: -${:.2} (bal: ${:.2})", id, amount, profile.bankroll));
		Ok(())
	}

	pub fn credit(&mut self, id: &str, amount: f32) {
		let id = normalize_id(id);
		self.ensure_exists(&id);
		let profile = self.profiles.get_mut(&id).expect("profile exists after ensure_exists");
		profile.bankroll += amount;
		logging::log("Bank", "CREDIT", &format!("{}: +${:.2} (bal: ${:.2})", id, amount, profile.bankroll));
	}

	pub fn buyin(&mut self, id: &str, amount: f32, table_id: &str) -> Result<(), InsufficientFunds> {
		let id = normalize_id(id);
		self.debit(&id, amount)?;
		logging::log("Bank", "BUYIN", &format!("{}: ${:.2} for table {}", id, amount, table_id));
		Ok(())
	}

	pub fn cashout(&mut self, id: &str, amount: f32, table_id: &str) {
		let id = normalize_id(id);
		self.credit(&id, amount);
		logging::log("Bank", "CASHOUT", &format!("{}: ${:.2} from table {}", id, amount, table_id));
	}

	pub fn award_prize(&mut self, id: &str, amount: f32, place: usize) {
		let id = normalize_id(id);
		self.credit(&id, amount);
		logging::log("Bank", "PRIZE", &format!("{}: ${:.2} ({})", id, amount, ordinal(place)));
	}

	pub fn profile_exists(&self, id: &str) -> bool {
		let id = normalize_id(id);
		self.profiles.contains_key(&id)
	}

	pub fn list_players(&self) -> Vec<(&str, &PlayerProfile)> {
		self.profiles.iter().map(|(k, v)| (k.as_str(), v)).collect()
	}

	pub fn save(&self) -> Result<(), String> {
		let file = ProfilesFile {
			default_bankroll: self.default_bankroll,
			profiles: self.profiles.clone(),
		};

		let content = toml::to_string_pretty(&file)
			.map_err(|e| format!("Failed to serialize profiles: {}", e))?;

		fs::write(&self.path, content)
			.map_err(|e| format!("Failed to write {}: {}", self.path.display(), e))?;

		Ok(())
	}
}

fn ordinal(n: usize) -> String {
	match n {
		1 => "1st".to_string(),
		2 => "2nd".to_string(),
		3 => "3rd".to_string(),
		_ => format!("{}th", n),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn test_bank() -> Bank {
		Bank {
			profiles: HashMap::new(),
			default_bankroll: 1000.0,
			path: PathBuf::from("/tmp/test.toml"),
		}
	}

	#[test]
	fn test_debit_credit() {
		let mut bank = test_bank();

		bank.credit("alice", 500.0);
		assert_eq!(bank.get_bankroll("alice"), 1500.0);

		bank.debit("alice", 200.0).unwrap();
		assert_eq!(bank.get_bankroll("alice"), 1300.0);

		let err = bank.debit("alice", 2000.0).unwrap_err();
		assert_eq!(err.available, 1300.0);
		assert_eq!(err.required, 2000.0);
	}

	#[test]
	fn test_get_default_bankroll() {
		let bank = test_bank();
		assert_eq!(bank.get_bankroll("unknown_player"), 1000.0);
	}

	#[test]
	fn test_get_profile_default() {
		let bank = test_bank();
		let profile = bank.get("unknown_player");
		assert_eq!(profile.bankroll, 1000.0);
	}

	#[test]
	fn test_ensure_exists_creates_profile() {
		let mut bank = test_bank();
		assert!(!bank.profiles.contains_key("bob"));
		bank.ensure_exists("bob");
		assert!(bank.profiles.contains_key("bob"));
		assert_eq!(bank.get_bankroll("bob"), 1000.0);
	}

	#[test]
	fn test_ensure_exists_preserves_existing() {
		let mut bank = test_bank();
		bank.credit("bob", 500.0);
		bank.ensure_exists("bob");
		assert_eq!(bank.get_bankroll("bob"), 1500.0);
	}

	#[test]
	fn test_buyin_debits_correctly() {
		let mut bank = test_bank();
		bank.ensure_exists("alice");
		bank.buyin("alice", 100.0, "table-1").unwrap();
		assert_eq!(bank.get_bankroll("alice"), 900.0);
	}

	#[test]
	fn test_buyin_insufficient_funds() {
		let mut bank = test_bank();
		bank.ensure_exists("alice");
		let result = bank.buyin("alice", 2000.0, "table-1");
		assert!(result.is_err());
	}

	#[test]
	fn test_cashout_credits_correctly() {
		let mut bank = test_bank();
		bank.ensure_exists("alice");
		bank.cashout("alice", 500.0, "table-1");
		assert_eq!(bank.get_bankroll("alice"), 1500.0);
	}

	#[test]
	fn test_award_prize() {
		let mut bank = test_bank();
		bank.award_prize("winner", 1000.0, 1);
		assert_eq!(bank.get_bankroll("winner"), 2000.0);
	}

	#[test]
	fn test_list_players_empty() {
		let bank = test_bank();
		assert!(bank.list_players().is_empty());
	}

	#[test]
	fn test_list_players_with_profiles() {
		let mut bank = test_bank();
		bank.ensure_exists("alice");
		bank.ensure_exists("bob");
		let players = bank.list_players();
		assert_eq!(players.len(), 2);
	}

	#[test]
	fn test_ordinal() {
		assert_eq!(ordinal(1), "1st");
		assert_eq!(ordinal(2), "2nd");
		assert_eq!(ordinal(3), "3rd");
		assert_eq!(ordinal(4), "4th");
		assert_eq!(ordinal(11), "11th");
	}

	#[test]
	fn test_insufficient_funds_display() {
		let err = InsufficientFunds {
			player_id: "alice".to_string(),
			required: 500.0,
			available: 100.0,
		};
		let msg = format!("{}", err);
		assert!(msg.contains("alice"));
		assert!(msg.contains("500"));
		assert!(msg.contains("100"));
	}

	#[test]
	fn test_case_insensitive_ids() {
		let mut bank = test_bank();
		bank.credit("Alice", 500.0);
		assert_eq!(bank.get_bankroll("alice"), 1500.0);
		assert_eq!(bank.get_bankroll("ALICE"), 1500.0);
		assert_eq!(bank.get_bankroll("Alice"), 1500.0);
		assert!(bank.profile_exists("alice"));
		assert!(bank.profile_exists("ALICE"));
	}
}
