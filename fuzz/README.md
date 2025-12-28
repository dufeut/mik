# Fuzzing Security-Critical Functions

This directory contains fuzz targets for security-critical functions in mik.
Fuzzing helps find edge cases and potential vulnerabilities that unit tests might miss.

## Prerequisites

1. **Nightly Rust**: cargo-fuzz requires nightly
   ```bash
   rustup install nightly
   ```

2. **cargo-fuzz**: Install the fuzzing tool
   ```bash
   cargo install cargo-fuzz
   ```

3. **Linux recommended**: libfuzzer works best on Linux. On Windows/macOS, consider using WSL or a Linux VM.

## Available Fuzz Targets

| Target | Function | Purpose |
|--------|----------|---------|
| `fuzz_sanitize_file_path` | `sanitize_file_path()` | Path traversal prevention |
| `fuzz_sanitize_module_name` | `sanitize_module_name()` | Module name validation |
| `fuzz_validate_windows_path` | `validate_windows_path()` | Windows-specific path attacks |
| `fuzz_json_parsing` | `serde_json::from_slice()` | JSON parsing robustness |
| `fuzz_combined_security` | All security functions | Combined adversarial testing |

## Running Fuzz Targets

### Quick Start

```bash
# Run a specific target for 60 seconds
cd mik
cargo +nightly fuzz run fuzz_sanitize_file_path -- -max_total_time=60

# Run until a crash is found
cargo +nightly fuzz run fuzz_sanitize_file_path

# Run with multiple jobs (parallel fuzzing)
cargo +nightly fuzz run fuzz_sanitize_file_path -- -jobs=4 -workers=4
```

### Recommended Fuzzing Session

```bash
# Start with path traversal (most security-critical)
cargo +nightly fuzz run fuzz_sanitize_file_path -- -max_total_time=300

# Then Windows-specific attacks
cargo +nightly fuzz run fuzz_validate_windows_path -- -max_total_time=300

# Module name validation
cargo +nightly fuzz run fuzz_sanitize_module_name -- -max_total_time=300

# JSON parsing (for completeness)
cargo +nightly fuzz run fuzz_json_parsing -- -max_total_time=300

# Combined adversarial testing
cargo +nightly fuzz run fuzz_combined_security -- -max_total_time=600
```

## Understanding Results

### If a crash is found

1. The crashing input is saved to `fuzz/artifacts/<target>/`
2. Reproduce the crash:
   ```bash
   cargo +nightly fuzz run fuzz_sanitize_file_path fuzz/artifacts/fuzz_sanitize_file_path/crash-xxxxx
   ```
3. Minimize the input:
   ```bash
   cargo +nightly fuzz tmin fuzz_sanitize_file_path fuzz/artifacts/fuzz_sanitize_file_path/crash-xxxxx
   ```

### Coverage

To see code coverage:

```bash
cargo +nightly fuzz coverage fuzz_sanitize_file_path
# View the report
cargo cov -- show target/x86_64-unknown-linux-gnu/coverage/fuzz_sanitize_file_path --format=html > coverage.html
```

## Corpus Management

The fuzzer maintains a corpus of interesting inputs in `fuzz/corpus/<target>/`.

### Seed the corpus with known edge cases

```bash
mkdir -p fuzz/corpus/fuzz_sanitize_file_path
echo -n "../etc/passwd" > fuzz/corpus/fuzz_sanitize_file_path/traversal1
echo -n "..\\..\\etc\\passwd" > fuzz/corpus/fuzz_sanitize_file_path/traversal2
echo -n "CON.txt" > fuzz/corpus/fuzz_sanitize_file_path/windows_reserved
echo -n "file.txt:stream" > fuzz/corpus/fuzz_sanitize_file_path/ads
```

### Minimize corpus (remove redundant inputs)

```bash
cargo +nightly fuzz cmin fuzz_sanitize_file_path
```

## Security Invariants Tested

### sanitize_file_path

- No path can escape the base directory via `..`
- Absolute paths are rejected
- Null bytes are rejected
- Windows reserved names (CON, PRN, etc.) are rejected
- UNC paths are rejected
- Alternate data streams are rejected

### sanitize_module_name

- No path separators allowed (/ or \)
- Special directories (. or ..) are rejected
- Control characters are rejected
- Length limited to 255 characters
- Null bytes are rejected

### validate_windows_path

- Reserved device names are detected regardless of case
- Reserved names with extensions are detected (CON.txt)
- UNC paths (\\server\share) are rejected
- Alternate data streams (file:stream) are rejected

## Wasmtime Reference

This fuzzing setup follows patterns from wasmtime's fuzzing infrastructure:
https://docs.wasmtime.dev/contributing-fuzzing.html

Key techniques used:
- Structured fuzzing with `Arbitrary` derive
- Invariant checking on both success and error cases
- Adversarial pattern generation
- Round-trip testing (for JSON)

## Continuous Fuzzing

For production, consider:

1. **OSS-Fuzz integration** for continuous fuzzing
2. **ClusterFuzz** for distributed fuzzing
3. **Regular fuzzing in CI** with time limits

Example CI step:
```yaml
- name: Fuzz security functions
  run: |
    cargo +nightly fuzz run fuzz_combined_security -- -max_total_time=120
```
