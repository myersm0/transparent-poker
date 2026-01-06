#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Position {
	Utg,
	Mp,
	Co,
	Btn,
	Sb,
	Bb,
}

impl Position {
	pub fn from_seat(seat: usize, button: usize, num_players: usize) -> Self {
		if num_players == 0 {
			return Position::Utg;
		}

		let dist_from_btn = (seat + num_players - button) % num_players;

		match num_players {
			2 => match dist_from_btn {
				0 => Position::Btn,
				_ => Position::Bb,
			},
			3 => match dist_from_btn {
				0 => Position::Btn,
				1 => Position::Sb,
				_ => Position::Bb,
			},
			_ => {
				match dist_from_btn {
					0 => Position::Btn,
					1 => Position::Sb,
					2 => Position::Bb,
					d if d == num_players - 1 => Position::Co,
					d if d == num_players - 2 && num_players > 4 => Position::Mp,
					_ => Position::Utg,
				}
			}
		}
	}

	pub fn name(&self) -> &'static str {
		match self {
			Position::Utg => "UTG",
			Position::Mp => "MP",
			Position::Co => "CO",
			Position::Btn => "BTN",
			Position::Sb => "SB",
			Position::Bb => "BB",
		}
	}

	pub fn is_late(&self) -> bool {
		matches!(self, Position::Co | Position::Btn)
	}

	pub fn is_blind(&self) -> bool {
		matches!(self, Position::Sb | Position::Bb)
	}
}

impl std::fmt::Display for Position {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.name())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_heads_up() {
		assert_eq!(Position::from_seat(0, 0, 2), Position::Btn);
		assert_eq!(Position::from_seat(1, 0, 2), Position::Bb);
	}

	#[test]
	fn test_three_handed() {
		assert_eq!(Position::from_seat(0, 0, 3), Position::Btn);
		assert_eq!(Position::from_seat(1, 0, 3), Position::Sb);
		assert_eq!(Position::from_seat(2, 0, 3), Position::Bb);
	}

	#[test]
	fn test_six_handed() {
		assert_eq!(Position::from_seat(0, 0, 6), Position::Btn);
		assert_eq!(Position::from_seat(1, 0, 6), Position::Sb);
		assert_eq!(Position::from_seat(2, 0, 6), Position::Bb);
		assert_eq!(Position::from_seat(3, 0, 6), Position::Utg);
		assert_eq!(Position::from_seat(4, 0, 6), Position::Mp);
		assert_eq!(Position::from_seat(5, 0, 6), Position::Co);
	}
}
