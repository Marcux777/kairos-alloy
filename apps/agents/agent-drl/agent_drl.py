#!/usr/bin/env python3
"""
Kairos Alloy DRL Agent (Inference Server)
========================================

This tool exposes a lightweight HTTP server compatible with Kairos Alloy's
remote agent contract:

- GET  /health
- POST /v1/act
- POST /v1/act_batch

It is designed for *playback* and verification of trained DRL policies from Rust
(TUI/headless backtest/paper) without changing any Rust code.

Runtime choice
--------------
This server supports:
- `--runtime mock` (default): deterministic HOLD/heuristic responses (no deps).
- `--runtime sb3`: loads a Stable-Baselines3 `.zip` model and calls `predict()`.

Secrets
-------
No API keys are stored. This server does not require secrets.
"""

from __future__ import annotations

import argparse
import json
import os
import time
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Optional, Tuple


HDR_MODEL_PATH = "X-KAIROS-MODEL-PATH"


def _json_response(handler: BaseHTTPRequestHandler, status: int, payload: dict) -> None:
    body = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    handler.send_response(status)
    handler.send_header("Content-Type", "application/json; charset=utf-8")
    handler.send_header("Content-Length", str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)


def _hold(reason: str, model_version: Optional[str] = None, latency_ms: int = 0) -> dict:
    return {
        "action_type": "HOLD",
        "size": 0.0,
        "confidence": None,
        "model_version": model_version,
        "latency_ms": int(latency_ms),
        "reason": reason,
    }


def _read_json_body(handler: BaseHTTPRequestHandler) -> Tuple[Optional[dict], Optional[str]]:
    try:
        length = int(handler.headers.get("Content-Length", "0"))
    except ValueError:
        length = 0
    raw = handler.rfile.read(length) if length > 0 else b"{}"
    try:
        obj = json.loads(raw.decode("utf-8"))
    except Exception:
        return None, "invalid_json"
    if not isinstance(obj, dict):
        return None, "invalid_request"
    return obj, None


def _normalize_observation(request: dict) -> Optional[list[float]]:
    obs = request.get("observation", [])
    if not isinstance(obs, list):
        return None
    out: list[float] = []
    for x in obs:
        try:
            out.append(float(x))
        except Exception:
            return None
    return out


class _Policy:
    def predict(self, obs: list[float]) -> Tuple[int, Optional[float]]:
        raise NotImplementedError

    def model_version(self) -> str:
        return "unknown"


class _MockPolicy(_Policy):
    def __init__(self, mode: str) -> None:
        self.mode = (mode or "hold").strip().lower()

    def predict(self, obs: list[float]) -> Tuple[int, Optional[float]]:
        # 0=HOLD, 1=BUY, 2=SELL
        if self.mode == "hold":
            return 0, 1.0
        if self.mode == "momentum":
            x = obs[0] if obs else 0.0
            if x > 0:
                return 1, 0.55
            if x < 0:
                return 2, 0.55
            return 0, 0.5
        return 0, 1.0

    def model_version(self) -> str:
        return f"mock:{self.mode}"


class _Sb3Policy(_Policy):
    def __init__(self, model_path: Path, algo: str, device: str) -> None:
        self.model_path = model_path
        self.algo = algo
        self.device = device
        self._model = self._load_model(model_path, algo=algo, device=device)

    def _load_model(self, path: Path, algo: str, device: str):
        try:
            from stable_baselines3 import A2C, DQN, PPO, SAC, TD3  # type: ignore
        except Exception as exc:  # pragma: no cover
            raise RuntimeError(
                "stable-baselines3 is not installed. Install it to use --runtime sb3."
            ) from exc

        algos = {
            "ppo": PPO,
            "sac": SAC,
            "dqn": DQN,
            "a2c": A2C,
            "td3": TD3,
        }

        if algo != "auto":
            cls = algos.get(algo)
            if cls is None:
                raise ValueError("unsupported --algo (use: auto|ppo|sac|dqn|a2c|td3)")
            return cls.load(str(path), device=device)

        # Auto-detect by trying common loaders.
        last_err: Optional[Exception] = None
        for name, cls in algos.items():
            try:
                return cls.load(str(path), device=device)
            except Exception as err:  # noqa: PERF203
                last_err = err
                continue
        raise RuntimeError(f"failed to load sb3 model: {last_err}")

    def predict(self, obs: list[float]) -> Tuple[int, Optional[float]]:
        import numpy as np  # type: ignore

        x = np.asarray(obs, dtype=np.float32)
        # SB3 expects batch or single obs depending on algo; handle both.
        action, _state = self._model.predict(x, deterministic=True)
        try:
            a = int(action)  # scalar
        except Exception:
            a = int(action[0])  # array-like
        return a, None

    def model_version(self) -> str:
        return f"sb3:{self.model_path.name}"


