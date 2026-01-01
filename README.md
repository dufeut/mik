<p align="center">
  <img src="docs/src/assets/logo.png" alt="mik" width="180" />
</p>

<h1 align="center">mik</h1>

<p align="center">
  <strong>Package manager and runtime for WASI HTTP components</strong>
</p>

<p align="center">
  <a href="https://dufeut.github.io/mik/">Docs</a> &bull;
  <a href="https://crates.io/crates/mik">Crates.io</a> &bull;
  <a href="LICENSE">MIT License</a>
</p>

---

## Install

```bash
# macOS / Linux
curl -LsSf https://raw.githubusercontent.com/dufeut/mik/main/install.sh | sh

# Windows
powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/dufeut/mik/main/install.ps1 | iex"

# With Cargo
cargo install mik

# Docker
docker pull ghcr.io/dufeut/mik
```

## Quick Start

```bash
# Create project
mik new my-service
cd my-service

# Build and run
mik build -rc
mik run
```

## Commands

| Command            | Description                                  |
| ------------------ | -------------------------------------------- |
| `mik new <name>`   | Create new project with hello world handler  |
| `mik add <pkg>`    | Add dependency (OCI, git, path, or URL)      |
| `mik remove <pkg>` | Remove dependency                            |
| `mik build`        | Build component (`-r` release, `-c` compose) |
| `mik run`          | Run local dev server on port 3000            |
| `mik sync`         | Sync dependencies from `mik.toml`            |
| `mik publish`      | Publish to GitHub Releases                   |
| `mik cache`        | Manage AOT compilation cache                 |

## Dependency Sources

```bash
mik add user/repo              # OCI (ghcr.io)
mik add user/repo:v1.0         # OCI with tag
mik add https://host/pkg.wasm  # HTTP URL
mik add pkg --git <url>        # Git repository
mik add pkg --path ../local    # Local path
```

## mik.toml

```toml
[project]
name = "my-service"
version = "0.1.0"

[server]
port = 3000

[composition]
http_handler = true  # Auto-downloads bridge from ghcr.io/dufeut/mik-sdk-bridge

[dependencies]
# OCI (ghcr.io is default registry)
"user/router" = "latest"
"user/auth" = "v1.0"

# Custom registry
db = { registry = "docker.io/myorg/db:v2.0" }

# Local path
utils = { path = "../my-utils" }
```

## Requirements

- [cargo-component](https://github.com/bytecodealliance/cargo-component) - `cargo install cargo-component`
- [wac](https://github.com/bytecodealliance/wac) (for composition) - `cargo install wac-cli`

## Documentation

Full documentation available at **[dufeut.github.io/mik](https://dufeut.github.io/mik/)**

## Related

- [mik-sdk](https://github.com/dufeut/mik-sdk) - SDK for writing handlers
- [mikcar](https://github.com/dufeut/mikcar) - Services for WASM handlers


## License

[MIT](LICENSE)
