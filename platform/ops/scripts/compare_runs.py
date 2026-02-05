#!/usr/bin/env python3
"""
Compare two Kairos Alloy run directories for determinism.

Checks:
  - equity.csv (exact match)
  - trades.csv (exact match)
  - summary.json (exact match after normalizing volatile fields)

Usage:
  scripts/compare_runs.py /path/to/runA /path/to/runB
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any, Dict, Tuple


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def first_diff_line(a: Path, b: Path) -> Tuple[int, str, str]:
    with a.open("r", encoding="utf-8", errors="replace") as fa, b.open(
        "r", encoding="utf-8", errors="replace"
    ) as fb:
        line_no = 0
        while True:
            line_no += 1
            la = fa.readline()
            lb = fb.readline()
            if la == "" and lb == "":
                return (0, "", "")
            if la != lb:
                return (line_no, la.rstrip("\n"), lb.rstrip("\n"))


def normalize_summary(value: Dict[str, Any]) -> Dict[str, Any]:
    # `run_id` is typically unique per run; it lives in `meta.run_id` and also inside
    # `config_snapshot.run.run_id`. For determinism comparison, we drop the snapshot entirely
    # and ignore `meta.run_id`.
    out = dict(value)
    if "config_snapshot" in out:
        out.pop("config_snapshot", None)
    meta = out.get("meta")
    if isinstance(meta, dict):
        meta = dict(meta)
        meta.pop("run_id", None)
        out["meta"] = meta
    return out


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def ensure_run_dir(path: Path) -> None:
    if not path.exists():
        raise SystemExit(f"run dir not found: {path}")
    if not path.is_dir():
        raise SystemExit(f"not a directory: {path}")


def compare_csv(name: str, a: Path, b: Path) -> bool:
    if not a.exists() or not b.exists():
        print(f"[ERR] missing {name}: {a} or {b}")
        return False
    ha = sha256_file(a)
    hb = sha256_file(b)
    if ha == hb:
        print(f"[OK]  {name}: identical ({ha[:12]})")
        return True
    line_no, la, lb = first_diff_line(a, b)
    print(f"[DIFF] {name}: sha256 differs ({ha[:12]} != {hb[:12]})")
    if line_no:
        print(f"       first diff at line {line_no}")
        print(f"       A: {la}")
        print(f"       B: {lb}")
    return False


def compare_summary(a: Path, b: Path) -> bool:
    if not a.exists() or not b.exists():
        print(f"[ERR] missing summary.json: {a} or {b}")
        return False
    va = load_json(a)
    vb = load_json(b)
    if not isinstance(va, dict) or not isinstance(vb, dict):
        print("[ERR] summary.json is not an object")
        return False
    na = normalize_summary(va)
    nb = normalize_summary(vb)
    ca = canonical_json(na)
    cb = canonical_json(nb)
    if ca == cb:
        print("[OK]  summary.json: identical (normalized)")
        return True
    print("[DIFF] summary.json: differs (normalized)")
    return False


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("run_a", type=Path)
    parser.add_argument("run_b", type=Path)
    args = parser.parse_args()

    run_a = args.run_a
    run_b = args.run_b
    ensure_run_dir(run_a)
    ensure_run_dir(run_b)

    ok = True
    ok &= compare_csv("equity.csv", run_a / "equity.csv", run_b / "equity.csv")
    ok &= compare_csv("trades.csv", run_a / "trades.csv", run_b / "trades.csv")
    ok &= compare_summary(run_a / "summary.json", run_b / "summary.json")

    return 0 if ok else 2


if __name__ == "__main__":
    raise SystemExit(main())

