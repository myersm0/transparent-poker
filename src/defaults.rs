use include_dir::{include_dir, Dir};
use std::fs;
use std::path::Path;

static CONFIG_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/config");

pub fn ensure_config() {
	let Some(user_config) = dirs::config_dir() else {
		return;
	};
	let dest = user_config.join("transparent-poker");

	extract_dir(&CONFIG_DIR, &dest);
}

fn extract_dir(dir: &Dir, dest: &Path) {
	for file in dir.files() {
		let file_dest = dest.join(file.path());
		if !file_dest.exists() {
			if let Some(parent) = file_dest.parent() {
				let _ = fs::create_dir_all(parent);
			}
			let _ = fs::write(&file_dest, file.contents());
		}
	}

	for subdir in dir.dirs() {
		extract_dir(subdir, dest);
	}
}

pub fn list_themes() -> Vec<String> {
	let mut themes = Vec::new();

	if let Some(dir) = CONFIG_DIR.get_dir("themes") {
		for file in dir.files() {
			if let Some(name) = file.path().file_stem() {
				themes.push(name.to_string_lossy().to_string());
			}
		}
	}

	if let Some(config_dir) = dirs::config_dir() {
		let user_themes = config_dir.join("transparent-poker").join("themes");
		if let Ok(entries) = fs::read_dir(user_themes) {
			for entry in entries.flatten() {
				let path = entry.path();
				if path.extension().map(|e| e == "toml").unwrap_or(false) {
					if let Some(stem) = path.file_stem() {
						let name = stem.to_string_lossy().to_string();
						if !themes.contains(&name) {
							themes.push(name);
						}
					}
				}
			}
		}
	}

	themes.sort();
	themes
}
