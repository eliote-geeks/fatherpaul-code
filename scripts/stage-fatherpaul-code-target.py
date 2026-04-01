#!/usr/bin/env python3

from __future__ import annotations

import argparse
import shutil
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Stage FatherPaul Code native binaries into a vendor-src tree."
    )
    parser.add_argument("--target", required=True, help="Rust target triple to stage.")
    parser.add_argument(
        "--release-dir",
        required=True,
        type=Path,
        help="Cargo release directory for the target.",
    )
    parser.add_argument(
        "--output-root",
        required=True,
        type=Path,
        help="Root directory where vendor-src/<target>/... will be created.",
    )
    parser.add_argument(
        "--rg-path",
        required=True,
        type=Path,
        help="Path to the ripgrep binary to bundle.",
    )
    parser.add_argument(
        "--windows-helpers",
        action="store_true",
        help="Also stage codex-windows-sandbox-setup and codex-command-runner.",
    )
    return parser.parse_args()


def copy_binary(src: Path, dest: Path) -> None:
    if not src.exists():
        raise FileNotFoundError(f"Missing binary: {src}")
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dest)


def main() -> int:
    args = parse_args()

    target_dir = args.output_root.resolve() / args.target
    codex_dir = target_dir / "codex"
    path_dir = target_dir / "path"

    if target_dir.exists():
        shutil.rmtree(target_dir)

    codex_dir.mkdir(parents=True, exist_ok=True)
    path_dir.mkdir(parents=True, exist_ok=True)

    is_windows = "windows" in args.target
    binary_ext = ".exe" if is_windows else ""

    release_dir = args.release_dir.resolve()
    copy_binary(release_dir / f"codex{binary_ext}", codex_dir / f"codex{binary_ext}")
    copy_binary(args.rg_path.resolve(), path_dir / f"rg{binary_ext}")

    if args.windows_helpers:
        copy_binary(
            release_dir / "codex-windows-sandbox-setup.exe",
            codex_dir / "codex-windows-sandbox-setup.exe",
        )
        copy_binary(
            release_dir / "codex-command-runner.exe",
            codex_dir / "codex-command-runner.exe",
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
