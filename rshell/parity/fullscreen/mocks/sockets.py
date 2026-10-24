#!/usr/bin/python3 -I
"""Line JSON-RPC peers for fixture `sockets` (parity-host's Socket mock semantics).

usage: sockets.py SOCKETS_JSON DIR READY_FILE
For each fixture key K (e.g. "parity.sock") listen on DIR/K. Every line read is a request
{id, method, params}; the reply is {id, result} from `methods`, or {id, error} if absent.
Connections without a request (subscriptions) stay open and silent.
"""
import json
import os
import socket
import sys
import threading


def serve(path, methods):
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(path)
    srv.listen(16)
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=handle, args=(conn, methods), daemon=True).start()


def handle(conn, methods):
    buf = b""
    with conn:
        while True:
            data = conn.recv(65536)
            if not data:
                return
            buf += data
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                try:
                    req = json.loads(line)
                except ValueError:
                    continue
                if req.get("method") in methods:
                    reply = {"id": req.get("id"), "result": methods[req["method"]]}
                else:
                    reply = {"id": req.get("id"), "error": "parity: no canned reply for %s" % req.get("method")}
                conn.sendall((json.dumps(reply) + "\n").encode())


def main():
    spec_path, directory, ready = sys.argv[1:4]
    if not os.path.realpath(directory).startswith("/tmp/claude-1000/"):
        sys.exit("sockets.py: refusing to listen outside the harness runtime dir")
    with open(spec_path, encoding="utf-8") as f:
        spec = json.load(f)
    os.makedirs(directory, exist_ok=True)
    for key, cfg in spec.items():
        threading.Thread(target=serve, args=(os.path.join(directory, key), cfg.get("methods", {})), daemon=True).start()
    with open(ready, "w") as f:
        f.write("ready\n")
    threading.Event().wait()


if __name__ == "__main__":
    main()
