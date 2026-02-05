from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Iterable, Optional


def _read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def _read_json(path: Path) -> Any:
    return json.loads(_read_text(path))


def _read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for raw in _read_text(path).splitlines():
        raw = raw.strip()
        if not raw:
            continue
        rows.append(json.loads(raw))
    return rows


def _read_toml(path: Path) -> dict[str, Any]:
    raw = _read_text(path)
    try:
        import tomllib  # py311+
    except Exception:  # pragma: no cover
        import toml as tomllib  # type: ignore
    return tomllib.loads(raw)


@dataclass(frozen=True)
class RunArtifacts:
    run_dir: Path
    run_id: str
    summary: Optional[dict[str, Any]]
    config_snapshot: Optional[dict[str, Any]]
    trades_csv: Optional[Path]
    equity_csv: Optional[Path]
    logs_jsonl: Optional[Path]
    manifest: Optional[dict[str, Any]]

    def paper_dir(self) -> Path:
        return self.run_dir / "paper"

    def ensure_paper_dirs(self) -> tuple[Path, Path]:
        tables = self.paper_dir() / "tables"
        figures = self.paper_dir() / "figures"
        tables.mkdir(parents=True, exist_ok=True)
        figures.mkdir(parents=True, exist_ok=True)
        return tables, figures


def list_runs(
    runs_dir: Path | str = Path("runs"),
    predicate: Optional[Callable[[Path], bool]] = None,
) -> list[Path]:
    runs_dir = Path(runs_dir)
    if not runs_dir.exists():
        return []
    out: list[Path] = []
    for child in sorted(runs_dir.iterdir()):
        if not child.is_dir():
            continue
        if predicate is not None and not predicate(child):
            continue
        out.append(child)
    return out


def load_run(run_dir: Path | str) -> RunArtifacts:
    run_dir = Path(run_dir)
    run_id = run_dir.name

    summary_path = run_dir / "summary.json"
    config_path = run_dir / "config_snapshot.toml"
    trades_path = run_dir / "trades.csv"
    equity_path = run_dir / "equity.csv"
    logs_path = run_dir / "logs.jsonl"
    manifest_path = run_dir / "manifest.json"

    summary = _read_json(summary_path) if summary_path.exists() else None
    config_snapshot = _read_toml(config_path) if config_path.exists() else None
    manifest = _read_json(manifest_path) if manifest_path.exists() else None

    return RunArtifacts(
        run_dir=run_dir,
        run_id=run_id,
        summary=summary,
        config_snapshot=config_snapshot,
        trades_csv=trades_path if trades_path.exists() else None,
        equity_csv=equity_path if equity_path.exists() else None,
        logs_jsonl=logs_path if logs_path.exists() else None,
        manifest=manifest,
    )


def latest_run(runs_dir: Path | str = Path("runs")) -> Optional[Path]:
    runs = list_runs(runs_dir)
    if not runs:
        return None
    return max(runs, key=lambda p: p.stat().st_mtime)


def iter_loaded_runs(
    runs: Iterable[Path],
    require_files: tuple[str, ...] = ("trades.csv", "equity.csv"),
) -> Iterable[RunArtifacts]:
    for run_dir in runs:
        ok = True
        for name in require_files:
            if not (run_dir / name).exists():
                ok = False
                break
        if not ok:
            continue
        yield load_run(run_dir)


def select_runs_by_manifest(
    runs: Iterable[RunArtifacts],
    *,
    protocol_id: Optional[str] = None,
    variant_id: Optional[str] = None,
) -> list[RunArtifacts]:
    selected: list[RunArtifacts] = []
    for r in runs:
        m = r.manifest or {}
        if protocol_id is not None and m.get("protocol_id") != protocol_id:
            continue
        if variant_id is not None and m.get("variant_id") != variant_id:
            continue
        selected.append(r)
    return selected

