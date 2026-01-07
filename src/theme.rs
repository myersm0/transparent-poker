use std::fs;
use std::path::PathBuf;

use ratatui::style::Color;
use ratatui::widgets::BorderType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
	pub hero_border_style: String,
	pub hero_border_color: String,

	pub actor_border_color: String,
	pub actor_name_color: String,

	pub folded_border_color: String,
	pub folded_text_color: String,

	pub eliminated_border_color: String,
	pub eliminated_text_color: String,

	pub all_in_border_color: String,

	pub winner_border_color: String,
	pub winner_name_color: String,

	pub default_border_color: String,

	pub stack_color: String,
	pub bet_color: String,
	pub pot_color: String,

	pub red_suit_color: String,
	pub black_suit_color: String,
	pub hidden_card_color: String,

	pub table_border_color: String,
	pub chat_border_color: String,
	pub system_message_color: String,
}

impl Default for Theme {
	fn default() -> Self {
		Self {
			hero_border_style: "double".to_string(),
			hero_border_color: "cyan".to_string(),

			actor_border_color: "yellow".to_string(),
			actor_name_color: "yellow".to_string(),

			folded_border_color: "gray".to_string(),
			folded_text_color: "gray".to_string(),

			eliminated_border_color: "dark_gray".to_string(),
			eliminated_text_color: "dark_gray".to_string(),

			all_in_border_color: "magenta".to_string(),

			winner_border_color: "green".to_string(),
			winner_name_color: "green".to_string(),

			default_border_color: "white".to_string(),

			stack_color: "green".to_string(),
			bet_color: "yellow".to_string(),
			pot_color: "yellow".to_string(),

			red_suit_color: "red".to_string(),
			black_suit_color: "white".to_string(),
			hidden_card_color: "blue".to_string(),

			table_border_color: "green".to_string(),
			chat_border_color: "blue".to_string(),
			system_message_color: "cyan".to_string(),
		}
	}
}

impl Theme {
	pub fn load() -> Self {
		if let Some(path) = Self::config_path() {
			if let Ok(contents) = fs::read_to_string(&path) {
				if let Ok(theme) = toml::from_str(&contents) {
					return theme;
				}
			}
		}
		Self::default()
	}

	fn config_path() -> Option<PathBuf> {
		if let Some(config_dir) = dirs::config_dir() {
			let user_path = config_dir.join("poker-terminal").join("theme.toml");
			if user_path.exists() {
				return Some(user_path);
			}
		}
		let repo_path = PathBuf::from("config/theme.toml");
		if repo_path.exists() {
			return Some(repo_path);
		}
		None
	}

	pub fn hero_border_type(&self) -> BorderType {
		parse_border_type(&self.hero_border_style)
	}

	pub fn hero_border(&self) -> Color {
		parse_color(&self.hero_border_color)
	}

	pub fn actor_border(&self) -> Color {
		parse_color(&self.actor_border_color)
	}

	pub fn actor_name(&self) -> Color {
		parse_color(&self.actor_name_color)
	}

	pub fn folded_border(&self) -> Color {
		parse_color(&self.folded_border_color)
	}

	pub fn folded_text(&self) -> Color {
		parse_color(&self.folded_text_color)
	}

	pub fn eliminated_border(&self) -> Color {
		parse_color(&self.eliminated_border_color)
	}

	pub fn eliminated_text(&self) -> Color {
		parse_color(&self.eliminated_text_color)
	}

	pub fn all_in_border(&self) -> Color {
		parse_color(&self.all_in_border_color)
	}

	pub fn winner_border(&self) -> Color {
		parse_color(&self.winner_border_color)
	}

	pub fn winner_name(&self) -> Color {
		parse_color(&self.winner_name_color)
	}

	pub fn default_border(&self) -> Color {
		parse_color(&self.default_border_color)
	}

	pub fn stack(&self) -> Color {
		parse_color(&self.stack_color)
	}

