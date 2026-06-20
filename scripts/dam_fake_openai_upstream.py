#!/usr/bin/env python3
"""Deterministic loopback OpenAI-compatible fake upstream for DAM smoke tests.

Implements the tiny API surface needed by `scripts/dam-build.sh agent-protection-smoke`:
- GET /v1/models
- POST /v1/chat/completions

The handler echoes exact-echo prompts and deliberately breaks DAM reference syntax for
adversarial token-transformation prompts. Keep this loopback-only and synthetic-data-only.
"""

from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Handler(BaseHTTPRequestHandler):
    server_version = "DamFakeOpenAI/0.1"

    def log_message(self, format: str, *args: object) -> None:
        return

    def _send_json(self, code: int, payload: dict) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler API
        if self.path == "/v1/models":
            self._send_json(200, {"object": "list", "data": [{"id": "fake-deterministic"}]})
        else:
            self._send_json(404, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler API
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length)
        try:
            data = json.loads(raw or b"{}")
        except json.JSONDecodeError:
            data = {}

        user_content = ""
        for message in reversed(data.get("messages") or []):
            if message.get("role") == "user":
                user_content = str(message.get("content") or "")
                break

        if "Text:" in user_content:
            content = user_content.split("Text:", 1)[1].strip()
            content = content.replace("[email:", "[ email:").replace("[ssn:", "[ ssn:")
        elif "alpha=" in user_content:
            content = user_content[user_content.index("alpha=") :].strip()
        else:
            content = user_content

        self._send_json(
            200,
            {
                "id": "chatcmpl-fake",
                "object": "chat.completion",
                "model": "fake-deterministic",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": content},
                        "finish_reason": "stop",
                    }
                ],
            },
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=18080)
    args = parser.parse_args()
    ThreadingHTTPServer((args.host, args.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
