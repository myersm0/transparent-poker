use transparent_poker::net::GameServer;

fn main() {
	let server = GameServer::new();
	let addr = "127.0.0.1:9999";
	
	if let Err(e) = server.run(addr) {
		eprintln!("Server error: {}", e);
		std::process::exit(1);
	}
}
