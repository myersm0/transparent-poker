use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph, Widget},
};

use crate::view::{Card, ChatMessage, PlayerStatus, PlayerView, Street, TableView};
use crate::tui::layout::TableLayout;

fn card_style(suit: char) -> Style {
	match suit {
		'h' | 'd' => Style::default().fg(Color::Red),
		_ => Style::default().fg(Color::White),
	}
}

fn render_card(card: &Card) -> Span<'static> {
	Span::styled(
		format!("{}{}", card.rank, card.suit_symbol()),
		card_style(card.suit),
	)
}

fn render_hole_cards(cards: &[Card; 2]) -> Line<'static> {
	Line::from(vec![
		render_card(&cards[0]),
		Span::raw(" "),
		render_card(&cards[1]),
	])
}

fn render_hidden_cards() -> Line<'static> {
	Line::styled("▓ ▓", Style::default().fg(Color::Blue))
}

pub struct PlayerWidget<'a> {
	player: &'a PlayerView,
	show_cards: bool,
	is_winner: bool,
}

impl<'a> PlayerWidget<'a> {
	pub fn new(player: &'a PlayerView, show_cards: bool) -> Self {
		Self { player, show_cards, is_winner: false }
	}

	pub fn winner(mut self, is_winner: bool) -> Self {
		self.is_winner = is_winner;
		self
	}
}

impl Widget for PlayerWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		let border_style = if self.is_winner {
			Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
		} else if self.player.is_actor {
			Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
		} else {
			match self.player.status {
				PlayerStatus::Folded => Style::default().fg(Color::DarkGray),
				PlayerStatus::AllIn => Style::default().fg(Color::Magenta),
				PlayerStatus::Eliminated => Style::default().fg(Color::Red),
				_ => Style::default().fg(Color::White),
			}
		};

		let name_display = if self.player.name.len() > 12 {
			format!("{}…", &self.player.name[..11])
		} else {
			self.player.name.clone()
		};

		let title_style = if self.is_winner {
			Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
		} else if self.player.is_actor {
			Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
		} else {
			Style::default()
		};

		let mut block = Block::default()
			.borders(Borders::ALL)
			.border_style(border_style)
			.title(Span::styled(name_display, title_style));

		if self.is_winner {
			block = block.title_top(
				Line::from(Span::styled(
					"💰",
					Style::default().fg(Color::Yellow),
				))
				.left_aligned()
			);
		} else if self.player.is_hero {
			block = block.title_top(
				Line::from(Span::styled(
					"★",
					Style::default().fg(Color::Cyan),
				))
				.left_aligned()
			);
		}

		if self.player.position == crate::view::Position::Button {
			block = block.title_top(
				Line::from(Span::styled(
					"◉",
					Style::default()
						.fg(Color::White)
						.add_modifier(Modifier::BOLD),
				))
				.right_aligned()
			);
		}

		let inner = block.inner(area);
		block.render(area, buf);

		if inner.height < 2 || inner.width < 6 {
			return;
		}

		let cards_line = if self.player.status == PlayerStatus::Folded {
			Line::styled("folded", Style::default().fg(Color::DarkGray))
		} else if self.player.status == PlayerStatus::Eliminated {
			Line::styled("out", Style::default().fg(Color::Red))
		} else if let Some(ref cards) = self.player.hole_cards {
			if self.show_cards || self.player.is_hero {
				render_hole_cards(cards)
			} else {
				render_hidden_cards()
			}
		} else {
			render_hidden_cards()
		};

		let stack_str = format!("${:.0}", self.player.stack);
		let bet_str = if self.player.current_bet > 0.0 {
			format!(" (${:.0})", self.player.current_bet)
		} else {
			String::new()
		};

		let mut stack_spans = vec![
			Span::styled(stack_str, Style::default().fg(Color::Green)),
			Span::styled(bet_str, Style::default().fg(Color::Yellow)),
		];

		if self.player.position == crate::view::Position::SmallBlind {
			stack_spans.push(Span::styled(" SB", Style::default().fg(Color::DarkGray)));
		} else if self.player.position == crate::view::Position::BigBlind {
			stack_spans.push(Span::styled(" BB", Style::default().fg(Color::DarkGray)));
		}

		let stack_line = Line::from(stack_spans);

		let paragraph = Paragraph::new(vec![cards_line, stack_line]);
		paragraph.render(inner, buf);
	}
}

pub struct BoardWidget<'a> {
	board: &'a [Card],
}

