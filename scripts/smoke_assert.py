"""Bounded observations for asynchronous graphical smoke-test fixtures."""

from pathlib import Path
import time


def assert_output_suffix(path, suffix, timeout=3):
    """Wait for captured PTY output, without replaying the input that produced it."""
    deadline = time.monotonic() + timeout
    while True:
        actual = Path(path).read_bytes()
        if actual.endswith(suffix):
            return
        if time.monotonic() >= deadline:
            raise AssertionError(f"Expected suffix {suffix!r}; got {actual!r}")
        time.sleep(0.05)
