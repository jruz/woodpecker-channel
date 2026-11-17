# Woodpecker CI Channel for Claude Code

MCP channel server that monitors Woodpecker CI pipelines and pushes failure notifications into the active session.

## Use with Claude Code

Set `WOODPECKER_SERVER` and `WOODPECKER_TOKEN` in the environment Claude Code runs in.

Register the server:

```sh
claude mcp add --scope user woodpecker-channel "$(which woodpecker-channel)" serve
```

Start Claude Code with the channel:

```sh
claude --dangerously-load-development-channels server:woodpecker-channel
```

Ask Claude to call `watch` with a `repo` and either a `branch` or an `event`. Pipeline results arrive as `<channel source="woodpecker-channel">` messages. Logs are written to `~/.local/state/woodpecker-channel/server.log`.

## Development

- `just run`
- `just check`
- `just test`
- `just install`

## Install from releases

Download the binary for your platform from [the latest release](https://github.com/jruz/woodpecker-channel/releases/latest):

| Platform            | Download                                                                                                                      |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64        | [Download](https://github.com/jruz/woodpecker-channel/releases/latest/download/woodpecker-channel-unknown-linux-musl-x86_64)  |
| Linux ARM64         | [Download](https://github.com/jruz/woodpecker-channel/releases/latest/download/woodpecker-channel-unknown-linux-musl-aarch64) |
| macOS Intel         | [Download](https://github.com/jruz/woodpecker-channel/releases/latest/download/woodpecker-channel-apple-darwin-x86_64)        |
| macOS Apple Silicon | [Download](https://github.com/jruz/woodpecker-channel/releases/latest/download/woodpecker-channel-apple-darwin-aarch64)       |

For example, on Linux x86_64:

```sh
curl --fail --location --output woodpecker-channel \
  https://github.com/jruz/woodpecker-channel/releases/latest/download/woodpecker-channel-unknown-linux-musl-x86_64
chmod +x woodpecker-channel
./woodpecker-channel --version
```

Move the binary into a directory on your `PATH` to run it from anywhere.

## Install with Nix

Requires Nix with flakes enabled.

```sh
nix profile add github:jruz/woodpecker-channel
```

Installs a published binary without compiling Rust. Supports Linux and macOS on x86_64 and aarch64.

Verify the installation:

```sh
woodpecker-channel --version
```

Upgrade:

```sh
nix profile upgrade woodpecker-channel
```

Run without installing:

```sh
nix run github:jruz/woodpecker-channel -- --help
```

Build from source instead:

```sh
nix build github:jruz/woodpecker-channel#source
```

## Licence

Licensed under the [EUPL 1.2](LICENSE).
