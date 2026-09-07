#!/usr/bin/env python3
"""Serve a local web build with fresh asset URLs, preserving draft storage.

Run after `flutter build web --wasm`; restart after each rebuild.
The generated build stays untouched. This is a loopback preview, not hosting.
"""

import hashlib
import io
import json
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

BUILD = Path(__file__).resolve().parents[1] / "build" / "web"
digest = hashlib.sha256()
for path in sorted(p for p in BUILD.rglob("*") if p.is_file()):
    digest.update(path.relative_to(BUILD).as_posix().encode())
    digest.update(hashlib.sha256(path.read_bytes()).digest())
PREFIX = f"/_flark/{digest.hexdigest()[:16]}/"


class Preview(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(BUILD), **kwargs)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def send_head(self):
        if self.path.startswith(PREFIX):
            self.path = "/" + self.path[len(PREFIX):]
        route = self.path.split("?", 1)[0]
        if route in ("/", "/index.html"):
            body = (BUILD / "index.html").read_text().replace(
                'src="flutter_bootstrap.js"',
                f'src="{PREFIX}flutter_bootstrap.js"',
            )
            mime = "text/html"
        elif route == "/flutter_bootstrap.js":
            bootstrap = (BUILD / "flutter_bootstrap.js").read_text()
            marker = "_flutter.loader.load("
            assert marker in bootstrap, "Flutter bootstrap format changed"
            config = {"entrypointBaseUrl": PREFIX, "assetBase": PREFIX}
            body = bootstrap.rsplit(marker, 1)[0] + marker + json.dumps(
                {"config": config}
            ) + ");\n"
            mime = "application/javascript"
        else:
            return super().send_head()
        payload = body.encode()
        self.send_response(200)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        return io.BytesIO(payload)


if __name__ == "__main__":
    assert (BUILD / "main.dart.wasm").is_file(), "Build with --wasm first"
    print(f"Flark preview: http://127.0.0.1:8813/ ({PREFIX})", flush=True)
    ThreadingHTTPServer(("127.0.0.1", 8813), Preview).serve_forever()
