#!/usr/bin/env python3
"""
Bayesian hyperparameter optimization for model training (Kairos research).

This script optimizes training hyperparameters with:
- Bayesian optimization (RBF surrogate + Expected Improvement acquisition)
- Parallel trial evaluation using threads (ThreadPoolExecutor)
- Deterministic runs via fixed seed

Config format (TOML)
--------------------
Required sections:

[study]
id = "drl_bayes_v1"
n_trials = 40
init_random = 8
parallelism = 4
seed = 42
maximize = true
metric_key = "val_sharpe"
output_path = "runs/optimize/drl_bayes_v1.json"
candidate_pool = 512
exploration = 0.01

[runner]
command = "python3 train.py --lr {learning_rate} --gamma {gamma} --batch-size {batch_size}"
timeout_sec = 3600
workdir = "."

[space.learning_rate]
type = "log_float"
low = 1e-5
high = 1e-2

[space.gamma]
type = "float"
low = 0.90
high = 0.999

[space.batch_size]
type = "int"
low = 64
high = 512

[space.net_arch]
type = "categorical"
choices = ["small", "medium", "large"]

The training command must print a JSON object in stdout that includes metric_key.
Example line: {"val_sharpe": 1.24}
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import math
import random
import re
import shlex
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Optional

try:
    import tomllib
except Exception as exc:  # pragma: no cover
    raise SystemExit("python>=3.11 is required (tomllib unavailable)") from exc

try:
    import numpy as np
except Exception as exc:  # pragma: no cover
    raise SystemExit("optimize_bayes requires numpy (pip install numpy)") from exc


def _to_float(value: Any, *, key: str) -> float:
    try:
        return float(value)
    except Exception as exc:
        raise ValueError(f"invalid float for {key}: {value!r}") from exc


def _to_int(value: Any, *, key: str) -> int:
    try:
        return int(value)
    except Exception as exc:
        raise ValueError(f"invalid int for {key}: {value!r}") from exc


def _to_bool(value: Any, *, key: str) -> bool:
    if isinstance(value, bool):
        return value
    raise ValueError(f"invalid bool for {key}: {value!r}")


@dataclass(frozen=True)
class ParamSpec:
    name: str
    kind: str
    low: Optional[float] = None
    high: Optional[float] = None
    choices: Optional[list[Any]] = None

    def sample(self, rng: random.Random) -> Any:
        if self.kind == "float":
            assert self.low is not None and self.high is not None
            return rng.uniform(self.low, self.high)
        if self.kind == "log_float":
            assert self.low is not None and self.high is not None
            lo = math.log(self.low)
            hi = math.log(self.high)
            return math.exp(rng.uniform(lo, hi))
        if self.kind == "int":
            assert self.low is not None and self.high is not None
            return rng.randint(int(self.low), int(self.high))
        if self.kind == "categorical":
            assert self.choices is not None and len(self.choices) > 0
            return rng.choice(self.choices)
        raise ValueError(f"unsupported param kind: {self.kind}")

    def encode(self, value: Any) -> float:
        if self.kind in ("float", "log_float"):
            assert self.low is not None and self.high is not None
            x = float(value)
            if self.kind == "log_float":
                x = math.log(max(1e-18, x))
                lo = math.log(self.low)
                hi = math.log(self.high)
            else:
                lo = self.low
                hi = self.high
            span = max(1e-12, hi - lo)
            return (x - lo) / span

        if self.kind == "int":
            assert self.low is not None and self.high is not None
            lo = float(int(self.low))
            hi = float(int(self.high))
            span = max(1.0, hi - lo)
            return (float(int(value)) - lo) / span

        if self.kind == "categorical":
            assert self.choices is not None and len(self.choices) > 0
            try:
                idx = self.choices.index(value)
            except ValueError:
                idx = 0
            if len(self.choices) == 1:
                return 0.0
            return float(idx) / float(len(self.choices) - 1)

        raise ValueError(f"unsupported param kind: {self.kind}")


@dataclass(frozen=True)
class StudyConfig:
    study_id: str
    n_trials: int
    init_random: int
    parallelism: int
    seed: int
    maximize: bool
    metric_key: str
    output_path: Path
    candidate_pool: int
    exploration: float


@dataclass(frozen=True)
class RunnerConfig:
    command: str
    timeout_sec: int
    workdir: Path


@dataclass
class TrialResult:
    trial_id: int
    params: dict[str, Any]
    status: str
    score: Optional[float]
    started_at_ms: int
    duration_ms: int
    command: str
    error: Optional[str] = None


def _hash_params(params: dict[str, Any]) -> str:
    return json.dumps(params, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def _build_specs(raw_space: dict[str, Any]) -> list[ParamSpec]:
    if not isinstance(raw_space, dict) or len(raw_space) == 0:
        raise ValueError("[space] must define at least one parameter")
    specs: list[ParamSpec] = []
    for name in sorted(raw_space.keys()):
        raw = raw_space[name]
        if not isinstance(raw, dict):
            raise ValueError(f"[space.{name}] must be a table")
        kind = str(raw.get("type", "")).strip()
        if kind in ("float", "log_float", "int"):
            low = _to_float(raw.get("low"), key=f"space.{name}.low")
            high = _to_float(raw.get("high"), key=f"space.{name}.high")
            if not (high > low):
                raise ValueError(f"space.{name}: high must be > low")
            if kind == "log_float" and low <= 0.0:
                raise ValueError(f"space.{name}: log_float requires low > 0")
            specs.append(ParamSpec(name=name, kind=kind, low=low, high=high))
            continue
        if kind == "categorical":
            choices = raw.get("choices")
            if not isinstance(choices, list) or len(choices) == 0:
                raise ValueError(f"space.{name}: categorical requires non-empty choices")
            specs.append(ParamSpec(name=name, kind=kind, choices=list(choices)))
            continue
        raise ValueError(
            f"space.{name}: unsupported type {kind!r} "
            f"(use float|log_float|int|categorical)"
        )
    return specs


def _load_config(path: Path) -> tuple[StudyConfig, RunnerConfig, list[ParamSpec]]:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("invalid TOML root")

    raw_study = data.get("study", {})
    raw_runner = data.get("runner", {})
    raw_space = data.get("space", {})
    if not isinstance(raw_study, dict):
        raise ValueError("[study] must be a table")
    if not isinstance(raw_runner, dict):
        raise ValueError("[runner] must be a table")

    study = StudyConfig(
        study_id=str(raw_study.get("id", "study")).strip() or "study",
        n_trials=max(1, _to_int(raw_study.get("n_trials", 40), key="study.n_trials")),
        init_random=max(1, _to_int(raw_study.get("init_random", 8), key="study.init_random")),
        parallelism=max(1, _to_int(raw_study.get("parallelism", 4), key="study.parallelism")),
        seed=_to_int(raw_study.get("seed", 42), key="study.seed"),
        maximize=_to_bool(raw_study.get("maximize", True), key="study.maximize"),
        metric_key=str(raw_study.get("metric_key", "score")),
        output_path=Path(str(raw_study.get("output_path", "runs/optimize/study.json"))),
        candidate_pool=max(
            32,
            _to_int(raw_study.get("candidate_pool", 512), key="study.candidate_pool"),
        ),
        exploration=max(
            0.0,
            _to_float(raw_study.get("exploration", 0.01), key="study.exploration"),
        ),
    )
    runner = RunnerConfig(
        command=str(raw_runner.get("command", "")).strip(),
        timeout_sec=max(1, _to_int(raw_runner.get("timeout_sec", 3600), key="runner.timeout_sec")),
        workdir=Path(str(raw_runner.get("workdir", "."))),
    )
    if not runner.command:
        raise ValueError("runner.command is required")

    specs = _build_specs(raw_space)
    return study, runner, specs


def _kernel_rbf(x1: np.ndarray, x2: np.ndarray, length_scale: float) -> np.ndarray:
    diff = x1[:, None, :] - x2[None, :, :]
    d2 = np.sum(diff * diff, axis=2)
    return np.exp(-0.5 * d2 / (length_scale * length_scale))


@dataclass
class RbfSurrogate:
    length_scale: float = 0.25
    noise: float = 1e-6
    _x: Optional[np.ndarray] = None
    _chol: Optional[np.ndarray] = None
    _alpha: Optional[np.ndarray] = None

    def fit(self, x: np.ndarray, y: np.ndarray) -> None:
        self._x = np.asarray(x, dtype=np.float64)
        yv = np.asarray(y, dtype=np.float64)
        k = _kernel_rbf(self._x, self._x, self.length_scale)
        k = k + np.eye(k.shape[0], dtype=np.float64) * self.noise
        try:
            chol = np.linalg.cholesky(k)
            z = np.linalg.solve(chol, yv)
            alpha = np.linalg.solve(chol.T, z)
            self._chol = chol
            self._alpha = alpha
        except np.linalg.LinAlgError:
            self._chol = None
            self._alpha = np.linalg.solve(k, yv)

    def predict(self, x: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        if self._x is None or self._alpha is None:
            mu = np.zeros((x.shape[0],), dtype=np.float64)
            sigma = np.ones((x.shape[0],), dtype=np.float64)
            return mu, sigma

        xc = np.asarray(x, dtype=np.float64)
        k_star = _kernel_rbf(self._x, xc, self.length_scale)  # [n_train, n_cand]
        mu = np.matmul(k_star.T, self._alpha)

        if self._chol is not None:
            v = np.linalg.solve(self._chol, k_star)
            var = 1.0 - np.sum(v * v, axis=0)
        else:
            var = np.ones((xc.shape[0],), dtype=np.float64) * 0.25
        var = np.maximum(var, 1e-12)
        sigma = np.sqrt(var)
        return mu, sigma


def _normal_pdf(z: np.ndarray) -> np.ndarray:
    return np.exp(-0.5 * z * z) / math.sqrt(2.0 * math.pi)


def _normal_cdf(z: np.ndarray) -> np.ndarray:
    return 0.5 * (1.0 + np.vectorize(math.erf)(z / math.sqrt(2.0)))


def _expected_improvement(
    mu: np.ndarray,
    sigma: np.ndarray,
    best: float,
    *,
    exploration: float,
) -> np.ndarray:
    sigma = np.maximum(sigma, 1e-12)
    imp = mu - best - exploration
    z = imp / sigma
    ei = imp * _normal_cdf(z) + sigma * _normal_pdf(z)
    ei[sigma <= 1e-12] = np.maximum(0.0, imp[sigma <= 1e-12])
    return ei


def _sample_params(specs: list[ParamSpec], rng: random.Random) -> dict[str, Any]:
    return {spec.name: spec.sample(rng) for spec in specs}


def _encode_params(specs: list[ParamSpec], params: dict[str, Any]) -> list[float]:
    return [spec.encode(params.get(spec.name)) for spec in specs]


def _format_cli_value(value: Any) -> str:
    if isinstance(value, float):
        return format(value, ".12g")
    return str(value)


_PARAM_PLACEHOLDER_RE = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")


def _render_command(command_template: str, params: dict[str, Any]) -> list[str]:
    def repl(match: re.Match[str]) -> str:
        key = match.group(1)
        if key not in params:
            return match.group(0)
        return _format_cli_value(params[key])

    rendered = _PARAM_PLACEHOLDER_RE.sub(repl, command_template)
    return shlex.split(rendered)


def _extract_metric(stdout: str, *, metric_key: str) -> float:
    lines = [ln.strip() for ln in stdout.splitlines() if ln.strip()]
    for line in reversed(lines):
        if not (line.startswith("{") and line.endswith("}")):
            continue
        try:
            obj = json.loads(line)
        except Exception:
            continue
        if not isinstance(obj, dict):
            continue
        if metric_key in obj:
            return _to_float(obj[metric_key], key=f"metric {metric_key}")

    txt = stdout.strip()
    if txt.startswith("{") and txt.endswith("}"):
        obj = json.loads(txt)
        if isinstance(obj, dict) and metric_key in obj:
            return _to_float(obj[metric_key], key=f"metric {metric_key}")
    raise ValueError(f"metric_key={metric_key!r} not found in stdout JSON")


def _build_command_objective(
    *,
    runner: RunnerConfig,
    metric_key: str,
) -> Callable[[dict[str, Any]], tuple[float, str]]:
    workdir = runner.workdir

    def objective(params: dict[str, Any]) -> tuple[float, str]:
        argv = _render_command(runner.command, params)
        if not argv:
            raise ValueError("empty command")
        started = time.time()
        cp = subprocess.run(
            argv,
            cwd=str(workdir),
            capture_output=True,
            text=True,
            timeout=runner.timeout_sec,
            check=False,
        )
        elapsed = time.time() - started
        cmd_str = " ".join(argv)
        if cp.returncode != 0:
            tail = (cp.stderr or cp.stdout or "").strip().splitlines()[-1:] or [""]
            msg = f"exit={cp.returncode} ({elapsed:.2f}s): {tail[0]}"
            raise RuntimeError(msg)
        score = _extract_metric(cp.stdout or "", metric_key=metric_key)
        return score, cmd_str

    return objective


def optimize(
    *,
    specs: list[ParamSpec],
    objective: Callable[[dict[str, Any]], tuple[float, str]],
    n_trials: int,
    init_random: int,
    parallelism: int,
    maximize: bool,
    seed: int,
    candidate_pool: int,
    exploration: float,
) -> dict[str, Any]:
    rng = random.Random(seed)
    surrogate = RbfSurrogate()

    successful: list[TrialResult] = []
    trials: list[TrialResult] = []
    seen_hashes: set[str] = set()
    pending_hashes: set[str] = set()

    launched = 0
    completed = 0
    running: dict[concurrent.futures.Future[TrialResult], tuple[int, str]] = {}

    def suggest_params() -> dict[str, Any]:
        if len(successful) < init_random or len(successful) < 2:
            return _sample_params(specs, rng)

        x_hist = np.asarray(
            [_encode_params(specs, t.params) for t in successful],
            dtype=np.float64,
        )
        y_hist = np.asarray(
            [float(t.score) if t.score is not None else float("nan") for t in successful],
            dtype=np.float64,
        )
        if not maximize:
            y_hist = -y_hist
        if not np.all(np.isfinite(y_hist)):
            return _sample_params(specs, rng)

        y_mu = float(np.mean(y_hist))
        y_sigma = float(np.std(y_hist))
        if y_sigma < 1e-12:
            y_sigma = 1.0
        y_norm = (y_hist - y_mu) / y_sigma

        surrogate.fit(x_hist, y_norm)

        candidate_params = [_sample_params(specs, rng) for _ in range(candidate_pool)]
        candidate_x = np.asarray(
            [_encode_params(specs, p) for p in candidate_params],
            dtype=np.float64,
        )
        mu, sigma = surrogate.predict(candidate_x)
        best = float(np.max(y_norm))
        ei = _expected_improvement(mu, sigma, best, exploration=exploration)
        best_idx = int(np.argmax(ei))
        return candidate_params[best_idx]

    def run_trial(trial_id: int, params: dict[str, Any]) -> TrialResult:
        started_at_ms = int(time.time() * 1000.0)
        t0 = time.perf_counter()
        try:
            score, command = objective(params)
            duration_ms = int((time.perf_counter() - t0) * 1000.0)
            return TrialResult(
                trial_id=trial_id,
                params=params,
                status="ok",
                score=float(score),
                started_at_ms=started_at_ms,
                duration_ms=duration_ms,
                command=command,
            )
        except Exception as exc:
            duration_ms = int((time.perf_counter() - t0) * 1000.0)
            return TrialResult(
                trial_id=trial_id,
                params=params,
                status="error",
                score=None,
                started_at_ms=started_at_ms,
                duration_ms=duration_ms,
                command="",
                error=str(exc),
            )

    with concurrent.futures.ThreadPoolExecutor(max_workers=parallelism) as pool:
        while completed < n_trials:
            while launched < n_trials and len(running) < parallelism:
                candidate = suggest_params()
                h = _hash_params(candidate)
                retries = 0
                while (h in seen_hashes or h in pending_hashes) and retries < 32:
                    candidate = _sample_params(specs, rng)
                    h = _hash_params(candidate)
                    retries += 1
                trial_id = launched
                launched += 1
                pending_hashes.add(h)
                fut = pool.submit(run_trial, trial_id, candidate)
                running[fut] = (trial_id, h)

            done, _ = concurrent.futures.wait(
                list(running.keys()),
                return_when=concurrent.futures.FIRST_COMPLETED,
            )
            for fut in done:
                trial_id, h = running.pop(fut)
                pending_hashes.discard(h)
                result = fut.result()
                trials.append(result)
                seen_hashes.add(_hash_params(result.params))
                if result.status == "ok":
                    successful.append(result)
                completed += 1
                if completed >= n_trials:
                    break

    if len(successful) == 0:
        raise RuntimeError("all trials failed")

    best_trial = max(successful, key=lambda t: float(t.score)) if maximize else min(
        successful, key=lambda t: float(t.score)
    )

    return {
        "best_params": best_trial.params,
        "best_score": best_trial.score,
        "best_trial_id": best_trial.trial_id,
        "successful_trials": len(successful),
        "failed_trials": len(trials) - len(successful),
        "trials": [
            {
                "trial_id": t.trial_id,
                "params": t.params,
                "status": t.status,
                "score": t.score,
                "started_at_ms": t.started_at_ms,
                "duration_ms": t.duration_ms,
                "command": t.command,
                "error": t.error,
            }
            for t in sorted(trials, key=lambda x: x.trial_id)
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Run Bayesian hyperparameter optimization for model training with "
            "parallel trial evaluation."
        )
    )
    parser.add_argument("--config", required=True, help="TOML path for study/runner/space.")
    args = parser.parse_args()

    cfg_path = Path(args.config)
    if not cfg_path.exists():
        raise SystemExit(f"config not found: {cfg_path}")

    study, runner, specs = _load_config(cfg_path)
    objective = _build_command_objective(runner=runner, metric_key=study.metric_key)

    t0 = time.perf_counter()
    result = optimize(
        specs=specs,
        objective=objective,
        n_trials=study.n_trials,
        init_random=study.init_random,
        parallelism=study.parallelism,
        maximize=study.maximize,
        seed=study.seed,
        candidate_pool=study.candidate_pool,
        exploration=study.exploration,
    )
    duration_ms = int((time.perf_counter() - t0) * 1000.0)

    artifact = {
        "study": {
            "id": study.study_id,
            "n_trials": study.n_trials,
            "init_random": study.init_random,
            "parallelism": study.parallelism,
            "seed": study.seed,
            "maximize": study.maximize,
            "metric_key": study.metric_key,
            "candidate_pool": study.candidate_pool,
            "exploration": study.exploration,
            "config_path": str(cfg_path),
        },
        "runner": {
            "command": runner.command,
            "timeout_sec": runner.timeout_sec,
            "workdir": str(runner.workdir),
        },
        "best": {
            "trial_id": result["best_trial_id"],
            "params": result["best_params"],
            "score": result["best_score"],
        },
        "stats": {
            "successful_trials": result["successful_trials"],
            "failed_trials": result["failed_trials"],
            "duration_ms": duration_ms,
        },
        "trials": result["trials"],
    }

    study.output_path.parent.mkdir(parents=True, exist_ok=True)
    study.output_path.write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    print(
        json.dumps(
            {
                "status": "ok",
                "study_id": study.study_id,
                "output_path": str(study.output_path),
                "best_trial_id": result["best_trial_id"],
                "best_score": result["best_score"],
                "best_params": result["best_params"],
                "successful_trials": result["successful_trials"],
                "failed_trials": result["failed_trials"],
                "duration_ms": duration_ms,
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
