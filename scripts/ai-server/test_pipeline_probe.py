import contextlib
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace

import deploy
import pipeline_probe as pipeline


class EmbeddingDeploymentTests(unittest.TestCase):
    def test_embedding_plan_is_a_pooling_server_not_a_generation_server(self):
        profile = deploy.CATALOG["profiles"]["embedding"]
        command = deploy.serve_command(Path("/env/python"), profile, "/weights", "127.0.0.1", 8002, 8192, .15)
        self.assertEqual(command[command.index("--runner") + 1], "pooling")
        self.assertEqual(command[command.index("--convert") + 1], "embed")
        self.assertEqual(json.loads(command[command.index("--override-pooler-config") + 1]), {"pooling_type": "LAST", "normalize": True})
        self.assertNotIn("--limit-mm-per-prompt", command)
        with tempfile.TemporaryDirectory() as folder, contextlib.redirect_stdout(io.StringIO()) as output:
            args = SimpleNamespace(root=Path(folder) / "not-created", profile="embedding", port=None,
                                   model_path=None, host="127.0.0.1", context=8192, memory=None, dry_run=True)
            deploy.serve(args, profile)
            plan = json.loads(output.getvalue())
            self.assertEqual(plan["protocol"], "OpenAI Embeddings")
            self.assertEqual(plan["servedModel"], "qwen3-embedding-0.6b")
            self.assertEqual(plan["command"][plan["command"].index("--gpu-memory-utilization") + 1], "0.15")
            self.assertFalse(args.root.exists())


class PipelineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.folder = Path(self.temp.name)
        self.image = self.folder / "sample.png"
        # HTTP contract fixture; the real-video test below supplies decoded JPEGs.
        self.image.write_bytes(b"\x89PNG\r\n\x1a\n" + b"contract-fixture")
        self.received = []
        self.bad_citation = False
        self.embedding_error = False
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                owner.received.append((self.path, dict(self.headers), body))
                if self.path == "/vision/v1/chat/completions":
                    if self.headers.get("Authorization") != "Bearer vision-test-only":
                        self.send_error(401)
                        return
                    content = body["messages"][0]["content"]
                    is_image = any(item.get("type") == "image_url" for item in content)
                    value = ({"title": "测试图片", "description": "山峰、湖泊与木屋", "tags": ["湖泊", "木屋"]} if is_image else
                             {"answer": "已有证据记录了湖泊和木屋。", "citations": ["missing" if owner.bad_citation else "sample-1"]})
                    response = {"choices": [{"message": {"content": json.dumps(value, ensure_ascii=False)}}]}
                elif self.path == "/embedding/v1/embeddings":
                    if self.headers.get("Authorization") != "Bearer embedding-test-only":
                        self.send_error(401)
                        return
                    if owner.embedding_error:
                        self.send_response(400)
                        self.end_headers()
                        self.wfile.write(b"private-response-body embedding-test-only")
                        return
                    values = body["input"]
                    response = {"data": list(reversed([{"index": i, "embedding": [1., 0., 0.] if i == 0 else [0., 1., 0.]} for i in range(len(values))]))}
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.end_headers()
                self.wfile.write(json.dumps(response).encode())

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        base = f"http://127.0.0.1:{self.server.server_port}"
        self.vision = {"base": base + "/vision/v1", "model": "mage-vl", "key": "vision-test-only"}
        self.embedding = {"base": base + "/embedding/v1", "model": "qwen3-embedding-0.6b", "key": "embedding-test-only",
                          "dimensions": 3, "instruction": "Retrieve media descriptions relevant to the search query.", "revision": "fixture"}

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        self.temp.cleanup()

    def test_real_http_analysis_embedding_persistence_retrieval_and_answer(self):
        report = pipeline.run_pipeline(self.vision, self.embedding, [self.image, self.image], "湖边木屋")
        self.assertEqual(report["result"], "passed")
        self.assertEqual(report["analysisCount"], 2)
        self.assertEqual(report["hits"][0]["id"], "sample-1")
        self.assertIn("sqlite-roundtrip", report["checks"])
        self.assertEqual(report["lapRuntime"], "not-tested")
        self.assertNotIn("test-only", json.dumps(report))
        requests = [body for url, _, body in self.received if url.endswith("/embeddings")]
        self.assertEqual(requests[0]["model"], "qwen3-embedding-0.6b")
        self.assertFalse(requests[0]["input"][0].startswith("Instruct:"))
        self.assertEqual(requests[1]["input"], ["Instruct: Retrieve media descriptions relevant to the search query.\nQuery:湖边木屋"])
        self.assertEqual(len(self.received), 5)

    def test_query_dimensions_are_checked_against_persisted_documents(self):
        self.embedding["dimensions"] = 1024
        with self.assertRaisesRegex(ValueError, "维度"):
            pipeline.run_pipeline(self.vision, self.embedding, [self.image], "湖边木屋")

    def test_provider_errors_and_invalid_citations_are_not_accepted(self):
        self.embedding_error = True
        with self.assertRaises(ValueError) as error:
            pipeline.run_pipeline(self.vision, self.embedding, [self.image], "湖边木屋")
        self.assertNotIn("private-response-body", str(error.exception))
        self.assertNotIn("embedding-test-only", str(error.exception))
        self.embedding_error = False
        self.bad_citation = True
        with self.assertRaisesRegex(ValueError, "引用"):
            pipeline.run_pipeline(self.vision, self.embedding, [self.image], "湖边木屋")

    def test_schema_and_batch_mapping_are_validated(self):
        for value in ('{"title":"x","description":"","tags":[]}', '{"title":"x","description":"ok","tags":"wrong"}', '{"title":"x\\u007f","description":"ok","tags":[]}', 'not JSON'):
            with self.assertRaises(ValueError):
                pipeline.analysis_record(value)
        for response in (None, [], {"data": "wrong"}):
            with self.assertRaises(ValueError):
                pipeline.embedding_vectors(response, 1)
        for rows in ([{"index": 0, "embedding": [1, 0]}, {"index": 0, "embedding": [0, 1]}],
                     [{"index": 0, "embedding": [1, 0]}],
                     [{"index": 0, "embedding": [True, 0]}, {"index": 1, "embedding": [0, 1]}]):
            with self.assertRaises(ValueError):
                pipeline.embedding_vectors({"data": rows}, 2)
        self.assertEqual(pipeline.embedding_vectors({"data": [{"index": 1, "embedding": [0, 1]}, {"index": 0, "embedding": [1, 0]}]}, 2), [[1., 0.], [0., 1.]])

    @unittest.skipUnless(shutil.which("ffmpeg") and shutil.which("ffprobe"), "Requires FFmpeg")
    def test_real_video_sampling_keeps_source_and_original_timestamps(self):
        video = self.folder / "public-fixture.mp4"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=96x64:rate=8",
                        "-t", "2", "-c:v", "mpeg4", str(video)], check=True, capture_output=True, timeout=30)
        before = hashlib.sha256(video.read_bytes()).hexdigest()
        images, seconds = pipeline.sample_video(video, 3, self.folder)
        self.assertEqual(len(images), 3)
        self.assertTrue(all(path.read_bytes().startswith(b"\xff\xd8\xff") for path in images))
        self.assertTrue(0 < seconds[0] < seconds[1] < seconds[2] < 2.01)
        report = pipeline.run_pipeline(self.vision, self.embedding, images, "视频中有什么？", seconds)
        self.assertEqual(report["analysisCount"], 1)
        self.assertEqual(report["videoFrameCount"], 3)
        self.assertEqual(hashlib.sha256(video.read_bytes()).hexdigest(), before)
        request = self.received[0][2]["messages"][0]["content"]
        self.assertEqual(len([part for part in request if part["type"] == "image_url"]), 3)
        self.assertIn(str(seconds[1]), request[-1]["text"])


if __name__ == "__main__":
    unittest.main()