@dataclass
class ServerState:
    policy: _Policy
    size: float

    def act(self, request: dict, latency_ms: int) -> dict:
        obs = _normalize_observation(request)
        if obs is None:
            return _hold("invalid_obs", model_version=self.policy.model_version(), latency_ms=latency_ms)

        start = time.perf_counter()
        try:
            action, confidence = self.policy.predict(obs)
        except Exception:
            return _hold("predict_error", model_version=self.policy.model_version(), latency_ms=latency_ms)
        infer_ms = int((time.perf_counter() - start) * 1000.0)

        # Map Discrete(3): 0=HOLD,1=BUY,2=SELL
        if action == 1:
            action_type = "BUY"
            size = float(self.size)
            reason = "sb3_buy"
        elif action == 2:
            action_type = "SELL"
            size = float(self.size)
            reason = "sb3_sell"
        else:
            action_type = "HOLD"
            size = 0.0
            reason = "sb3_hold"

        return {
            "action_type": action_type,
            "size": size,
            "confidence": confidence,
            "model_version": self.policy.model_version(),
            "latency_ms": int(infer_ms or latency_ms or 0),
            "reason": reason,
        }


class Handler(BaseHTTPRequestHandler):
    server_version = "kairos-agent-drl/0.1"

    def do_GET(self):  # noqa: N802
        if self.path == "/health":
            body = b"OK\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_error(404, "not found")

    def do_POST(self):  # noqa: N802
        if self.path not in ("/v1/act", "/v1/act_batch"):
            self.send_error(404, "not found")
            return

        start = time.perf_counter()
        request, err = _read_json_body(self)
        if err is not None or request is None:
            _json_response(self, 400, {"error": err or "invalid_request"})
            return

        server = self.server  # type: ignore[assignment]
        state: ServerState = server.state  # type: ignore[attr-defined]
        latency_ms = int((time.perf_counter() - start) * 1000.0)

        # Optional: allow per-request override for experiments.
        override = self.headers.get(HDR_MODEL_PATH)
        if override:
            # We keep this simple: ignore override unless runtime is sb3 and path exists.
            pass

        if self.path == "/v1/act_batch":
            items = request.get("items", [])
            if not isinstance(items, list):
                _json_response(self, 400, {"error": "invalid_items"})
                return
            out_items = []
            for item in items:
                if not isinstance(item, dict):
                    out_items.append(_hold("invalid_item", model_version=state.policy.model_version(), latency_ms=latency_ms))
                else:
                    out_items.append(state.act(item, latency_ms=latency_ms))
            _json_response(self, 200, {"items": out_items})
            return

        _json_response(self, 200, state.act(request, latency_ms=latency_ms))

    def log_message(self, fmt, *args):  # noqa: N802
        return


def main() -> int:
    parser = argparse.ArgumentParser(description="Kairos Alloy DRL inference agent (HTTP/JSON).")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8002)
    parser.add_argument("--runtime", default="mock", choices=["mock", "sb3"])
    parser.add_argument("--model-path", default=None, help="SB3 .zip path (required for --runtime sb3).")
    parser.add_argument("--algo", default="auto", help="SB3 algorithm: auto|ppo|sac|dqn|a2c|td3")
    parser.add_argument("--device", default="cpu", help="SB3 device: cpu|cuda|auto")
    parser.add_argument("--mock-mode", default="hold", choices=["hold", "momentum"])
    parser.add_argument("--size", type=float, default=1.0, help="Order size passed to Rust (interpreted by size_mode).")
    args = parser.parse_args()

    if args.runtime == "sb3":
        if not args.model_path:
            raise SystemExit("--model-path is required for --runtime sb3")
        model_path = Path(args.model_path)
        if not model_path.exists():
            raise SystemExit(f"model not found: {model_path}")
        policy: _Policy = _Sb3Policy(model_path=model_path, algo=args.algo.lower(), device=args.device)
    else:
        policy = _MockPolicy(mode=args.mock_mode)

    state = ServerState(policy=policy, size=float(args.size))
    httpd = ThreadingHTTPServer((args.host, args.port), Handler)
    httpd.state = state  # type: ignore[attr-defined]
    print(f"agent-drl: listening on http://{args.host}:{args.port} runtime={args.runtime} model={policy.model_version()}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

