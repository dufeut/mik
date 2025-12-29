# hello-go

WASI HTTP component in Go using TinyGo.

## Known Issue

Go WASI HTTP components currently hang when served by mik. This is an upstream
issue with the Go WASI HTTP tooling (wasi-go-sdk), not mik - Python, JavaScript,
and C components all work correctly.

The Go SDK ecosystem for WASI HTTP is still maturing:
- [wasi-go-sdk](https://github.com/rajatjindal/wasi-go-sdk) - Early stage, "needs more testing"
- [wasmCloud SDK](https://wasmcloud.com/docs/developer/languages/go/components/) - Requires wasmCloud infrastructure
- [wit-bindgen-go](https://github.com/bytecodealliance/go-modules) - Raw bindings, complex to use

## Prerequisites

```bash
# Install TinyGo 0.40+ from https://tinygo.org/getting-started/install/
go mod download
```

## Build

```bash
# Get WIT from SDK
WIT_DIR=$(go list -m -f '{{.Dir}}' github.com/rajatjindal/wasi-go-sdk)/wit

# Build
mkdir -p dist
tinygo build -target=wasip2 --wit-package "$WIT_DIR" --wit-world sdk -o dist/hello-go.wasm .
```

## Run (with wasmtime serve)

The component works with `wasmtime serve`:

```bash
wasmtime serve -Scli dist/hello-go.wasm
curl http://localhost:8080/
```

## Size

Go components are ~1.3MB.
