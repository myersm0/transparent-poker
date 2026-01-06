use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy)]
pub struct SeatPosition {
	pub x: u16,
	pub y: u16,
	pub width: u16,
	pub height: u16,
}

impl SeatPosition {
	pub fn rect(&self) -> Rect {
		Rect::new(self.x, self.y, self.width, self.height)
	}
}

pub struct TableLayout {
	pub seats: Vec<SeatPosition>,
	pub board_area: Rect,
	pub pot_area: Rect,
	pub chat_area: Rect,
}

impl TableLayout {
	pub fn compute(area: Rect, num_players: usize) -> Self {
		let seat_width: u16 = 18;
		let seat_height: u16 = 4;

		let chat_height: u16 = 12;

		let table_area = Rect::new(
			area.x,
			area.y,
			area.width,
			area.height.saturating_sub(chat_height),
		);

		let center_x = table_area.x + table_area.width / 2;
		let center_y = table_area.y + table_area.height / 2;

		let seats = layout_oval(table_area, num_players, seat_width, seat_height);

		let board_area = Rect::new(
			center_x.saturating_sub(15),
			center_y.saturating_sub(1),
			30,
			1,
		);

		let pot_area = Rect::new(
			center_x.saturating_sub(10),
			center_y,
			20,
			1,
		);

		let bottom_y = area.y + area.height.saturating_sub(chat_height);

		let chat_area = Rect::new(
			area.x + 1,
			bottom_y,
			area.width.saturating_sub(2),
			chat_height,
		);

		Self {
			seats,
			board_area,
			pot_area,
			chat_area,
		}
	}
}

fn layout_oval(area: Rect, n: usize, w: u16, h: u16) -> Vec<SeatPosition> {
	let n = n.min(10);
	let cx = area.x as f32 + area.width as f32 / 2.0;
	let cy = area.y as f32 + area.height as f32 / 2.0;

	let rx = (area.width as f32 / 2.0) - (w as f32 / 2.0) - 2.0;
	let ry = (area.height as f32 / 2.0) - (h as f32 / 2.0) - 1.0;

	let mut seats = Vec::with_capacity(n);

	for i in 0..n {
		let angle = std::f32::consts::PI * (0.5 + (i as f32 / n as f32) * 2.0);

		let x = cx + rx * angle.cos();
		let y = cy + ry * angle.sin();

		seats.push(SeatPosition {
			x: (x - w as f32 / 2.0).max(area.x as f32) as u16,
			y: (y - h as f32 / 2.0).max(area.y as f32) as u16,
			width: w,
			height: h,
		});
	}

	seats
}
