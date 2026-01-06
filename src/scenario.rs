use std::fs;
use std::path::Path;
use crate::view::TableView;

pub fn load_scenario<P: AsRef<Path>>(path: P) -> Result<TableView, String> {
	let content = fs::read_to_string(path)
		.map_err(|e| format!("Failed to read file: {}", e))?;
	toml::from_str(&content)
		.map_err(|e| format!("Failed to parse TOML: {}", e))
}

pub fn load_scenarios_from_dir<P: AsRef<Path>>(dir: P) -> Vec<TableView> {
	let dir = dir.as_ref();
	let mut scenarios = Vec::new();

	if let Ok(entries) = fs::read_dir(dir) {
		for entry in entries.flatten() {
			let path = entry.path();
			if path.extension().map(|e| e == "toml").unwrap_or(false) {
				if let Ok(scenario) = load_scenario(&path) {
					scenarios.push(scenario);
				}
			}
		}
	}

	scenarios.sort_by_key(|s| s.hand_num);
	scenarios
}
