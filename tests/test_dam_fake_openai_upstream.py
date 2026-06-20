import importlib.util
import json
import threading
import unittest
import urllib.request
from http.server import ThreadingHTTPServer
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "dam_fake_openai_upstream.py"


def load_module():
    spec = importlib.util.spec_from_file_location("dam_fake_openai_upstream", SCRIPT)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class DamFakeOpenAiUpstreamTests(unittest.TestCase):
    def test_models_endpoint_returns_fake_model(self):
        fake = load_module()
        server = ThreadingHTTPServer(("127.0.0.1", 0), fake.Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{server.server_port}/v1/models") as response:
                payload = json.load(response)
            self.assertEqual(payload["data"][0]["id"], "fake-deterministic")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)

    def test_chat_completion_echoes_exact_prompt_suffix(self):
        fake = load_module()
        server = ThreadingHTTPServer(("127.0.0.1", 0), fake.Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            body = json.dumps(
                {
                    "model": "local",
                    "messages": [
                        {"role": "user", "content": "Repeat exactly. alpha=one beta=two"}
                    ],
                }
            ).encode("utf-8")
            request = urllib.request.Request(
                f"http://127.0.0.1:{server.server_port}/v1/chat/completions",
                data=body,
                headers={"content-type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(request) as response:
                payload = json.load(response)
            self.assertEqual(payload["choices"][0]["message"]["content"], "alpha=one beta=two")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)

    def test_chat_completion_breaks_dam_reference_open_brackets_for_transform_prompt(self):
        fake = load_module()
        server = ThreadingHTTPServer(("127.0.0.1", 0), fake.Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            content = "Insert one space after the opening bracket in every DAM token. Text: [email:abc] [ssn:def]"
            body = json.dumps(
                {
                    "model": "local",
                    "messages": [{"role": "user", "content": content}],
                }
            ).encode("utf-8")
            request = urllib.request.Request(
                f"http://127.0.0.1:{server.server_port}/v1/chat/completions",
                data=body,
                headers={"content-type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(request) as response:
                payload = json.load(response)
            self.assertEqual(payload["choices"][0]["message"]["content"], "[ email:abc] [ ssn:def]")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main()
