use std::fs;
use std::path::PathBuf;

use ratatui::style::Color;
use ratatui::widgets::BorderType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
	pub background_color: String,

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
	pub chat_text_color: String,
	pub system_message_color: String,

	pub menu_border_color: String,
	pub menu_title_color: String,
	pub menu_text_color: String,
	pub menu_selected_color: String,
	pub menu_selected_bg: String,
	pub menu_unselected_color: String,
	pub menu_host_marker_color: String,
	pub menu_ai_marker_color: String,
	pub menu_highlight_color: String,

	pub status_watching_color: String,
	pub status_watching_border: String,
	pub status_your_turn_color: String,
	pub status_your_turn_border: String,
	pub status_quit_color: String,
	pub status_quit_border: String,
	pub status_game_over_color: String,
	pub status_game_over_border: String,
}

impl Default for Theme {
	fn default() -> Self {
		Self {
			background_color: "#1A1A1A".to_string(),

			hero_border_style: "double".to_string(),
			hero_border_color: "#00D7D7".to_string(),

			actor_border_color: "#D7D700".to_string(),
			actor_name_color: "#D7D700".to_string(),

			folded_border_color: "#808080".to_string(),
			folded_text_color: "#808080".to_string(),

			eliminated_border_color: "#4A4A4A".to_string(),
			eliminated_text_color: "#4A4A4A".to_string(),

			all_in_border_color: "#D700D7".to_string(),

			winner_border_color: "#00D700".to_string(),
			winner_name_color: "#00D700".to_string(),

			default_border_color: "#D7D7D7".to_string(),

			stack_color: "#00D700".to_string(),
			bet_color: "#D7D700".to_string(),
			pot_color: "#D7D700".to_string(),

			red_suit_color: "#D70000".to_string(),
			black_suit_color: "#D7D7D7".to_string(),
			hidden_card_color: "#0087D7".to_string(),

			table_border_color: "#00D700".to_string(),
			chat_border_color: "#0087D7".to_string(),
			chat_text_color: "#B0B0B0".to_string(),
			system_message_color: "#00D7D7".to_string(),

			menu_border_color: "#00D700".to_string(),
			menu_title_color: "#D7D7D7".to_string(),
			menu_text_color: "#D7D7D7".to_string(),
			menu_selected_color: "#D7D700".to_string(),
			menu_selected_bg: "#303030".to_string(),
			menu_unselected_color: "#808080".to_string(),
			menu_host_marker_color: "#00D7D7".to_string(),
			menu_ai_marker_color: "#D700D7".to_string(),
			menu_highlight_color: "#00D7D7".to_string(),

			status_watching_color: "#808080".to_string(),
			status_watching_border: "#808080".to_string(),
			status_your_turn_color: "#D7D700".to_string(),
			status_your_turn_border: "#D7D700".to_string(),
			status_quit_color: "#D70000".to_string(),
			status_quit_border: "#D70000".to_string(),
			status_game_over_color: "#00D700".to_string(),
			status_game_over_border: "#00D700".to_string(),
		}
	}
}

impl Theme {
	pub fn load(name: Option<&str>) -> Self {
		let theme_name = name
			.map(|s| s.to_string())
			.or_else(|| std::env::var("POKER_THEME").ok())
			.unwrap_or_else(|| "dark".to_string());

		Self::load_named(&theme_name).unwrap_or_else(|e| {
			eprintln!("Warning: {}. Using default theme.", e);
			Self::default()
		})
	}

	pub fn load_named(name: &str) -> Result<Self, String> {
		if let Some(config_dir) = dirs::config_dir() {
			let user_path = config_dir
				.join("transparent-poker")
				.join("themes")
				.join(format!("{}.toml", name));
			if user_path.exists() {
				return Self::from_file(&user_path);
			}
		}

		let repo_path = PathBuf::from("config")
			.join("themes")
			.join(format!("{}.toml", name));
		if repo_path.exists() {
			return Self::from_file(&repo_path);
		}

		Err(format!("Theme '{}' not found", name))
	}

	fn from_file(path: &PathBuf) -> Result<Self, String> {
		let contents = fs::read_to_string(path)
			.map_err(|e| format!("Failed to read theme file: {}", e))?;
		toml::from_str(&contents)
			.map_err(|e| format!("Failed to parse theme: {}", e))
	}

	pub fn background(&self) -> Color {
		parse_color(&self.background_color)
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

	pub fn chat_text(&self) -> Color {
		parse_color(&self.chat_text_color)
	}

	pub fn system_message(&self) -> Color {
		parse_color(&self.system_message_color)
	}

	pub fn menu_border(&self) -> Color {
		parse_color(&self.menu_border_color)
	}

	pub fn menu_title(&self) -> Color {
		parse_color(&self.menu_title_color)
	}

	pub fn menu_text(&self) -> Color {
		parse_color(&self.menu_text_color)
	}

	pub fn menu_selected(&self) -> Color {
		parse_color(&self.menu_selected_color)
	}

	pub fn menu_selected_bg(&self) -> Color {
		parse_color(&self.menu_selected_bg)
	}

	pub fn menu_unselected(&self) -> Color {
		parse_color(&self.menu_unselected_color)
	}

	pub fn menu_host_marker(&self) -> Color {
		parse_color(&self.menu_host_marker_color)
	}

	pub fn menu_ai_marker(&self) -> Color {
		parse_color(&self.menu_ai_marker_color)
	}

	pub fn menu_highlight(&self) -> Color {
		parse_color(&self.menu_highlight_color)
	}

	pub fn status_watching(&self) -> Color {
		parse_color(&self.status_watching_color)
	}

	pub fn status_watching_border(&self) -> Color {
		parse_color(&self.status_watching_border)
	}

	pub fn status_your_turn(&self) -> Color {
		parse_color(&self.status_your_turn_color)
	}

	pub fn status_your_turn_border(&self) -> Color {
		parse_color(&self.status_your_turn_border)
	}

	pub fn status_quit(&self) -> Color {
		parse_color(&self.status_quit_color)
	}

	pub fn status_quit_border(&self) -> Color {
		parse_color(&self.status_quit_border)
	}

	pub fn status_game_over(&self) -> Color {
		parse_color(&self.status_game_over_color)
	}

	pub fn status_game_over_border(&self) -> Color {
		parse_color(&self.status_game_over_border)
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
