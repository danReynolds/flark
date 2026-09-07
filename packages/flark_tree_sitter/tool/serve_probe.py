#!/usr/bin/env python3
"""Serve the isolated browser probe and save its synthetic local test receipts."""
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import json
import os

package = Path(__file__).resolve().parent.parent
os.chdir(package)

class Handler(SimpleHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/receipt":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 < length < 100000:
                raise ValueError("receipt length")
            value = json.loads(self.rfile.read(length))
            runtime = value["runtime"]
            if runtime not in ["js", "dart-wasm"]:
                raise ValueError("runtime")
            path = package / "build" / f"worker-browser-{runtime}.json"
            path.write_text(json.dumps(value, indent=2) + "\n")
        except (ValueError, KeyError, TypeError):
            self.send_error(400)
            return
        self.send_response(204)
        self.end_headers()

if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", 8814), Handler).serve_forever()
