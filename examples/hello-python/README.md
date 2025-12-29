# hello-python

WASI HTTP component in Python using componentize-py.

> **Note:** Python components require runtime compatibility work. The component builds successfully but may not run on all WASI HTTP runtimes.

## Prerequisites

```bash
pip install componentize-py
```

## Build

```bash
# Get WASI HTTP 0.2.0 WIT
git clone --depth 1 --branch v0.2.0 https://github.com/WebAssembly/wasi-http.git
cp -r wasi-http/wit .

# Generate bindings
componentize-py -d wit -w proxy bindings .

# Build
mkdir -p dist
componentize-py -d wit -w proxy componentize app -o dist/hello-python.wasm
```

## Run

```bash
mik run dist/hello-python.wasm
curl http://localhost:3000/run/hello-python/
```

## Size

Python components are large (~40MB) due to the embedded Python interpreter.
