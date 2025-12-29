# hello-c

WASI HTTP component in C using wasi-sdk.

## Prerequisites

```bash
# Download wasi-sdk from https://github.com/WebAssembly/wasi-sdk/releases
# Set WASI_SDK to installation path

cargo install wit-bindgen-cli
```

## Build

```bash
# Get WASI HTTP 0.2.0 WIT
git clone --depth 1 --branch v0.2.0 https://github.com/WebAssembly/wasi-http.git

# Generate C bindings
wit-bindgen c wasi-http/wit --out-dir gen --world proxy

# Build (Linux/macOS)
$WASI_SDK/bin/clang --target=wasm32-wasip2 -O2 -c gen/proxy.c -o proxy.o
$WASI_SDK/bin/clang --target=wasm32-wasip2 -O2 -c hello.c -o hello.o
$WASI_SDK/bin/clang --target=wasm32-wasip2 -O2 proxy.o hello.o gen/proxy_component_type.o -o hello-c.wasm

# Output to dist/
mkdir -p dist
mv hello-c.wasm dist/
```

## Run

```bash
mik run dist/hello-c.wasm
curl http://localhost:3000/run/hello-c/
```
