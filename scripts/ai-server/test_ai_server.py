import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

HERE = Path(__file__).parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, HERE / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


deploy, probe = load("deploy"), load("probe")


class DeployTests(unittest.TestCase):
    def test_pins_are_distinct_and_complete(self):
        profiles = deploy.CATALOG["profiles"]
        self.assertNotEqual(profiles["mage"]["port"], profiles["qwen"]["port"])
        self.assertEqual(len({p["port"] for p in profiles.values()}), len(profiles))
        for profile in profiles.values():
            self.assertRegex(profile["modelRevision"], r"^[0-9a-f]{40}$")
        self.assertIn("vllm==0.11.0", profiles["qwen"]["packages"])
        self.assertRegex(profiles["mage"]["sourceRevision"], r"^[0-9a-f]{40}$")

    def test_dry_run_does_not_install_or_create_files(self):
        with tempfile.TemporaryDirectory() as temp, patch.object(deploy.subprocess, "run", side_effect=AssertionError("Unexpected process")):
            root = Path(temp) / "not-created"
            for name, profile in deploy.CATALOG["profiles"].items():
                with contextlib.redirect_stdout(io.StringIO()) as output:
                    deploy.install(root, name, profile, dry_run=True)
                plan = json.loads(output.getvalue())
                self.assertEqual(plan["revision"], profile["modelRevision"])
            self.assertFalse(root.exists())

    def test_serve_plan_keeps_auth_out_of_process_arguments(self):
        for name, profile in deploy.CATALOG["profiles"].items():
            command = deploy.serve_command(Path("/env/bin/python"), profile, "/model", "127.0.0.1", profile["port"], 8192, .8)
            self.assertIn("127.0.0.1", command)
            self.assertEqual(command[command.index("--served-model-name") + 1], profile["servedModel"])
            self.assertNotIn("--api-key", command)
            if name == "mage":
                self.assertIn("LAP_AI_API_KEY", command[2])

    def test_key_survives_repeated_startup(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "key"
            first = deploy.load_or_create_key(path)
            self.assertEqual(first, deploy.load_or_create_key(path))
            self.assertGreaterEqual(len(first), 32)

    def test_driver_gate_rejects_incompatible_mage_environment(self):
        result = subprocess.CompletedProcess([], 0, stdout="CUDA Version: 12.8")
        with patch.object(deploy, "check_platform"), patch.object(deploy.shutil, "which", return_value="/bin/tool"), patch.object(deploy.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(ValueError, "13.0"):
                deploy.doctor(deploy.CATALOG["profiles"]["mage"])

    def test_driver_alone_does_not_replace_mage_cuda_toolkit(self):
        outputs = [subprocess.CompletedProcess([], 0, stdout="CUDA Version: 13.0"),
                   subprocess.CompletedProcess([], 0, stdout="Cuda compilation tools, release 12.8")]
        with patch.object(deploy, "check_platform"), patch.object(deploy.shutil, "which", return_value="/bin/tool"), patch.object(deploy.subprocess, "run", side_effect=outputs):
            with self.assertRaisesRegex(ValueError, "开发工具"):
                deploy.doctor(deploy.CATALOG["profiles"]["mage"])

    def test_install_refuses_unowned_nonempty_folder(self):
        with tempfile.TemporaryDirectory() as temp, patch.object(deploy, "doctor"), patch.object(deploy.subprocess, "run", side_effect=AssertionError("Unexpected install")):
            root = Path(temp)
            folder = root / "qwen"
            folder.mkdir()
            existing = folder / "user-file.txt"
            existing.write_text("preserve", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "已有内容"):
                deploy.install(root, "qwen", deploy.CATALOG["profiles"]["qwen"])
            self.assertEqual(existing.read_text(encoding="utf-8"), "preserve")


class WireTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.image = Path(self.temp.name) / "sample.png"
        self.image.write_bytes(b"\x89PNG\r\n\x1a\n" + b"sample-public-fixture")

    def tearDown(self):
        self.temp.cleanup()

    def test_each_protocol_encodes_its_own_image_shape(self):
        chat = probe.build_payload("chat", "m", "q", [self.image], 64)
        responses = probe.build_payload("responses", "m", "q", [self.image], 64)
        messages = probe.build_payload("messages", "m", "q", [self.image], 64)
        self.assertTrue(chat["messages"][0]["content"][1]["image_url"]["url"].startswith("data:image/png;base64,"))
        self.assertEqual(responses["input"][0]["content"][1]["type"], "input_image")
        self.assertFalse(responses["store"])
        self.assertEqual(messages["messages"][0]["content"][1]["source"]["media_type"], "image/png")
        self.assertEqual(probe.headers("messages", "test-key")["x-api-key"], "test-key")
        self.assertEqual(probe.headers("messages", "test-key")["anthropic-version"], "2026-10-21")
        self.assertEqual(probe.headers("messages", "test-key", "2023-06-01")["anthropic-version"], "2023-06-01")
        self.assertNotIn("Authorization", probe.headers("messages", "test-key"))

    def test_embedding_contract_rejects_image_and_bad_vectors(self):
        with self.assertRaises(ValueError):
            probe.build_payload("embeddings", "m", "q", [self.image], 64)
        for vector in ([], [0, 0], [float("nan")], [True]):
            with self.assertRaises(ValueError):
                probe.extract("embeddings", {"data": [{"embedding": vector}]})

    def test_vllm_multimodal_uses_one_messages_embedding_for_text_image_or_timed_frames(self):
        query = probe.build_payload("vllm-multimodal", "qwen-vl-embed", "湖边木屋", [], 64,
                                    instruction="Retrieve images or text relevant to the user's query.")
        self.assertEqual(query["messages"][0]["content"][0]["text"], "Retrieve images or text relevant to the user's query.")
        self.assertEqual(query["messages"][1]["content"], [{"type": "text", "text": "湖边木屋"}])
        self.assertTrue(query["add_generation_prompt"])
        self.assertEqual(query["encoding_format"], "float")
        self.assertNotIn("input", query)
        self.assertNotIn("max_tokens", query)
        image = probe.build_payload("vllm-multimodal", "qwen-vl-embed", "", [self.image], 64)
        self.assertEqual(image["messages"][0]["content"][0]["text"], probe.MULTIMODAL_DOCUMENT_INSTRUCTION)
        self.assertTrue(image["messages"][1]["content"][0]["image_url"]["url"].startswith("data:image/png;base64,"))
        video = probe.build_payload("vllm-multimodal", "qwen-vl-embed", "", [self.image, self.image], 64, [1.25, 3.5])
        content = video["messages"][1]["content"]
        self.assertEqual([part["type"] for part in content], ["text", "image_url", "text", "image_url"])
        self.assertIn("1.250", content[0]["text"])
        self.assertIn("3.500", content[2]["text"])
        for times in ([1], [-1, 2], [2, 1], [1, float("nan")], [True, 2]):
            with self.assertRaises(ValueError):
                probe.build_payload("vllm-multimodal", "m", "", [self.image, self.image], 64, times)
        for data in ([], [{"index": 1, "embedding": [1, 0]}], [{"index": 0, "embedding": [1, 0]}] * 2):
            with self.assertRaises(ValueError):
                probe.extract("vllm-multimodal", {"data": data})
        self.assertIn("维度=2", probe.extract("vllm-multimodal", {"data": [{"index": 0, "embedding": [1, 0]}]}))

    def test_endpoint_normalizes_only_base_and_preserves_prefix(self):
        self.assertEqual(probe.endpoint("http://localhost:8000/openai/v1/", "models"), "http://localhost:8000/openai/v1/models")
        self.assertEqual(probe.endpoint("http://localhost:8000", "models"), "http://localhost:8000/v1/models")
        for base in ("https://user:secret@host/v1", "http://host/v1?token=secret", "http://host/v1/chat/completions"):
            with self.assertRaises(ValueError):
                probe.endpoint(base, "models")

    def test_responses_and_messages_ignore_reasoning_blocks(self):
        self.assertEqual(probe.extract("responses", {"output": [{"type": "reasoning"}, {"type": "message", "content": [{"type": "output_text", "text": "answer"}]}]}), "answer")
        self.assertEqual(probe.extract("messages", {"content": [{"type": "thinking", "thinking": "hidden"}, {"type": "text", "text": "answer"}]}), "answer")

    def test_real_loopback_http_round_trips_and_redacts_error_body(self):
        received = []

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b'{"data":[{"id":"m"}]}')

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                received.append((self.path, dict(self.headers), body))
                if body["model"] == "error":
                    self.send_response(401)
                    self.end_headers()
                    self.wfile.write(b'private-test-key and prompt contents')
                    return
                data = {"/v1/chat/completions": {"choices": [{"message": {"content": "chat-ok"}}]},
                        "/v1/responses": {"output": [{"type": "message", "content": [{"type": "output_text", "text": "responses-ok"}]}]},
                        "/v1/messages": {"content": [{"type": "text", "text": "messages-ok"}]},
                        "/v1/embeddings": {"data": [{"index": 0, "embedding": [.1, .2]}]}}[self.path]
                self.send_response(200)
                self.end_headers()
                self.wfile.write(json.dumps(data).encode())

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            base = f"http://127.0.0.1:{server.server_port}/v1"
            self.assertEqual(probe.request_json(base + "/models", "chat", "private-test-key")["data"][0]["id"], "m")
            for protocol, route in probe.ROUTES.items():
                payload = probe.build_payload(protocol, "m", "q", [] if protocol == "embeddings" else [self.image], 64)
                response = probe.request_json(base + "/" + route, protocol, "private-test-key", payload)
                self.assertTrue(probe.extract(protocol, response))
            self.assertEqual(len(received), 5)
            self.assertEqual(received[-1][0], "/v1/embeddings")
            self.assertIn("messages", received[-1][2])
            self.assertNotIn("input", received[-1][2])
            self.assertEqual(received[2][1]["X-Api-Key"], "private-test-key")
            self.assertEqual(received[2][1]["Anthropic-Version"], "2026-10-21")
            with self.assertRaises(ValueError) as error:
                probe.request_json(base + "/chat/completions", "chat", "private-test-key", {"model": "error"})
            self.assertNotIn("private-test-key", str(error.exception))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
