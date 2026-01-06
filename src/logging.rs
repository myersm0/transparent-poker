use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct LogState {
	file: Option<std::fs::File>,
	current_date: String,
	game_id: String,
	hand_num: u32,
}

static LOG_STATE: Mutex<LogState> = Mutex::new(LogState {
	file: None,
	current_date: String::new(),
	game_id: String::new(),
	hand_num: 0,
});

fn today() -> String {
	let secs = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_secs();
	let days = secs / 86400;
	let year = 1970 + (days / 365);
	let day_of_year = days % 365;
	let month = day_of_year / 30 + 1;
	let day = day_of_year % 30 + 1;
	format!("{:04}-{:02}-{:02}", year, month.min(12), day.min(31))
}

fn timestamp() -> String {
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap();
	let secs = now.as_secs();
	let millis = now.as_millis() % 1000;
	let hours = (secs / 3600) % 24;
	let mins = (secs / 60) % 60;
	let s = secs % 60;
	format!("{:02}:{:02}:{:02}.{:03}", hours, mins, s, millis)
}

fn ensure_log_file(state: &mut LogState) {
	let date = today();
	if state.current_date != date || state.file.is_none() {
		let _ = fs::create_dir_all("logs");
		let path = format!("logs/poker-{}.log", date);
		if let Ok(file) = OpenOptions::new()
			.create(true)
			.append(true)
			.open(&path)
		{
			state.file = Some(file);
			state.current_date = date;
		}
	}
}

pub fn set_game_id(game_id: u64) {
	if let Ok(mut state) = LOG_STATE.lock() {
		state.game_id = format!("{:08x}", game_id & 0xFFFFFFFF);
	}
}

pub fn set_hand_num(hand_num: u32) {
	if let Ok(mut state) = LOG_STATE.lock() {
		state.hand_num = hand_num;
	}
}

pub fn log(module: &str, log_type: &str, message: &str) {
	if let Ok(mut state) = LOG_STATE.lock() {
		ensure_log_file(&mut state);

		let game_id = if state.game_id.is_empty() { "--------" } else { &state.game_id };
		let line = format!(
			"[{}][{}][H{}][{}:{}] {}\n",
			timestamp(),
			game_id,
			state.hand_num,
			module,
			log_type,
			message
		);

		if let Some(ref mut file) = state.file {
			let _ = file.write_all(line.as_bytes());
			let _ = file.flush();
		}
	}
}

pub fn log_verbatim(module: &str, log_type: &str, label: &str, content: &str) {
	let single_line = content.replace('\n', " ").replace('\r', "");
	log(module, log_type, &format!("{}: <<<{}>>>", label, single_line));
}

pub mod engine {
	use super::log;

	pub fn hand_started(button: usize, num_players: usize) {
		log("Engine", "HAND", &format!("started button={} players={}", button, num_players));
	}

	pub fn action(player: &str, action_desc: &str, pot: f32) {
		log("Engine", "ACTION", &format!("{}: {} (pot: ${:.0})", player, action_desc, pot));
	}

	pub fn street(street: &str, board: &str) {
		if board.is_empty() {
			log("Engine", "STREET", street);
		} else {
			log("Engine", "STREET", &format!("{} {}", street, board));
		}
	}

	pub fn pot_awarded(player: &str, amount: f32, hand_desc: Option<&str>) {
		match hand_desc {
			Some(desc) => log("Engine", "POT", &format!("{} wins ${:.0} ({})", player, amount, desc)),
			None => log("Engine", "POT", &format!("{} wins ${:.0}", player, amount)),
		}
	}

	pub fn game_ended(reason: &str) {
		log("Engine", "GAME", &format!("ended: {}", reason));
	}
}

pub mod ai {
	use super::{log, log_verbatim};

	pub fn strategy(player: &str, msg: &str) {
		log("AI", "STRATEGY", &format!("{}: {}", player, msg));
	}

	pub fn rule(player: &str, msg: &str) {
		log("AI", "RULE", &format!("{}: {}", player, msg));
	}

	pub fn prompt(player: &str, prompt: &str) {
		log_verbatim("AI", "PROMPT", player, prompt);
	}

	pub fn response(player: &str, response: &str) {
		log_verbatim("AI", "RESPONSE", player, response);
	}

	pub fn decision(player: &str, source: &str, action: &str) {
		log("AI", "DECISION", &format!("{}: {} → {}", player, source, action));
	}

	pub fn error(player: &str, msg: &str) {
		log("AI", "ERROR", &format!("{}: {}", player, msg));
	}

	pub fn cost(player: &str, model: &str, input_tokens: u32, output_tokens: u32) {
		log("AI", "COST", &format!("{}: model={} in={} out={}", player, model, input_tokens, output_tokens));
	}
}

pub mod tui {
	use super::log;

	pub fn input(key: &str) {
		log("TUI", "INPUT", key);
	}

	pub fn action(action_desc: &str) {
		log("TUI", "ACTION", action_desc);
	}

	pub fn event(msg: &str) {
		log("TUI", "EVENT", msg);
	}
}