impl<'a> BoardWidget<'a> {
	pub fn new(board: &'a [Card], _street: Street) -> Self {
		Self { board }
	}
}

impl Widget for BoardWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		let mut spans: Vec<Span> = Vec::new();

		spans.push(Span::styled("[ ", Style::default().fg(Color::DarkGray)));

		for i in 0..5 {
			if i > 0 {
				spans.push(Span::raw("  "));
			}
			if let Some(card) = self.board.get(i) {
				spans.push(render_card(card));
			} else {
				spans.push(Span::styled("--", Style::default().fg(Color::DarkGray)));
			}
		}

		spans.push(Span::styled(" ]", Style::default().fg(Color::DarkGray)));

		let line = Line::from(spans);
		let paragraph = Paragraph::new(line);
		paragraph.render(area, buf);
	}
}

pub struct TableWidget<'a> {
	view: &'a TableView,
	show_all_cards: bool,
}

impl<'a> TableWidget<'a> {
	pub fn new(view: &'a TableView) -> Self {
		Self {
			view,
			show_all_cards: view.street == Street::Showdown,
		}
	}

	pub fn show_all_cards(mut self, show: bool) -> Self {
		self.show_all_cards = show;
		self
	}
}

impl Widget for TableWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		let title = if let Some(ref name) = self.view.table_name {
			format!(
				" {} | Hand #{} - {} ",
				name,
				self.view.hand_num,
				self.view.street.name()
			)
		} else {
			format!(
				" Hand #{} - {} ",
				self.view.hand_num,
				self.view.street.name()
			)
		};

		let outer_block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(Color::Green))
			.title(title)
			.title_bottom(format!(
				" Blinds ${:.0}/${:.0} ",
				self.view.blinds.0, self.view.blinds.1
			));

		let inner = outer_block.inner(area);
		outer_block.render(area, buf);

		let layout = TableLayout::compute(inner, self.view.players.len());

		for (i, player) in self.view.players.iter().enumerate() {
			if let Some(seat_pos) = layout.seats.get(i) {
				let is_winner = self.view.winner_seats.contains(&player.seat);
				let widget = PlayerWidget::new(player, self.show_all_cards).winner(is_winner);
				widget.render(seat_pos.rect(), buf);

				if let Some(action) = &player.last_action {
					let action_rect = Rect {
						x: seat_pos.rect().x,
						y: seat_pos.rect().y + seat_pos.rect().height,
						width: seat_pos.rect().width,
						height: 1,
					};
					if action_rect.y < inner.y + inner.height {
						let style = if player.action_fresh {
							Style::default().fg(Color::White)
						} else {
							Style::default().fg(Color::DarkGray)
						};
						let action_text = if action.len() > action_rect.width as usize {
							format!("{}…", &action[..action_rect.width as usize - 1])
						} else {
							action.clone()
						};
						buf.set_string(action_rect.x, action_rect.y, &action_text, style);
					}
				}
			}
		}

		let board_widget = BoardWidget::new(&self.view.board, self.view.street);
		board_widget.render(layout.board_area, buf);

		let pot_str = format!("Pot: ${:.0}", self.view.pot);
		let pot_line = Line::styled(pot_str, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
		Paragraph::new(pot_line).render(layout.pot_area, buf);

		let chat_widget = ChatWidget::new(&self.view.chat_messages);
		chat_widget.render(layout.chat_area, buf);
	}
}

pub struct ChatWidget<'a> {
	messages: &'a [ChatMessage],
}

impl<'a> ChatWidget<'a> {
	pub fn new(messages: &'a [ChatMessage]) -> Self {
		Self { messages }
	}
}

impl Widget for ChatWidget<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(Color::Blue))
			.title(" Game Log ");

		let inner = block.inner(area);
		block.render(area, buf);

		let max_lines = inner.height as usize;
		let start = self.messages.len().saturating_sub(max_lines);

		let lines: Vec<Line> = self.messages[start..]
			.iter()
			.map(|msg| {
				if msg.is_system {
					Line::styled(
						format!("» {}", msg.text),
						Style::default().fg(Color::Cyan),
					)
				} else {
					Line::from(vec![
						Span::styled(
							format!("{}: ", msg.sender),
							Style::default().fg(Color::Green),
						),
						Span::raw(&msg.text),
					])
				}
			})
			.collect();

		Paragraph::new(lines).render(inner, buf);
	}
}
