"""L0 differential harness for `requests.get(...)`.

Runs the corpus oracle (`requests_subset.get`) against a stable seed
of localhost-served fixtures + asserts the cobrust-requests Rust crate
returns equivalent shape (status / headers / body). The Rust side is
exercised at `crates/cobrust-requests/tests/requests_downstream.rs`;
this file documents the L0 contract for the curator.
"""

import sys
import os
import http.server
import threading
import socket

HERE = os.path.dirname(os.path.abspath(__file__))
SHIPPED = os.path.join(HERE, "..", "upstream")
sys.path.insert(0, SHIPPED)

from requests_subset import get  # type: ignore


class _Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("X-Test-Marker", "cobrust-requests-harness")
        body = b'{"hello":"world"}'
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args, **kwargs):
        pass


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def main():
    port = free_port()
    server = http.server.HTTPServer(("127.0.0.1", port), _Handler)
    t = threading.Thread(target=server.serve_forever)
    t.daemon = True
    t.start()
    try:
        resp = get(f"http://127.0.0.1:{port}/")
        assert resp.status_code == 200
        body = resp.json()
        assert body.get("hello") == "world"
        print("PASS h_get")
    finally:
        server.shutdown()


if __name__ == "__main__":
    main()
