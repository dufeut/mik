#!/usr/bin/env python3
"""Find Rust files with {X} lines of code."""

from pathlib import Path


def main(max_size: int = 500):
    root = Path(__file__).parent.parent
    rust_files = sorted(root.rglob("*.rs"))

    large_files = []
    for path in rust_files:
        # Skip target directory and bindings.rs files
        if (
            "target" in path.parts
            or "test" in path.parts
            or "test.rs" in path.name
            or path.name == "bindings.rs"
        ):
            continue
        try:
            loc = len(path.read_text(encoding="utf-8").splitlines())
            if loc >= max_size:
                large_files.append((path.relative_to(root), loc))
        except Exception:
            pass

    large_files.sort(key=lambda x: -x[1])

    if not large_files:
        print(f"No Rust files with {max_size}+ LOC found.")
        return

    print(f"Rust files with {max_size}+ LOC ({len(large_files)} files):\n")
    for path, loc in large_files:
        print(f"  {loc:>5} lines  {path}")


if __name__ == "__main__":
    main()
