#!/usr/bin/env python3
"""
Kairos Gym (research)
=====================

This module provides a Gymnasium-compatible environment that uses the existing
Kairos Alloy *remote agent* contract (HTTP ActionRequest -> ActionResponse) as
the step boundary.

Design (matches the academic plan):
- Communication: local HTTP (Rust acts as the HTTP client via AgentClient).
- Granularity: bar-based (1 step == 1 ActionRequest).
- No Rust changes required: we spawn the Rust binary with a config whose
  `agent.url` points to this process.

Key idea
--------
The Rust runner advances the simulation and calls our `/v1/act` endpoint every
bar. We *block* that HTTP request until the RL loop supplies an action via
`env.step(action)`. Then we wait for the *next* ActionRequest to arrive and use
it as the next observation.

Limitations (current MVP constraints)
-------------------------------------
- Episodes are executed by running the Rust process for a full backtest/paper
  run. `reset()` restarts the Rust process.
- To run windowed episodes (walk-forward), use `mode="sweep"` with a single
  split (start/end). This reuses `kairos-application`'s in-memory split filter.

Security / secrets
------------------
This module does not persist API keys. If your agent requires secrets (LLMs),
pass them via runtime headers or environment variables in your *agent* process,
not in the config snapshot.
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import random
import re
import socket
import subprocess
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Optional, Tuple, Union

try:
    import numpy as np
except Exception as exc:  # pragma: no cover
    raise SystemExit("kairos_gym requires numpy. Install via notebooks/requirements.txt") from exc


JsonDict = dict[str, Any]
Obs = np.ndarray


def _now_ms() -> int:
    return int(time.time() * 1000)


def _free_tcp_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def _json_response(handler, status: int, payload: JsonDict) -> None:
    body = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    handler.send_response(status)
    handler.send_header("Content-Type", "application/json; charset=utf-8")
    handler.send_header("Content-Length", str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)


def _hold(reason: str) -> JsonDict:
    return {
        "action_type": "HOLD",
        "size": 0.0,
        "confidence": None,
        "model_version": "kairos_gym",
        "latency_ms": 0,
        "reason": reason,
    }


def _patch_toml_value(raw: str, section: str, key: str, value_literal: str) -> str:
    """
    Patch a TOML key inside a single-level [section] block.
    If key is missing, append it to the section.
    """
    lines = raw.splitlines(keepends=True)
    out: list[str] = []
    in_section = False
    patched = False
    section_header = re.compile(rf"^\s*\[{re.escape(section)}\]\s*$")
    any_section = re.compile(r"^\s*\[.*\]\s*$")
    key_re = re.compile(rf"^\s*{re.escape(key)}\s*=")

    for idx, line in enumerate(lines):
        if section_header.match(line.strip()):
            in_section = True
            out.append(line)
            continue
        if in_section and any_section.match(line.strip()):
            if not patched:
                out.append(f'{key} = {value_literal}\n')
                patched = True
            in_section = False
            out.append(line)
            continue
        if in_section and key_re.match(line):
            out.append(f'{key} = {value_literal}\n')
            patched = True
            continue
        out.append(line)

    if in_section and not patched:
        out.append(f'{key} = {value_literal}\n')
        patched = True

    if not patched:
        # section not present; append at end
        if out and not out[-1].endswith("\n"):
            out[-1] += "\n"
        out.append(f"\n[{section}]\n{key} = {value_literal}\n")
    return "".join(out)


def _patch_config_for_gym(
    base_toml: str,
    *,
    run_id: str,
    agent_url: str,
    out_dir: Optional[str],
    force_report_html_off: bool = True,
) -> str:
    raw = base_toml
    raw = _patch_toml_value(raw, "run", "run_id", json.dumps(run_id, ensure_ascii=False))
    raw = _patch_toml_value(raw, "agent", "mode", json.dumps("remote"))
    raw = _patch_toml_value(raw, "agent", "url", json.dumps(agent_url, ensure_ascii=False))
    if out_dir is not None:
        raw = _patch_toml_value(raw, "paths", "out_dir", json.dumps(out_dir, ensure_ascii=False))
    if force_report_html_off:
        raw = _patch_toml_value(raw, "report", "html", "false")
    return raw


def _make_single_split_sweep_toml(
    *,
    sweep_id: str,
    mode: str,
    base_config_path: Path,
    split_id: str,
    split_start: Optional[str],
    split_end: Optional[str],
) -> str:
    if mode not in ("backtest", "paper"):
        raise ValueError("mode must be backtest|paper")
    split_lines = []
    split_lines.append('[[splits]]\n')
    split_lines.append(f'id = {json.dumps(split_id)}\n')
    if split_start is not None:
        split_lines.append(f'start = {json.dumps(split_start)}\n')
    if split_end is not None:
        split_lines.append(f'end = {json.dumps(split_end)}\n')

    return (
        f'[base]\nconfig = {json.dumps(str(base_config_path))}\n\n'
        f'[sweep]\n'
        f'id = {json.dumps(sweep_id)}\n'
        f'mode = {json.dumps(mode)}\n'
        f'parallelism = 1\n'
        f'resume = false\n\n'
        + "".join(split_lines)
    )


@dataclass
class _ActionCall:
    request: JsonDict
    received_at_ms: int
    _event: threading.Event
    _response: Optional[JsonDict] = None

    def set_response(self, response: JsonDict) -> None:
        self._response = response
        self._event.set()

    def wait(self, timeout_s: float) -> Optional[JsonDict]:
        ok = self._event.wait(timeout_s)
        if not ok:
            return None
        return self._response


class _Bridge:
    def __init__(self, timeout_s: float):
        self.timeout_s = timeout_s
        self.incoming: "queue.Queue[_ActionCall]" = queue.Queue()


def _make_http_server(bridge: _Bridge, host: str, port: int):
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    class Handler(BaseHTTPRequestHandler):
        server_version = "kairos-gym/0.1"

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
            if self.path != "/v1/act":
                self.send_error(404, "not found")
                return

            try:
                length = int(self.headers.get("Content-Length", "0"))
            except ValueError:
                length = 0
            raw = self.rfile.read(length) if length > 0 else b"{}"
            try:
                request = json.loads(raw.decode("utf-8"))
            except Exception:
                _json_response(self, 400, {"error": "invalid_json"})
                return
            if not isinstance(request, dict):
                _json_response(self, 400, {"error": "invalid_request"})
                return

            call = _ActionCall(
                request=request,
                received_at_ms=_now_ms(),
                _event=threading.Event(),
            )
            bridge.incoming.put(call)

            response = call.wait(bridge.timeout_s)
            if response is None:
                response = _hold("gym_timeout")
            _json_response(self, 200, response)

        def log_message(self, fmt, *args):  # noqa: N802
            return

    return ThreadingHTTPServer((host, port), Handler)


class KairosGymEnv:
    """
    Gym-like environment backed by a Kairos Alloy Rust process.

    This is intentionally lightweight and does not hard-require Gymnasium at import
    time. If you want the `gymnasium.Env` base class, use `as_gymnasium_env()`.
    """

    def __init__(
        self,
        *,
        base_config_path: Union[str, Path],
        rust_cmd: Optional[list[str]] = None,
        mode: str = "backtest",
        split_start: Optional[str] = None,
        split_end: Optional[str] = None,
        out_dir: Optional[Union[str, Path]] = None,
        agent_host: str = "127.0.0.1",
        agent_port: Optional[int] = None,
        agent_timeout_s: float = 30.0,
        observation_builder: Optional[Callable[[JsonDict], Obs]] = None,
        reward_fn: Optional[Callable[[JsonDict, JsonDict], float]] = None,
        action_to_response: Optional[Callable[[Any], JsonDict]] = None,
        seed: Optional[int] = None,
    ):
        self.base_config_path = Path(base_config_path)
        self.mode = mode.strip().lower()
        self.split_start = split_start
        self.split_end = split_end
        self.out_dir = str(out_dir) if out_dir is not None else None

        self.agent_host = agent_host
        self.agent_port = agent_port or _free_tcp_port()
        self.agent_timeout_s = float(agent_timeout_s)

        self.bridge = _Bridge(timeout_s=self.agent_timeout_s)
        self._httpd = _make_http_server(self.bridge, self.agent_host, self.agent_port)
        self._server_thread = threading.Thread(target=self._httpd.serve_forever, daemon=True)
        self._server_thread.start()

        self.agent_url = f"http://{self.agent_host}:{self.agent_port}"

        self._rng = random.Random(seed if seed is not None else 0)
        self._episode_seed = seed

        self._observation_builder = observation_builder or self._default_observation_builder
        self._reward_fn = reward_fn or self._default_reward_fn
        self._action_to_response = action_to_response or self._default_action_to_response

        self._rust_cmd = rust_cmd or [
            "cargo",
            "run",
            "-q",
            "-p",
            "kairos-tui",
            "--",
            "--headless",
            "--mode",
            self.mode if self.mode != "sweep" else "sweep",
        ]

        self._proc: Optional[subprocess.Popen] = None
        self._current_call: Optional[_ActionCall] = None
        self._prev_equity: Optional[float] = None
        self._obs_len: Optional[int] = None
        self._run_id: Optional[str] = None
        self._tmp_dir: Optional[Path] = None

    def _default_observation_builder(self, request: JsonDict) -> Obs:
        obs = request.get("observation", [])
        if not isinstance(obs, list):
            obs = []
        arr = np.asarray(obs, dtype=np.float32)
        return arr

    def _default_reward_fn(self, prev_req: JsonDict, next_req: JsonDict) -> float:
        prev_eq = float(prev_req.get("portfolio_state", {}).get("equity", 0.0))
        next_eq = float(next_req.get("portfolio_state", {}).get("equity", prev_eq))
        return next_eq - prev_eq

    def _default_action_to_response(self, action: Any) -> JsonDict:
        # Default: Discrete(3) -> HOLD/BUY/SELL with size=1.0.
        # 0=HOLD, 1=BUY, 2=SELL.
        try:
            a = int(action)
        except Exception:
            return _hold("invalid_action")

        if a == 0:
            return {
                "action_type": "HOLD",
                "size": 0.0,
                "confidence": None,
                "model_version": "kairos_gym",
                "latency_ms": 0,
                "reason": "gym_hold",
            }
        if a == 1:
            return {
                "action_type": "BUY",
                "size": 1.0,
                "confidence": None,
                "model_version": "kairos_gym",
                "latency_ms": 0,
                "reason": "gym_buy",
            }
        if a == 2:
            return {
                "action_type": "SELL",
                "size": 1.0,
                "confidence": None,
                "model_version": "kairos_gym",
                "latency_ms": 0,
                "reason": "gym_sell",
            }
        return _hold("invalid_action")

    def _spawn_rust(self, config_path: Path, sweep_path: Optional[Path]) -> subprocess.Popen:
        cmd = list(self._rust_cmd)
        if self.mode == "sweep" or sweep_path is not None:
            # Ensure we use sweep mode and pass sweep config.
            cmd = [
                "cargo",
                "run",
                "-q",
                "-p",
                "kairos-tui",
                "--",
                "--headless",
                "--mode",
                "sweep",
                "--sweep-config",
                str(sweep_path),
            ]
        else:
            cmd = cmd + ["--config", str(config_path)]

        stdout_path = (config_path.parent / "rust_stdout.log").as_posix()
        stderr_path = (config_path.parent / "rust_stderr.log").as_posix()
        stdout = open(stdout_path, "wb")
        stderr = open(stderr_path, "wb")
        return subprocess.Popen(cmd, cwd=str(self.base_config_path.parent), stdout=stdout, stderr=stderr)

    def _wait_for_call_or_done(self, timeout_s: float) -> Optional[_ActionCall]:
        deadline = time.time() + timeout_s
        while time.time() < deadline:
            try:
                return self.bridge.incoming.get(timeout=0.05)
            except queue.Empty:
                pass
            if self._proc is not None and self._proc.poll() is not None:
                return None
        return None

    def reset(self) -> Tuple[Obs, JsonDict]:
        self.close_episode()

        if not self.base_config_path.exists():
            raise FileNotFoundError(str(self.base_config_path))

        base_toml = self.base_config_path.read_text(encoding="utf-8")
        run_id = f"gym_{int(time.time())}_{self._rng.randint(0, 10_000_000):07d}"
        self._run_id = run_id

        tmp_dir = Path("/tmp") / "kairos_gym" / run_id
        tmp_dir.mkdir(parents=True, exist_ok=True)
        self._tmp_dir = tmp_dir

        patched = _patch_config_for_gym(
            base_toml,
            run_id=run_id,
            agent_url=self.agent_url,
            out_dir=self.out_dir,
            force_report_html_off=True,
        )
        cfg_path = tmp_dir / "config.toml"
        cfg_path.write_text(patched, encoding="utf-8")

        sweep_path: Optional[Path] = None
        if self.mode == "sweep":
            sweep_path = tmp_dir / "sweep.toml"
            sweep_toml = _make_single_split_sweep_toml(
                sweep_id=f"gym_sweep_{run_id}",
                mode="backtest",
                base_config_path=cfg_path,
                split_id="episode",
                split_start=self.split_start,
                split_end=self.split_end,
            )
            sweep_path.write_text(sweep_toml, encoding="utf-8")

        self._proc = self._spawn_rust(cfg_path, sweep_path)

        call = self._wait_for_call_or_done(timeout_s=30.0)
        if call is None:
            code = self._proc.poll() if self._proc is not None else None
            raise RuntimeError(f"rust process did not request action (exit={code})")

        self._current_call = call
        req = call.request
        obs = self._observation_builder(req)
        if self._obs_len is None:
            self._obs_len = int(obs.shape[0])
        elif int(obs.shape[0]) != self._obs_len:
            raise RuntimeError(f"observation length changed: {obs.shape[0]} != {self._obs_len}")

        self._prev_equity = float(req.get("portfolio_state", {}).get("equity", 0.0))

        info: JsonDict = {
            "run_id": req.get("run_id"),
            "timestamp": req.get("timestamp"),
            "symbol": req.get("symbol"),
            "timeframe": req.get("timeframe"),
            "equity": self._prev_equity,
        }
        return obs, info

    def step(self, action: Any) -> Tuple[Obs, float, bool, bool, JsonDict]:
        if self._current_call is None:
            raise RuntimeError("env.step() called before reset()")
        if self._proc is None:
            raise RuntimeError("rust process is not running")

        call = self._current_call
        prev_req = call.request

        response = self._action_to_response(action)
        call.set_response(response)

        next_call = self._wait_for_call_or_done(timeout_s=30.0)
        if next_call is None:
            terminated = True
            truncated = False
            reward = 0.0
            info: JsonDict = {
                "run_id": prev_req.get("run_id"),
                "timestamp": prev_req.get("timestamp"),
                "terminated_reason": "rust_exit",
                "last_equity": float(prev_req.get("portfolio_state", {}).get("equity", 0.0)),
            }
            obs = self._observation_builder(prev_req)
            self._current_call = None
            return obs, reward, terminated, truncated, info

        next_req = next_call.request
        reward = float(self._reward_fn(prev_req, next_req))
        self._current_call = next_call

        obs = self._observation_builder(next_req)
        terminated = False
        truncated = False
        info = {
            "run_id": next_req.get("run_id"),
            "timestamp": next_req.get("timestamp"),
            "equity": float(next_req.get("portfolio_state", {}).get("equity", 0.0)),
            "agent_reason": response.get("reason"),
        }
        return obs, reward, terminated, truncated, info

    def close_episode(self) -> None:
        if self._current_call is not None:
            try:
                self._current_call.set_response(_hold("gym_close"))
            except Exception:
                pass
        self._current_call = None
        self._prev_equity = None

        if self._proc is not None:
            try:
                if self._proc.poll() is None:
                    self._proc.terminate()
                    try:
                        self._proc.wait(timeout=2.0)
                    except subprocess.TimeoutExpired:
                        self._proc.kill()
                        self._proc.wait(timeout=2.0)
            finally:
                self._proc = None

    def close(self) -> None:
        self.close_episode()
        try:
            self._httpd.shutdown()
        except Exception:
            pass

    # Convenience for libraries that expect gymnasium.Env subclass.
    def as_gymnasium_env(self):
        try:
            import gymnasium as gym  # type: ignore
            from gymnasium import spaces  # type: ignore
        except Exception as exc:  # pragma: no cover
            raise RuntimeError(
                "gymnasium is not installed. Install it for RL training."
            ) from exc

        env = self

        class _Wrapped(gym.Env):  # type: ignore
            metadata = {"render_modes": []}

            def __init__(self):
                super().__init__()
                self._inner = env
                # Spaces become known after the first reset (obs length depends on config/features).
                self.observation_space = spaces.Box(
                    low=-np.inf, high=np.inf, shape=(1,), dtype=np.float32
                )
                self.action_space = spaces.Discrete(3)

            def reset(self, *, seed: Optional[int] = None, options: Optional[dict] = None):
                if seed is not None:
                    # Keep deterministic behavior for wrappers relying on seed.
                    self._inner._rng = random.Random(seed)
                obs, info = self._inner.reset()
                self.observation_space = spaces.Box(
                    low=-np.inf, high=np.inf, shape=(int(obs.shape[0]),), dtype=np.float32
                )
                return obs, info

            def step(self, action):
                return self._inner.step(action)

            def close(self):
                return self._inner.close()

        return _Wrapped()


def _cli() -> int:
    parser = argparse.ArgumentParser(description="Kairos Gym (research wrapper).")
    parser.add_argument("--base-config", required=True, help="Base TOML config path.")
    parser.add_argument("--mode", default="backtest", choices=["backtest", "sweep"])
    parser.add_argument("--split-start", default=None, help="Sweep split start (RFC3339).")
    parser.add_argument("--split-end", default=None, help="Sweep split end (RFC3339).")
    parser.add_argument("--out-dir", default=None, help="Override paths.out_dir for the run.")
    parser.add_argument("--steps", type=int, default=25, help="Smoke steps to run.")
    args = parser.parse_args()

    env = KairosGymEnv(
        base_config_path=args.base_config,
        mode=args.mode,
        split_start=args.split_start,
        split_end=args.split_end,
        out_dir=args.out_dir,
    )

    obs, info = env.reset()
    print("reset:", {k: info.get(k) for k in ("run_id", "timestamp", "symbol", "timeframe", "equity")})
    total = 0.0
    for i in range(args.steps):
        action = env._rng.randint(0, 2)
        obs, reward, terminated, truncated, info = env.step(action)
        total += float(reward)
        print(f"step={i} a={action} r={reward:.6f} eq={info.get('equity')}")
        if terminated or truncated:
            break
    env.close()
    print("total_reward:", total)
    return 0


if __name__ == "__main__":
    raise SystemExit(_cli())

