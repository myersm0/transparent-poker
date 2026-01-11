# Development Guide

This document covers the architecture, key abstractions, and how to extend the codebase.

## Project Structure

```
src/
├── bin/
│   ├── play.rs          # Main CLI: poker play, poker register, etc.
│   ├── server.rs        # Network server: poker-server
│   ├── headless.rs      # Headless game runner (for testing/bots)
│   └── ai_game.rs       # AI-only games
├── engine/
│   ├── runner.rs        # GameRunner: main game loop
│   ├── adapter.rs       # PlayerAdapter: bridges PlayerPort to rs_poker Agent
│   └── historian.rs     # Event recording and rake calculation
├── events/
│   ├── types.rs         # GameEvent, PlayerAction, ValidActions, etc.
│   └── transformer.rs   # ViewUpdater: transforms events into view state
├── players/
│   ├── port.rs          # PlayerPort trait
│   ├── terminal.rs      # Human player via terminal
│   ├── rules_player.rs  # AI player using strategy rules
│   ├── remote_player.rs # Network player proxy
│   └── test_player.rs   # Scripted player for tests
├── lobby/
│   └── mod.rs           # LobbyBackend trait, LocalBackend, NetworkBackend
├── net/
│   ├── protocol.rs      # ClientMessage, ServerMessage, encoding
│   ├── client.rs        # GameClient: TCP connection to server
│   ├── server.rs        # GameServer: accepts connections, manages tables
│   └── remote_player.rs # Server-side remote player wrapper
├── strategy/
│   ├── archetype.rs     # Strategy definitions (TAG, LAG, etc.)
│   ├── hand_group.rs    # Hand classification (premium, strong, etc.)
│   └── position.rs      # Position-based adjustments
├── ai/
│   └── rules.rs         # Rule-based decision engine
├── bank.rs              # Bankroll management, buy-in/cashout
├── table.rs             # TableConfig, BlindClock, payouts
├── menu.rs              # TUI menu system
├── theme.rs             # Color theme loading
├── view.rs              # TableView, PlayerView (display state)
└── tui/
    ├── input.rs         # Input state machine
    └── widgets.rs       # Ratatui widget implementations
```

## Core Abstractions

### PlayerPort

The central abstraction for any entity that can play poker:

```rust
pub trait PlayerPort: Send + Sync {
    fn name(&self) -> &str;
    fn seat(&self) -> Seat;

    async fn request_action(
        &self,
        seat: Seat,
        valid_actions: ValidActions,
        snapshot: &GameSnapshot,
    ) -> PlayerResponse;

    async fn notify(&self, event: GameEvent);
}
```

Implementations:
- `TerminalPlayer` — Human at the keyboard
- `RulesPlayer` — AI using strategy archetypes
- `RemotePlayer` — Proxy for network players
- `TestPlayer` — Scripted actions for testing

### LobbyBackend

Abstracts local vs network game setup:

```rust
pub trait LobbyBackend {
    fn send(&mut self, cmd: LobbyCommand);
    fn poll(&mut self) -> Option<LobbyEvent>;
    fn table_config(&self, table_id: &str) -> Option<TableConfig>;
    fn get_bankroll(&self, player_id: &str) -> f32;
}
```

Implementations:
- `LocalBackend` — Direct table/player management
- `NetworkBackend` — Proxies commands to GameClient

### GameRunner

Orchestrates the game loop:

