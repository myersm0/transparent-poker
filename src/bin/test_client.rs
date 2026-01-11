use std::io::{self, Write};
use std::time::Duration;
use transparent_poker::net::GameClient;

fn main() -> io::Result<()> {
	let addr = "127.0.0.1:9999";
	println!("Connecting to {}...", addr);
	
	let mut client = GameClient::connect(addr)?;
	println!("Connected!");

	print!("Enter username: ");
	io::stdout().flush()?;
	let mut username = String::new();
	io::stdin().read_line(&mut username)?;
	let username = username.trim();

	client.login(username)?;

	loop {
		while let Some(msg) = client.try_recv() {
			println!("< {:?}", msg);
		}

		print!("> ");
		io::stdout().flush()?;
		
		let mut input = String::new();
		io::stdin().read_line(&mut input)?;
		let input = input.trim();

		match input {
			"list" | "l" => {
				client.list_tables()?;
			}
			"quit" | "q" => {
				break;
			}
			"ready" | "r" => {
				client.ready()?;
			}
			"leave" => {
				client.leave_table()?;
			}
			cmd if cmd.starts_with("join ") => {
				let table_id = cmd.strip_prefix("join ").unwrap();
				client.join_table(table_id)?;
			}
			"help" | "h" | "?" => {
				println!("Commands:");
				println!("  list, l     - List available tables");
				println!("  join <id>   - Join a table");
				println!("  ready, r    - Mark yourself ready");
				println!("  leave       - Leave current table");
				println!("  quit, q     - Disconnect");
			}
			"" => {}
			_ => {
				println!("Unknown command. Type 'help' for commands.");
			}
		}

		std::thread::sleep(Duration::from_millis(50));
	}

	println!("Disconnected.");
	Ok(())
}