	pub fn bet(&self) -> Color {
		parse_color(&self.bet_color)
	}

	pub fn pot(&self) -> Color {
		parse_color(&self.pot_color)
	}

	pub fn red_suit(&self) -> Color {
		parse_color(&self.red_suit_color)
	}

	pub fn black_suit(&self) -> Color {
		parse_color(&self.black_suit_color)
	}

	pub fn hidden_card(&self) -> Color {
		parse_color(&self.hidden_card_color)
	}

	pub fn table_border(&self) -> Color {
		parse_color(&self.table_border_color)
	}

	pub fn chat_border(&self) -> Color {
		parse_color(&self.chat_border_color)
	}

	pub fn system_message(&self) -> Color {
		parse_color(&self.system_message_color)
	}
}

fn parse_color(s: &str) -> Color {
	match s.to_lowercase().as_str() {
		"black" => Color::Black,
		"red" => Color::Red,
		"green" => Color::Green,
		"yellow" => Color::Yellow,
		"blue" => Color::Blue,
		"magenta" => Color::Magenta,
		"cyan" => Color::Cyan,
		"gray" | "grey" => Color::Gray,
		"dark_gray" | "dark_grey" | "darkgray" | "darkgrey" => Color::DarkGray,
		"light_red" | "lightred" => Color::LightRed,
		"light_green" | "lightgreen" => Color::LightGreen,
		"light_yellow" | "lightyellow" => Color::LightYellow,
		"light_blue" | "lightblue" => Color::LightBlue,
		"light_magenta" | "lightmagenta" => Color::LightMagenta,
		"light_cyan" | "lightcyan" => Color::LightCyan,
		"white" => Color::White,
		_ => {
			if let Some(hex) = s.strip_prefix('#') {
				if let Ok(rgb) = u32::from_str_radix(hex, 16) {
					let r = ((rgb >> 16) & 0xFF) as u8;
					let g = ((rgb >> 8) & 0xFF) as u8;
					let b = (rgb & 0xFF) as u8;
					return Color::Rgb(r, g, b);
				}
			}
			if s.starts_with("rgb(") && s.ends_with(')') {
				let inner = &s[4..s.len() - 1];
				let parts: Vec<&str> = inner.split(',').collect();
				if parts.len() == 3 {
					if let (Ok(r), Ok(g), Ok(b)) = (
						parts[0].trim().parse::<u8>(),
						parts[1].trim().parse::<u8>(),
						parts[2].trim().parse::<u8>(),
					) {
						return Color::Rgb(r, g, b);
					}
				}
			}
			Color::White
		}
	}
}

fn parse_border_type(s: &str) -> BorderType {
	match s.to_lowercase().as_str() {
		"double" => BorderType::Double,
		"thick" => BorderType::Thick,
		"rounded" => BorderType::Rounded,
		_ => BorderType::Plain,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_color_names() {
		assert_eq!(parse_color("red"), Color::Red);
		assert_eq!(parse_color("GREEN"), Color::Green);
		assert_eq!(parse_color("dark_gray"), Color::DarkGray);
		assert_eq!(parse_color("darkgrey"), Color::DarkGray);
	}

	#[test]
	fn test_parse_color_hex() {
		assert_eq!(parse_color("#FF0000"), Color::Rgb(255, 0, 0));
		assert_eq!(parse_color("#00ff00"), Color::Rgb(0, 255, 0));
	}

	#[test]
	fn test_parse_color_rgb() {
		assert_eq!(parse_color("rgb(255, 128, 0)"), Color::Rgb(255, 128, 0));
	}

	#[test]
	fn test_parse_border_type() {
		assert_eq!(parse_border_type("double"), BorderType::Double);
		assert_eq!(parse_border_type("THICK"), BorderType::Thick);
		assert_eq!(parse_border_type("rounded"), BorderType::Rounded);
		assert_eq!(parse_border_type("plain"), BorderType::Plain);
		assert_eq!(parse_border_type("unknown"), BorderType::Plain);
	}
}