1. Creates `PlayerAdapter` for each player (bridges `PlayerPort` to rs_poker's `Agent` trait)
2. Runs hands via `HoldemSimulationBuilder`
3. Collects events from `EventHistorian`
4. Broadcasts events to all players and the event channel

```rust
let (runner, handle) = GameRunner::new(config, runtime_handle);
runner.add_player(player_arc);
runner.run(); // Blocks until game ends

// Consume events from handle.event_rx
```

### GameEvent

All game state changes are expressed as events:

```rust
pub enum GameEvent {
    GameCreated { game_id, config },
    PlayerJoined { seat, name, stack },
    HandStarted { hand_id, button, seats },
    HoleCardsDealt { seat, cards },
    BlindPosted { seat, blind_type, amount },
    ActionRequest { seat, valid_actions, time_limit },
    ActionTaken { seat, action, stack_after },
    StreetChanged { street, board },
    PotAwarded { pot_type, seat, amount, hand_description },
    HandEnded { results },
    PlayerEliminated { seat, name, finish_position },
    GameEnded { reason, final_standings },
    // ...
}
```

Events flow through `ViewUpdater` to maintain `TableView` state for rendering.

## Network Protocol

Client and server communicate via length-prefixed JSON messages over TCP.

### Message Format
```
[4 bytes: length (big-endian u32)][JSON payload]
```

### Client → Server
```rust
pub enum ClientMessage {
    Login { username },
    ListTables,
    JoinTable { table_id },
    LeaveTable,
    Ready,
    AddAI { strategy },
    RemoveAI { seat },
    Action { action: PlayerAction },
    Chat { text },
}
```

### Server → Client
```rust
pub enum ServerMessage {
    Welcome { username, message },
    Error { message },
    LobbyState { tables },
    TableJoined { table_id, table_name, seat, players, min_players, max_players },
    PlayerJoinedTable { seat, username },
    PlayerLeftTable { seat, username },
    PlayerReady { seat },
    AIAdded { seat, name },
    AIRemoved { seat },
    GameStarting { countdown },
    GameEvent(GameEvent),
    ActionRequest { valid_actions, time_limit },
}
```

### Connection Flow
1. Client connects, sends `Login`
2. Server responds with `Welcome`, then `LobbyState`
3. Client sends `JoinTable`, server responds with `TableJoined`
4. Players send `Ready`, server broadcasts `PlayerReady`
5. When conditions met, server sends `GameStarting`
6. During game, server sends `GameEvent` and `ActionRequest`
7. Client responds with `Action`

## Adding a New Player Type

1. Create a struct implementing `PlayerPort`:

```rust
pub struct MyPlayer {
    name: String,
    seat: Seat,
}

impl PlayerPort for MyPlayer {
    fn name(&self) -> &str { &self.name }
    fn seat(&self) -> Seat { self.seat }

    async fn request_action(
        &self,
        _seat: Seat,
        valid: ValidActions,
        snapshot: &GameSnapshot,
    ) -> PlayerResponse {
        // Your decision logic here
        PlayerResponse::Action(PlayerAction::Fold)
    }

    async fn notify(&self, _event: GameEvent) {
        // Handle event notifications
    }
}
```

2. Wrap in `Arc` and add to runner:
```rust
let player = Arc::new(MyPlayer::new(Seat(0), "Bot"));
runner.add_player(player);
```

## Adding a New AI Strategy

1. Add strategy definition in `config/strategies.toml`:
```toml
[strategies.my_style]
vpip = 0.25
pfr = 0.18
aggression = 1.5
# ...
```

2. The `StrategyStore` loads these automatically. Reference in `players.toml`:
```toml
[[players]]
id = "new_bot"
strategy = "my_style"
```

## Testing

### Unit Tests
```bash
cargo test --lib
```

Key test modules:
- `bank::tests` — Buy-in, cashout, bankroll operations
- `lobby::tests` — LocalBackend commands and events
- `net::protocol::tests` — Message serialization
- `events::types::tests` — Event structure validation

### Integration Tests
```bash
cargo test --test integration
```

Tests full game scenarios with `TestPlayer`:
- Heads-up games complete
- Raise caps enforced
- Multi-way pots
- All-in and side pots
- Different betting structures

### Manual Testing

Run AI-only games:
```bash
cargo run --bin ai-game
```

Run with debug logging:
```bash
RUST_LOG=debug cargo run --bin poker -- play -p test
```

## Bank System

The `Bank` handles persistent bankroll tracking:

```rust
// Buy into a game
bank.buyin("player_id", 100.0, "table_id")?;

// Cash out after game
bank.cashout("player_id", 150.0, "table_id");

// Award tournament prize
bank.award_prize("player_id", 500.0, 1); // 1st place

// Direct manipulation
bank.credit("player_id", 100.0);
bank.debit("player_id", 50.0)?;
```

Profiles are persisted to `profiles.toml` in the config directory.

### Early Termination Handling

- **Cash games**: Current stacks are credited back (using `TableView` state)
- **Tournaments**: No refund (buy-ins are forfeited)

## Event Flow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ HoldemSim    │────▶│ Historian    │────▶│ event_tx     │
│ (rs_poker)   │     │              │     │ (channel)    │
└──────────────┘     └──────────────┘     └──────────────┘
                                                 │
                     ┌───────────────────────────┼───────────────────────────┐
                     ▼                           ▼                           ▼
              ┌──────────────┐           ┌──────────────┐           ┌──────────────┐
              │ ViewUpdater  │           │ PlayerPort   │           │ Logger       │
              │ → TableView  │           │ .notify()    │           │              │
              └──────────────┘           └──────────────┘           └──────────────┘
```

## Configuration

### TableConfig Fields

```rust
pub struct TableConfig {
    pub id: String,
    pub name: String,
    pub format: GameFormat,           // Cash or SitNGo
    pub betting: BettingStructure,    // NoLimit, PotLimit, FixedLimit

    pub small_blind: Option<f32>,
    pub big_blind: Option<f32>,
    pub min_buy_in: Option<f32>,      // Cash games
    pub max_buy_in: Option<f32>,
    pub buy_in: Option<f32>,          // Tournaments
    pub starting_stack: Option<f32>,

    pub min_players: usize,
    pub max_players: usize,
    pub max_raises_per_round: u32,

    pub rake_percent: f32,
    pub rake_cap: Option<f32>,
    pub no_flop_no_drop: bool,

    pub blind_levels: Option<Vec<BlindLevel>>,  // Tournament blinds
    pub payouts: Option<Vec<f32>>,              // Tournament payout %

    pub action_timeout_seconds: Option<u32>,
    pub seed: Option<u64>,
}
```

### Strategy Fields

```rust
pub struct Strategy {
    pub vpip: f32,              // Voluntarily put $ in pot
    pub pfr: f32,               // Preflop raise %
    pub aggression: f32,        // Bet/raise vs call ratio
    pub three_bet: f32,         // 3-bet frequency
    pub fold_to_three_bet: f32,
    pub cbet: f32,              // Continuation bet %
    pub fold_to_cbet: f32,
    pub bluff_frequency: f32,
}
```

## Common Tasks

### Add a new game event
1. Add variant to `GameEvent` in `events/types.rs`
2. Handle in `ViewUpdater::apply()` in `events/transformer.rs`
3. Handle in player notification if needed

### Add a new CLI command
1. Add to clap derives in `src/bin/play.rs`
2. Handle in main match statement

### Add a new theme
1. Create `themes/mytheme.toml` in config directory
2. Theme auto-loads on next run

### Debug network issues
```bash
# Server with logging
RUST_LOG=debug cargo run --bin poker-server -- --port 9999

# Client with logging
RUST_LOG=debug cargo run --bin poker -- play -p test -s localhost:9999
```
