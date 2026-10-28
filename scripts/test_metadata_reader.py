#!/usr/bin/env python3
"""Run metadata/XMP tests against the application's actual storage protections.

Run from the repository: python scripts/test_metadata_reader.py
Set LAP_TEST_EXIFTOOL=1 to also verify exports with an installed ExifTool.
The normal Tauri/native build prerequisites and cached Cargo dependencies are
required. These tests use fixtures, not the user's library.
"""
from pathlib import Path
import argparse
import subprocess

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--test-binary", type=Path, help="Reuse a test executable built from the current source")
options = parser.parse_args()
command = ([str(options.test_binary.resolve())] if options.test_binary else
           ["cargo", "test", "--locked", "--offline", "--manifest-path",
            str(root / "src-tauri/Cargo.toml"), "--"])
subprocess.run(
    command + ["t_metadata::", "t_xmp::", "t_xmp_files::", "--test-threads=1"],
    cwd=root,
    check=True,
)
