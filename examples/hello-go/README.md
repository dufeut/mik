# hello-go

WASI HTTP component in Go using TinyGo.

## Prerequisites

```bash
# Download TinyGo from https://tinygo.org/getting-started/install/
go mod download
```

## Build

```bash
# Get WIT from SDK
WIT_DIR=$(go list -m -f '{{.Dir}}' github.com/rajatjindal/wasi-go-sdk)/wit

# Build
tinygo build -target=wasip2 --wit-package "$WIT_DIR" --wit-world sdk -o hello-go.wasm .

# Output to dist/
mkdir -p dist
mv hello-go.wasm dist/
```

## Run

```bash
mik run dist/hello-go.wasm
curl http://localhost:3000/run/hello-go/
```
