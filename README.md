# blacepos.xyz/invite

## Usage

```rust
cargo run -- --log "DEBUG" --web-interface "127.0.0.1" --http-bind 8000
```

## Slot

This server implements the Slot protocol which allows it to be unified with other modules in blacepos.xyz. See [Slot](https://github.com/blacepos/slot) for more information about what this means and how it works.

To enable the Slot client, specify the Slot server's port on localhost with `--slot-addr`. It is recommended to use 0 for the HTTP port if using Slot so it doesn't have to be coordinated with other modules.

```rust
cargo run -- --log "DEBUG" --web-interface "127.0.0.1" --slot-addr 7568 --http-bind 0
```
