#!/usr/bin/env python3
"""
Train and evaluate a DRL policy (PPO/SB3) using Kairos Gym.

This script is designed to be called by optimize_bayes.py.
It prints a single JSON line with metric `val_sharpe` in stdout.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.machinery
import importlib.util
import json
import math
import sys
import time
from pathlib import Path
from typing import Any, Optional

import numpy as np


def _load_kairos_gym_module():
    path = Path(__file__).resolve().parents[1] / "kairos-gym" / "kairos_gym.py"
    spec = importlib.util.spec_from_loader(
        "kairos_gym_impl_for_train",
        importlib.machinery.SourceFileLoader("kairos_gym_impl_for_train", str(path)),
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load kairos_gym module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _policy_kwargs_for_net_arch(net_arch: str) -> dict[str, Any]:
    key = (net_arch or "medium").strip().lower()
    if key == "small":
        return {"net_arch": [64, 64]}
    if key == "large":
        return {"net_arch": [256, 256]}
    return {"net_arch": [128, 128]}


def _sharpe_like(returns: list[float]) -> float:
    if len(returns) < 2:
        return 0.0
    arr = np.asarray(returns, dtype=np.float64)
    mu = float(np.mean(arr))
    std = float(np.std(arr, ddof=1))
    if not np.isfinite(std) or std <= 1e-12:
        return 0.0
    return mu / std * math.sqrt(float(arr.shape[0]))


def _model_path_for_trial(
    *,
    model_out_dir: Path,
    learning_rate: float,
    gamma: float,
    batch_size: int,
    entropy_coef: float,
    net_arch: str,
    seed: int,
) -> Path:
    payload = {
        "learning_rate": learning_rate,
        "gamma": gamma,
        "batch_size": batch_size,
        "entropy_coef": entropy_coef,
        "net_arch": net_arch,
        "seed": seed,
    }
    raw = json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")
    suffix = hashlib.sha1(raw).hexdigest()[:12]
    return model_out_dir / f"ppo_{suffix}.zip"


def main() -> int:
    parser = argparse.ArgumentParser(description="Train PPO (SB3) with Kairos Gym and emit val_sharpe.")
    parser.add_argument("--base-config", required=True, help="Base Kairos TOML config.")
    parser.add_argument("--mode", default="backtest", choices=["backtest", "sweep"])
    parser.add_argument("--split-start", default=None, help="RFC3339 start for sweep episode window.")
    parser.add_argument("--split-end", default=None, help="RFC3339 end for sweep episode window.")
    parser.add_argument("--learning-rate", type=float, required=True)
    parser.add_argument("--gamma", type=float, required=True)
    parser.add_argument("--batch-size", type=int, required=True)
    parser.add_argument("--entropy-coef", type=float, required=True)
    parser.add_argument("--net-arch", default="medium", choices=["small", "medium", "large"])
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--total-timesteps", type=int, default=25_000)
    parser.add_argument("--eval-steps", type=int, default=8_000)
    parser.add_argument("--device", default="cpu", help="SB3 device: cpu|cuda|auto")
    parser.add_argument(
        "--model-out-dir",
        default="runs/models",
        help="Directory for trained model artifacts (.zip).",
    )
    args = parser.parse_args()

    started = time.perf_counter()

    try:
        import gymnasium as _  # noqa: F401
        from stable_baselines3 import PPO
    except Exception as exc:
        raise SystemExit(
            "train_drl_sb3 requires gymnasium and stable-baselines3. "
            "Install your RL stack in the active environment."
        ) from exc

    gym_mod = _load_kairos_gym_module()
    env = gym_mod.KairosGymEnv(
        base_config_path=args.base_config,
        mode=args.mode,
        split_start=args.split_start,
        split_end=args.split_end,
        seed=args.seed,
    )
    gym_env = env.as_gymnasium_env()

    model_path = _model_path_for_trial(
        model_out_dir=Path(args.model_out_dir),
        learning_rate=float(args.learning_rate),
        gamma=float(args.gamma),
        batch_size=int(args.batch_size),
        entropy_coef=float(args.entropy_coef),
        net_arch=args.net_arch,
        seed=int(args.seed),
    )
    model_path.parent.mkdir(parents=True, exist_ok=True)

    n_steps = max(128, int(args.batch_size))
    batch_size = min(int(args.batch_size), n_steps)
    policy_kwargs = _policy_kwargs_for_net_arch(args.net_arch)

    model = PPO(
        "MlpPolicy",
        gym_env,
        learning_rate=float(args.learning_rate),
        gamma=float(args.gamma),
        batch_size=batch_size,
        ent_coef=float(args.entropy_coef),
        n_steps=n_steps,
        policy_kwargs=policy_kwargs,
        seed=int(args.seed),
        device=args.device,
        verbose=0,
    )

    try:
        model.learn(total_timesteps=max(1, int(args.total_timesteps)))
        model.save(str(model_path))

        obs, info = gym_env.reset(seed=int(args.seed) + 1)
        rewards: list[float] = []
        returns: list[float] = []

        prev_eq: Optional[float] = None
        if isinstance(info, dict):
            val = info.get("equity")
            if isinstance(val, (int, float)):
                prev_eq = float(val)

        for _ in range(max(1, int(args.eval_steps))):
            action, _ = model.predict(obs, deterministic=True)
            obs, reward, terminated, truncated, info = gym_env.step(action)
            rewards.append(float(reward))

            eq: Optional[float] = None
            if isinstance(info, dict):
                val = info.get("equity")
                if isinstance(val, (int, float)):
                    eq = float(val)

            if prev_eq is not None and eq is not None and abs(prev_eq) > 1e-12:
                returns.append((eq / prev_eq) - 1.0)
            if eq is not None:
                prev_eq = eq

            if terminated or truncated:
                break

        elapsed_ms = int((time.perf_counter() - started) * 1000.0)
        out = {
            "val_sharpe": float(_sharpe_like(returns)),
            "eval_total_reward": float(sum(rewards)),
            "eval_steps": int(len(rewards)),
            "model_path": str(model_path),
            "net_arch": args.net_arch,
            "learning_rate": float(args.learning_rate),
            "gamma": float(args.gamma),
            "batch_size": int(args.batch_size),
            "entropy_coef": float(args.entropy_coef),
            "seed": int(args.seed),
            "duration_ms": elapsed_ms,
        }
        print(json.dumps(out, ensure_ascii=False))
        return 0
    finally:
        try:
            gym_env.close()
        except Exception:
            pass
        try:
            env.close()
        except Exception:
            pass


if __name__ == "__main__":
    raise SystemExit(main())
