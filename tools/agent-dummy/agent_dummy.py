#!/usr/bin/env python3
import argparse
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def _json_response(handler: BaseHTTPRequestHandler, status: int, payload: dict) -> None:
    body = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    handler.send_response(status)
    handler.send_header("Content-Type", "application/json; charset=utf-8")
    handler.send_header("Content-Length", str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)


class Handler(BaseHTTPRequestHandler):
    server_version = "kairos-agent-dummy/0.1"

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

        mode = getattr(self.server, "mode", "hold")
        action_type = "HOLD"
        size = 0.0

        if mode == "tiny_buy":
            action_type = "BUY"
            size = 0.0001
        elif mode == "momentum":
            obs = request.get("observation", [])
            if isinstance(obs, list) and len(obs) > 0:
                try:
                    x = float(obs[0])
                except Exception:
                    x = 0.0
                if x > 0:
                    action_type = "BUY"
                    size = 0.0001
                elif x < 0:
                    action_type = "SELL"
                    size = 1.0

        latency_ms = int((time.perf_counter() - start) * 1000.0)
        response = {
            "action_type": action_type,
            "size": size,
            "confidence": 1.0,
            "model_version": "dummy-0.1",
            "latency_ms": latency_ms,
        }
        _json_response(self, 200, response)

    def log_message(self, fmt, *args):  # noqa: N802
        # Keep stdout clean for quickstart usage.
        return


def main() -> int:
    parser = argparse.ArgumentParser(description="Kairos Alloy dummy agent (HTTP/JSON).")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument(
        "--mode",
        default="hold",
        choices=["hold", "tiny_buy", "momentum"],
        help="Response policy (default: hold).",
    )
    args = parser.parse_args()

    httpd = ThreadingHTTPServer((args.host, args.port), Handler)
    httpd.mode = args.mode
    print(f"agent-dummy: listening on http://{args.host}:{args.port} mode={args.mode}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

