#![allow(clippy::nonminimal_bool)]

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HandGroup {
	Premium,
	Strong,
	Solid,
	Playable,
	Speculative,
	Marginal,
	Trash,
}

impl HandGroup {
	pub fn name(&self) -> &'static str {
		match self {
			HandGroup::Premium => "Premium",
			HandGroup::Strong => "Strong",
			HandGroup::Solid => "Solid",
			HandGroup::Playable => "Playable",
			HandGroup::Speculative => "Speculative",
			HandGroup::Marginal => "Marginal",
			HandGroup::Trash => "Trash",
		}
	}

	pub fn from_name(name: &str) -> Option<HandGroup> {
		match name.to_lowercase().as_str() {
			"premium" => Some(HandGroup::Premium),
			"strong" => Some(HandGroup::Strong),
			"solid" => Some(HandGroup::Solid),
			"playable" => Some(HandGroup::Playable),
			"speculative" => Some(HandGroup::Speculative),
			"marginal" => Some(HandGroup::Marginal),
			"trash" => Some(HandGroup::Trash),
			_ => None,
		}
	}
}

impl fmt::Display for HandGroup {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.name())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoleCards {
	pub high_rank: u8,
	pub low_rank: u8,
	pub suited: bool,
	pub pair: bool,
}

impl HoleCards {
	pub fn new(rank1: u8, rank2: u8, suited: bool) -> Self {
		let (high_rank, low_rank) = if rank1 >= rank2 {
			(rank1, rank2)
		} else {
			(rank2, rank1)
		};
		Self {
			high_rank,
			low_rank,
			suited,
			pair: high_rank == low_rank,
		}
	}

	pub fn from_chars(r1: char, r2: char, suited: bool) -> Option<Self> {
		let rank1 = char_to_rank(r1)?;
		let rank2 = char_to_rank(r2)?;
		Some(Self::new(rank1, rank2, suited))
	}

	pub fn gap(&self) -> u8 {
		self.high_rank - self.low_rank
	}

	pub fn connected(&self) -> bool {
		self.gap() == 1
	}

	pub fn one_gapper(&self) -> bool {
		self.gap() == 2
	}

	pub fn classify(&self) -> HandGroup {
		if self.pair {
			classify_pair(self.high_rank)
		} else if self.suited {
			classify_suited(self.high_rank, self.low_rank, self.connected(), self.one_gapper())
		} else {
			classify_offsuit(self.high_rank, self.low_rank, self.connected())
		}
	}
}

pub fn char_to_rank(c: char) -> Option<u8> {
	match c.to_ascii_uppercase() {
		'2' => Some(2),
		'3' => Some(3),
		'4' => Some(4),
		'5' => Some(5),
		'6' => Some(6),
		'7' => Some(7),
		'8' => Some(8),
		'9' => Some(9),
		'T' => Some(10),
		'J' => Some(11),
		'Q' => Some(12),
		'K' => Some(13),
		'A' => Some(14),
		_ => None,
	}
}

pub fn rank_to_char(rank: u8) -> char {
	match rank {
		2 => '2',
		3 => '3',
		4 => '4',
		5 => '5',
		6 => '6',
		7 => '7',
		8 => '8',
		9 => '9',
		10 => 'T',
		11 => 'J',
		12 => 'Q',
		13 => 'K',
		14 => 'A',
		_ => '?',
	}
}

fn classify_pair(rank: u8) -> HandGroup {
	match rank {
		14 | 13 => HandGroup::Premium,
		12 => HandGroup::Strong,
		11 | 10 => HandGroup::Solid,
		9 | 8 => HandGroup::Playable,
		5..=7 => HandGroup::Speculative,
		_ => HandGroup::Marginal,
	}
}

fn classify_suited(high: u8, low: u8, connected: bool, one_gap: bool) -> HandGroup {
	if high == 14 && low == 13 {
		return HandGroup::Premium;
	}
	if (high == 14 && low >= 11) || (high == 13 && low == 12) {
		return HandGroup::Strong;
	}
	if high == 14 && low == 10 {
		return HandGroup::Solid;
	}
	if (high == 13 && low == 11) || (high == 12 && low == 11) || (high == 11 && low == 10) {
		return HandGroup::Solid;
	}
	if high == 14 {
		return HandGroup::Playable;
	}
	if high >= 10 && (connected || one_gap) {
		return HandGroup::Playable;
	}
	if connected && (5..=9).contains(&high) {
		return HandGroup::Speculative;
	}
	if one_gap && high >= 6 {
		return HandGroup::Speculative;
	}
	if high >= 12 && low <= 9 {
		return HandGroup::Marginal;
	}
	if high >= 8 {
		return HandGroup::Marginal;
	}
	HandGroup::Trash
}

fn classify_offsuit(high: u8, low: u8, connected: bool) -> HandGroup {
	if high == 14 && low == 13 {
		return HandGroup::Strong;
	}
	if high == 14 && low >= 11 {
		return HandGroup::Solid;
	}
	if high == 13 && low == 12 {
		return HandGroup::Solid;
	}
	if high == 14 && low == 10 {
		return HandGroup::Playable;
	}
	if (high == 13 && low == 11) || (high == 12 && low == 11) {
		return HandGroup::Playable;
	}
	if high >= 11 && low == 10 {
		return HandGroup::Marginal;
	}
	if high == 14 {
		return HandGroup::Marginal;
	}
	if connected && high >= 10 {
		return HandGroup::Marginal;
	}
	HandGroup::Trash
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_premium() {
		assert_eq!(HoleCards::new(14, 14, false).classify(), HandGroup::Premium);
		assert_eq!(HoleCards::new(13, 13, false).classify(), HandGroup::Premium);
		assert_eq!(HoleCards::new(14, 13, true).classify(), HandGroup::Premium);
	}

	#[test]
	fn test_strong() {
		assert_eq!(HoleCards::new(12, 12, false).classify(), HandGroup::Strong);
		assert_eq!(HoleCards::new(14, 13, false).classify(), HandGroup::Strong);
		assert_eq!(HoleCards::new(14, 12, true).classify(), HandGroup::Strong);
	}

	#[test]
	fn test_trash() {
		assert_eq!(HoleCards::new(7, 2, false).classify(), HandGroup::Trash);
		assert_eq!(HoleCards::new(8, 3, false).classify(), HandGroup::Trash);
	}

	#[test]
	fn test_speculative() {
		assert_eq!(HoleCards::new(8, 7, true).classify(), HandGroup::Speculative);
		assert_eq!(HoleCards::new(5, 5, false).classify(), HandGroup::Speculative);
	}
}
