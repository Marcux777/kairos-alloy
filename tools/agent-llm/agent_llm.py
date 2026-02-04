#!/usr/bin/env python3
import argparse
import datetime as _dt
import hashlib
import json
import os
import random
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Optional, Tuple, Dict, Any


PROMPT_VERSION = "v1"
MAX_REASON_CHARS = 2000
HDR_LLM_PROVIDER = "X-KAIROS-LLM-PROVIDER"
HDR_LLM_API_KEY = "X-KAIROS-LLM-API-KEY"
HDR_LLM_MODEL = "X-KAIROS-LLM-MODEL"


def _utc_now_iso() -> str:
    return _dt.datetime.now(tz=_dt.timezone.utc).isoformat()


def _json_response(handler: BaseHTTPRequestHandler, status: int, payload: dict) -> None:
    body = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    handler.send_response(status)
    handler.send_header("Content-Type", "application/json; charset=utf-8")
    handler.send_header("Content-Length", str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)


def _canonical_json(obj: dict) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _sha256_hex(data: str) -> str:
    return hashlib.sha256(data.encode("utf-8")).hexdigest()


def _clamp(v: float, lo: float, hi: float) -> float:
    return max(lo, min(hi, v))


class Cache:
    def __init__(self, path: str) -> None:
        self.path = path
        self._items = {}  # request_hash -> response dict
        self._loaded = False

    def load(self) -> None:
        if self._loaded:
            return
        self._loaded = True
        if not os.path.exists(self.path):
            return
        try:
            with open(self.path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        rec = json.loads(line)
                    except Exception:
                        continue
                    h = rec.get("request_hash")
                    resp = rec.get("response")
                    if isinstance(h, str) and isinstance(resp, dict):
                        self._items[h] = resp
        except Exception:
            # If cache is unreadable, proceed without it (best-effort).
            return

    def get(self, request_hash: str):
        self.load()
        return self._items.get(request_hash)

    def put(self, request_hash: str, response: dict, meta: dict) -> None:
        self.load()
        self._items[request_hash] = response
        os.makedirs(os.path.dirname(self.path), exist_ok=True)
        rec = {
            "request_hash": request_hash,
            "response": response,
            "created_at": _utc_now_iso(),
        }
        rec.update(meta)
        with open(self.path, "a", encoding="utf-8") as f:
            f.write(_canonical_json(rec))
            f.write("\n")


class GeminiClient:
    def __init__(
        self,
        api_key: str,
        model: str,
        temperature: float,
        max_output_tokens: int,
        http_timeout_s: float,
    ) -> None:
        self.api_key = api_key
        self.model = model
        self.temperature = temperature
        self.max_output_tokens = max_output_tokens
        self.http_timeout_s = http_timeout_s

    def generate_json(self, prompt: str) -> Tuple[str, int]:
        # Gemini Generative Language API endpoint (v1beta).
        url = (
            "https://generativelanguage.googleapis.com/v1beta/models/"
            + urllib.parse.quote(self.model, safe="")
            + ":generateContent?key="
            + urllib.parse.quote(self.api_key, safe="")
        )

        body = {
            "contents": [{"role": "user", "parts": [{"text": prompt}]}],
            "generationConfig": {
                "temperature": float(self.temperature),
                "maxOutputTokens": int(self.max_output_tokens),
                "responseMimeType": "application/json",
            },
        }
        headers = {"Content-Type": "application/json; charset=utf-8"}

        def do_request(req_body: dict) -> Tuple[dict, int]:
            raw = json.dumps(req_body, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
            req = urllib.request.Request(url=url, data=raw, method="POST", headers=headers)
            start = time.perf_counter()
            with urllib.request.urlopen(req, timeout=self.http_timeout_s) as resp:
                latency_ms = int((time.perf_counter() - start) * 1000.0)
                payload = json.loads(resp.read().decode("utf-8"))
            return payload, latency_ms

        try:
            payload, latency_ms = do_request(body)
        except urllib.error.HTTPError as e:
            # Best-effort fallback for API variants that reject responseMimeType.
            if e.code == 400:
                body2 = {
                    "contents": body["contents"],
                    "generationConfig": {
                        "temperature": float(self.temperature),
                        "maxOutputTokens": int(self.max_output_tokens),
                    },
                }
                payload, latency_ms = do_request(body2)
            else:
                raise

        text = ""
        try:
            candidates = payload.get("candidates") or []
            if candidates:
                content = candidates[0].get("content") or {}
                parts = content.get("parts") or []
                if parts:
                    text = parts[0].get("text") or ""
        except Exception:
            text = ""

        return text, latency_ms


class OpenAIClient:
    def __init__(
        self,
        api_key: str,
        model: str,
        temperature: float,
        max_output_tokens: int,
        http_timeout_s: float,
        base_url: str,
        json_mode: bool,
    ) -> None:
        self.api_key = api_key
        self.model = model
        self.temperature = temperature
        self.max_output_tokens = max_output_tokens
        self.http_timeout_s = http_timeout_s
        self.base_url = base_url.rstrip("/")
        self.json_mode = json_mode

    def generate_json(self, prompt: str) -> Tuple[str, int]:
        # Use Chat Completions for broad compatibility.
        url = f"{self.base_url}/chat/completions"
        headers = {
            "Content-Type": "application/json; charset=utf-8",
            "Authorization": f"Bearer {self.api_key}",
        }

        body: Dict[str, Any] = {
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": float(self.temperature),
            "max_tokens": int(self.max_output_tokens),
        }
        if self.json_mode:
            body["response_format"] = {"type": "json_object"}

        raw = json.dumps(body, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
        req = urllib.request.Request(url=url, data=raw, method="POST", headers=headers)

        start = time.perf_counter()
        with urllib.request.urlopen(req, timeout=self.http_timeout_s) as resp:
            latency_ms = int((time.perf_counter() - start) * 1000.0)
            payload = json.loads(resp.read().decode("utf-8"))

        text = ""
        try:
            choices = payload.get("choices") or []
            if choices:
                msg = choices[0].get("message") or {}
                text = msg.get("content") or ""
        except Exception:
            text = ""
        return text, latency_ms


def _extract_json_object(text: str) -> Optional[Dict[str, Any]]:
    if not isinstance(text, str):
        return None
    text = text.strip()
    if not text:
        return None
    if text.startswith("{") and text.endswith("}"):
        try:
            obj = json.loads(text)
            return obj if isinstance(obj, dict) else None
        except Exception:
            return None
    start = text.find("{")
    end = text.rfind("}")
    if start == -1 or end == -1 or end <= start:
        return None
    try:
        obj = json.loads(text[start : end + 1])
        return obj if isinstance(obj, dict) else None
    except Exception:
        return None


def _feature_schema_fingerprint(args: argparse.Namespace) -> Dict[str, Any]:
    return {
        "return_mode": args.return_mode,
        "sma_windows": args.sma_windows,
        "volatility_windows": args.volatility_windows,
        "rsi_enabled": bool(args.rsi_enabled),
        "sentiment_dim": args.sentiment_dim,
    }


def _parse_csv_ints(s: str) -> list[int]:
    s = (s or "").strip()
    if not s:
        return []
    out = []
    for part in s.split(","):
        part = part.strip()
        if not part:
            continue
        out.append(int(part))
    return out


def _name_observation(obs: list, args: argparse.Namespace) -> Dict[str, float]:
    values = []
    for x in obs:
        try:
            v = float(x)
        except Exception:
            v = 0.0
        values.append(v)

    sma_windows = _parse_csv_ints(args.sma_windows)
    vol_windows = _parse_csv_ints(args.volatility_windows)
    base_count = 1 + len(sma_windows) + len(vol_windows) + (1 if args.rsi_enabled else 0)
    sentiment_dim = 0
    if args.sentiment_dim != "auto":
        try:
            sentiment_dim = max(0, int(args.sentiment_dim))
        except Exception:
            sentiment_dim = 0
    else:
        sentiment_dim = max(0, len(values) - base_count)

    named = {}
    idx = 0
    named["ret"] = values[idx] if idx < len(values) else 0.0
    idx += 1
    for w in sma_windows:
        named[f"sma_{w}"] = values[idx] if idx < len(values) else 0.0
        idx += 1
    for w in vol_windows:
        named[f"vol_{w}"] = values[idx] if idx < len(values) else 0.0
        idx += 1
    if args.rsi_enabled:
        named["rsi_14"] = values[idx] if idx < len(values) else 0.0
        idx += 1
    for i in range(sentiment_dim):
        named[f"sentiment_{i}"] = values[idx] if idx < len(values) else 0.0
        idx += 1

    return named


def _build_prompt(request: Dict[str, Any], named_features: Dict[str, float], args: argparse.Namespace) -> str:
    portfolio = request.get("portfolio_state") if isinstance(request.get("portfolio_state"), dict) else {}
    header = {
        "timestamp": request.get("timestamp"),
        "symbol": request.get("symbol"),
        "timeframe": request.get("timeframe"),
        "run_id": request.get("run_id"),
        "api_version": request.get("api_version"),
        "feature_version": request.get("feature_version"),
    }
    schema = _feature_schema_fingerprint(args)

    instructions = (
        "You are a trading decision engine.\n"
        "Return ONLY valid JSON with keys: action_type, size, confidence, reason.\n"
        "action_type must be one of BUY, SELL, HOLD.\n"
        "size must be a non-negative number.\n"
        "If uncertain, return HOLD with size=0.\n"
    )
    if args.size_mode == "pct_equity":
        instructions += "For pct_equity, size must be between 0 and 1.\n"

    payload = {
        "header": header,
        "portfolio_state": {
            "cash": portfolio.get("cash"),
            "position_qty": portfolio.get("position_qty"),
            "position_avg_price": portfolio.get("position_avg_price"),
            "equity": portfolio.get("equity"),
        },
        "features": named_features,
        "feature_schema": schema,
        "constraints": {
            "size_mode": args.size_mode,
            "max_size": args.max_size,
        },
    }
    return instructions + "\nINPUT:\n" + _canonical_json(payload)


def _normalize_llm_decision(
    obj: Dict[str, Any], args: argparse.Namespace
) -> Tuple[str, float, Optional[float], Optional[str]]:
    action_type = str(obj.get("action_type", "HOLD")).upper().strip()
    if action_type not in ("BUY", "SELL", "HOLD"):
        action_type = "HOLD"

    size = obj.get("size", 0.0)
    try:
        size = float(size)
    except Exception:
        size = 0.0
    if not (size >= 0.0) or not (size < float("inf")):
        size = 0.0

    if args.size_mode == "pct_equity":
        size = _clamp(size, 0.0, float(args.max_size))

    confidence = obj.get("confidence", None)
    if confidence is not None:
        try:
            confidence = float(confidence)
        except Exception:
            confidence = None
        if confidence is not None and (not (0.0 <= confidence <= 1.0) or not (confidence < float("inf"))):
            confidence = None

    reason = obj.get("reason", None)
    if reason is not None:
        reason = str(reason)
        if len(reason) > MAX_REASON_CHARS:
            reason = reason[:MAX_REASON_CHARS]

    return action_type, size, confidence, reason


class Handler(BaseHTTPRequestHandler):
    server_version = "kairos-agent-llm/0.1"

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

        server = self.server  # type: ignore[assignment]
        latency_ms = int((time.perf_counter() - start) * 1000.0)
        llm_provider = self.headers.get(HDR_LLM_PROVIDER)
        llm_api_key = self.headers.get(HDR_LLM_API_KEY)
        llm_model = self.headers.get(HDR_LLM_MODEL)

        if self.path == "/v1/act_batch":
            items = request.get("items", [])
            if not isinstance(items, list):
                _json_response(self, 400, {"error": "invalid_items"})
                return
            out_items = []
            for idx, item in enumerate(items):
                if not isinstance(item, dict):
                    out_items.append(server.hold_response(reason="batch_invalid_item"))
                    continue
                # Evaluate only the last item (most recent); earlier items are holds.
                if idx == len(items) - 1:
                    out_items.append(
                        server.act_single(
                            item,
                            latency_ms=latency_ms,
                            llm_provider=llm_provider,
                            llm_api_key=llm_api_key,
                            llm_model=llm_model,
                        )
                    )
                else:
                    out_items.append(server.hold_response(reason="batch_hold"))
            _json_response(self, 200, {"items": out_items})
            return

        _json_response(
            self,
            200,
            server.act_single(
                request,
                latency_ms=latency_ms,
                llm_provider=llm_provider,
                llm_api_key=llm_api_key,
                llm_model=llm_model,
            ),
        )

    def log_message(self, fmt, *args):  # noqa: N802
        return


class Server(ThreadingHTTPServer):
    def __init__(self, addr, handler, args: argparse.Namespace) -> None:
        super().__init__(addr, handler)
        self.args = args
        self._counters = {}  # (run_id, symbol, timeframe) -> int
        self._caches = {}  # run_id -> Cache
        self._rng = random.Random(0)

        self.http_timeout_s = float(os.environ.get("KAIROS_LLM_HTTP_TIMEOUT_S", "10"))

    def _cache_for_run(self, run_id: str) -> Cache:
        cache = self._caches.get(run_id)
        if cache is not None:
            return cache
        path = os.path.join(self.args.cache_dir, run_id, "agent_llm_cache.jsonl")
        cache = Cache(path)
        self._caches[run_id] = cache
        return cache

    def hold_response(self, reason: str) -> dict:
        payload = {
            "action_type": "HOLD",
            "size": 0.0,
            "confidence": None,
            "model_version": "cadence",
            "latency_ms": 0,
            "reason": reason[:MAX_REASON_CHARS],
        }
        return payload

    def _should_eval(self, key: tuple[str, str, str]) -> bool:
        n = int(self.args.eval_every_n_bars)
        if n <= 0:
            return True
        c = self._counters.get(key, 0)
        self._counters[key] = c + 1
        return (c % n) == 0

    def _request_hash(self, request: dict, provider: str, model: str) -> str:
        envelope = {
            "prompt_version": PROMPT_VERSION,
            "agent": {
                "provider": provider,
                "model": model,
                "temperature": float(self.args.temperature),
                "max_output_tokens": int(self.args.max_output_tokens),
                "eval_every_n_bars": int(self.args.eval_every_n_bars),
                "size_mode": self.args.size_mode,
                "max_size": float(self.args.max_size),
            },
            "feature_schema": _feature_schema_fingerprint(self.args),
            "request": request,
        }
        return _sha256_hex(_canonical_json(envelope))

    def _mock_decision(self, request_hash: str) -> dict:
        # Deterministic mock: use hash prefix as seed for stable pseudo-decisions.
        seed = int(request_hash[:8], 16)
        rng = random.Random(seed)
        x = rng.random()
        if x < 0.10:
            return {"action_type": "SELL", "size": 0.25, "confidence": 0.55, "reason": "mock: sell"}
        if x < 0.20:
            return {"action_type": "BUY", "size": 0.25, "confidence": 0.55, "reason": "mock: buy"}
        return {"action_type": "HOLD", "size": 0.0, "confidence": 0.55, "reason": "mock: hold"}

    def _resolve_llm_settings(
        self,
        llm_provider: Optional[str],
        llm_model: Optional[str],
        llm_api_key: Optional[str],
    ) -> Tuple[str, str, Optional[str]]:
        provider = (llm_provider or self.args.provider or "").strip().lower()
        if provider not in ("gemini", "openai"):
            provider = (self.args.provider or "gemini").strip().lower()
        model = (llm_model or self.args.model or "").strip()
        if not model:
            model = self.args.model
        api_key = (llm_api_key or "").strip()
        if not api_key:
            if provider == "gemini":
                api_key = os.environ.get("GEMINI_API_KEY", "").strip()
            elif provider == "openai":
                api_key = os.environ.get("OPENAI_API_KEY", "").strip()
        return provider, model, (api_key if api_key else None)

    def _make_provider(self, provider: str, model: str, api_key: str):
        if provider == "gemini":
            return GeminiClient(
                api_key=api_key,
                model=model,
                temperature=float(self.args.temperature),
                max_output_tokens=int(self.args.max_output_tokens),
                http_timeout_s=self.http_timeout_s,
            )
        if provider == "openai":
            return OpenAIClient(
                api_key=api_key,
                model=model,
                temperature=float(self.args.temperature),
                max_output_tokens=int(self.args.max_output_tokens),
                http_timeout_s=self.http_timeout_s,
                base_url=self.args.openai_base_url,
                json_mode=bool(self.args.openai_json_mode),
            )
        raise ValueError(f"unsupported provider: {provider}")

    def act_single(
        self,
        request: dict,
        latency_ms: int,
        llm_provider: Optional[str] = None,
        llm_api_key: Optional[str] = None,
        llm_model: Optional[str] = None,
    ) -> dict:
        run_id = str(request.get("run_id") or "unknown")
        symbol = str(request.get("symbol") or "unknown")
        timeframe = str(request.get("timeframe") or "unknown")
        key = (run_id, symbol, timeframe)

        if not self._should_eval(key):
            return self.hold_response(reason="cadence_hold")

        provider, model, api_key = self._resolve_llm_settings(llm_provider, llm_model, llm_api_key)
        req_hash = self._request_hash(request, provider=provider, model=model)
        cache = self._cache_for_run(run_id)
        cached = cache.get(req_hash) if self.args.cache_mode in ("record_replay", "replay") else None
        if isinstance(cached, dict):
            return cached

        # Generate (live or mock), then optionally record.
        model_version = None
        llm_latency_ms = None
        reason = None

        if self.args.llm_mode == "mock":
            llm_start = time.perf_counter()
            decision = self._mock_decision(req_hash)
            llm_latency_ms = int((time.perf_counter() - llm_start) * 1000.0)
            model_version = "mock-0.1"
        else:
            if not api_key:
                decision = {
                    "action_type": "HOLD",
                    "size": 0.0,
                    "confidence": None,
                    "reason": "missing_api_key",
                }
                model_version = model
            else:
                obs = request.get("observation", [])
                if not isinstance(obs, list):
                    obs = []
                named = _name_observation(obs, self.args)
                prompt = _build_prompt(request, named, self.args)
                try:
                    client = self._make_provider(provider, model, api_key)
                    text, llm_latency_ms = client.generate_json(prompt)
                    obj = _extract_json_object(text) or {}
                    decision = obj
                    model_version = model
                except urllib.error.HTTPError as e:
                    decision = {"action_type": "HOLD", "size": 0.0, "confidence": None, "reason": f"llm_http_error:{e.code}"}
                    model_version = model
                except Exception:
                    decision = {"action_type": "HOLD", "size": 0.0, "confidence": None, "reason": "llm_error"}
                    model_version = model

        action_type, size, confidence, reason = _normalize_llm_decision(decision, self.args)
        payload = {
            "action_type": action_type,
            "size": float(size),
            "confidence": confidence,
            "model_version": model_version,
            "latency_ms": int(llm_latency_ms or latency_ms or 0),
            "reason": reason,
        }

        if self.args.cache_mode in ("record_replay", "record") and not isinstance(cached, dict):
            try:
                cache.put(
                    req_hash,
                    payload,
                    meta={
                        "model": model_version,
                        "llm_latency_ms": int(llm_latency_ms or 0),
                    },
                )
            except Exception:
                pass

        return payload


def main() -> int:
    parser = argparse.ArgumentParser(description="Kairos Alloy LLM agent (HTTP/JSON).")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8001)

    parser.add_argument("--provider", default="gemini", choices=["gemini", "openai"])
    parser.add_argument("--model", default=None)
    parser.add_argument("--llm-mode", default="mock", choices=["mock", "live"])
    parser.add_argument("--temperature", type=float, default=0.0)
    parser.add_argument("--max-output-tokens", type=int, default=256)
    parser.add_argument("--openai-base-url", default="https://api.openai.com/v1")
    parser.add_argument("--openai-json-mode", action="store_true", default=False)

    parser.add_argument("--eval-every-n-bars", type=int, default=240)
    parser.add_argument("--cache-mode", default="record_replay", choices=["record_replay", "record", "replay", "off"])
    parser.add_argument("--cache-dir", default="runs")

    parser.add_argument("--size-mode", default="pct_equity", choices=["pct_equity", "qty"])
    parser.add_argument("--max-size", type=float, default=1.0)

    parser.add_argument("--return-mode", default="log", choices=["log", "pct"])
    parser.add_argument("--sma-windows", default="10,50")
    parser.add_argument("--volatility-windows", default="10")
    parser.add_argument("--rsi-enabled", action="store_true", default=False)
    parser.add_argument("--sentiment-dim", default="auto")

    args = parser.parse_args()

    if args.model is None:
        if args.provider == "gemini":
            args.model = "gemini-1.5-flash"
        elif args.provider == "openai":
            args.model = "gpt-4o-mini"
        else:
            args.model = "unknown"

    httpd = Server((args.host, args.port), Handler, args)
    print(f"agent-llm: listening on http://{args.host}:{args.port} mode={args.llm_mode} model={args.model} n={args.eval_every_n_bars}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
