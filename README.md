# poker-terminal
A fully offline Texas Hold'em poker game for the terminal. Play against configurable AI opponents with different play styles.

## Quick Start
```bash
cargo run --bin play
```

## Controls

| Key | Action |
|-----|--------|
| `f` | Fold |
| `c` | Check / Call |
| `r` | Raise (←/→ to adjust, Enter to confirm) |
| `a` | All-in |
| `↑`/`↓` | Game speed |
| `q` | Quit |

## AI Opponents
Opponents use strategy archetypes defined in `config/strategies.toml`:

| Style | Description |
|-------|-------------|
| `tag` | Tight-aggressive. Patient, selective, aggressive when playing. |
| `lag` | Loose-aggressive. Wide range, constant pressure. |
| `rock` | Very tight. Only plays premium hands. |
| `calling_station` | Passive. Calls too much, rarely folds. |
| `maniac` | Hyper-aggressive. Raises constantly. |

Edit `config/players.toml` to customize your opponent roster.

## Game Formats
**Cash Games** — Fixed blinds, play indefinitely.
**Sit-n-Go Tournaments** — Increasing blinds, prize payouts, play until one player remains.

Configure tables in `config/tables.toml`:

```toml
[tables.home_game]
name = "Home Game"
format = "cash"
betting = "no-limit"
small_blind = 1.0
big_blind = 2.0
min_buy_in = 40.0
max_buy_in = 200.0
```

## Architecture
```
Engine (rs_poker)
    │
    ├── emits ──> GameEvent ──> TUI / Logger
    │
    └── requests ──> PlayerPort
                        ├── TerminalPlayer (you)
                        ├── RulesPlayer (AI)
                        └── TestPlayer (for tests)
```

The engine emits events; renderers consume them. Players implement `PlayerPort` to respond to action requests. This separation allows the same game logic to support terminal, web, or Discord interfaces.

## Configuration
Config files load from `~/.config/poker-terminal/` first, falling back to `./config/`:

| File | Purpose |
|------|---------|
| `tables.toml` | Stakes, formats, blind schedules |
| `players.toml` | AI opponent roster |
| `strategies.toml` | Play style definitions |
| `profiles.toml` | Bankrolls (auto-created) |



## Logs
Logs write to `logs/poker-YYYY-MM-DD.log`:
```
[14:23:45.100][a1b2c3d4][H3][Engine:HAND] started button=2 players=3
[14:23:45.123][a1b2c3d4][H3][AI:STRATEGY] Lisa: Premium in BTN
[14:23:45.124][a1b2c3d4][H3][AI:DECISION] Lisa: RULE → raise $30
```

## License
Apache-2.0
