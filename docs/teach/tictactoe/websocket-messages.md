# Noughts and Crosses: WebSocket Messages

Every message is JSON with a `type` field. Cells are numbered 0–8, row-major.

## Client → Server

| Type | Payload | Purpose |
|---|---|---|
| `join` | `{ "name": "Ada" }` | Register as a player |
| `create_game` | `{}` | Start a new game and wait for an opponent |
| `join_game` | `{ "gameId": "abc123" }` | Join an existing game |
| `move` | `{ "gameId": "abc123", "cell": 4 }` | Play a mark (cells 0–8, row-major) |
| `rematch` | `{ "gameId": "abc123" }` | Offer or accept a rematch |
| `resign` | `{ "gameId": "abc123" }` | Concede |
| `leave` | `{ "gameId": "abc123" }` | Leave the game |
| `chat` | `{ "gameId": "abc123", "text": "gg" }` | Optional table talk |
| `ping` | `{}` | Keepalive |

## Server → Client

| Type | Payload | Purpose |
|---|---|---|
| `welcome` | `{ "playerId": "p1" }` | Confirms the join |
| `game_created` | `{ "gameId": "abc123" }` | Reply to `create_game` |
| `game_start` | `{ "gameId", "you": "X", "opponent": "Grace", "turn": "X" }` | Both players are present |
| `state` | `{ "board": ["X","","O","","X","","","",""], "turn": "O" }` | Full board snapshot |
| `move_made` | `{ "cell": 4, "mark": "X", "next": "O" }` | Broadcast after a valid move |
| `game_over` | `{ "result": "win"\|"draw", "winner": "X", "line": [0,4,8] }` | End of game, including the winning line |
| `rematch_offered` | `{ "from": "Grace" }` | Opponent wants another game |
| `opponent_left` | `{ "reason": "disconnect"\|"resign" }` | Opponent has gone |
| `chat` | `{ "from": "Grace", "text": "gg" }` | Relayed chat |
| `error` | `{ "code": "NOT_YOUR_TURN", "message": "..." }` | Rejected action |
| `pong` | `{}` | Keepalive reply |

## Error codes

`NOT_YOUR_TURN`, `CELL_TAKEN`, `INVALID_CELL`, `GAME_NOT_FOUND`, `GAME_FULL`, `GAME_OVER`, `BAD_MESSAGE`

## Example exchange

```json
// client
{ "type": "move", "gameId": "abc123", "cell": 4 }

// server, to both players
{ "type": "move_made", "cell": 4, "mark": "X", "next": "O" }

// server, after the winning move
{ "type": "game_over", "result": "win", "winner": "X", "line": [0, 4, 8] }
```

## Design notes

- **The server is authoritative.** Clients only send intent (`move`). The server validates turn order, bounds and occupancy, then broadcasts the result.
- **Send `state` on reconnect.** A full snapshot lets a dropped client resync without replaying moves.
- **Add a `seq` number** to server messages if you want to detect missed updates.
- **Mark every message with `type`** so the client can use a simple `switch` dispatcher.
