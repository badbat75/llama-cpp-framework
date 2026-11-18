# The stub bff for the UX check (fe/ux/check.js): serves the built web UI and
# the routes the pages read at rest, so the pages reach their shapes without a
# manager, a server.ini or a model. Loopback only.
#
# Usage: python3 stub.py <site-dir> [port]   (the site dir is fe/dist)
#
# The port defaults to 0: the OS picks a free one and the port the server got is
# printed, flushed, as the first stdout line. The caller reads the port there, so
# it never talks to a foreign server already listening on a fixed port, and a
# stub that dies at bind never goes unnoticed.
#
# The answers follow the bff's contract (fe/briefs/api-contract.md). The states
# are set with GET /__set?name=value, and read back as JSON.
import json
import sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlparse

SITE = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 0

# session: "admin" (a signed-in `pai` member), "reader" (signed in, no
# configure), "out" (no session); inst: the instance's state; key: "withheld"
# when the bff hides OpencodeApiKey from the reader.
STATE = {"session": "admin", "inst": "active", "key": "withheld"}

# A long unbroken name: the banner has to hold it at 320 px.
USER = "abcdefghijklmnopqrstuvwxyz012345"

# GET /api/profiles/model: rows with every engine shape the list shows.
MODEL_PROFILES = [
    {"id": "Qwen3.6-35B-A3B-UD-Q4_K_XL", "engine": "llama.cpp", "engineAvailable": True, "enabled": True, "model": ""},
    {"id": "Qwen3-27B", "engine": "llama.cpp", "engineAvailable": True, "enabled": True, "model": "/m/qwen/Qwen3-27B-Q8_0.gguf"},
    {"id": "Swift-Flash-Next-Draft", "engine": "Strata", "engineAvailable": False, "enabled": True, "model": ""},
    {"id": "Old-Coder", "engine": "llama.cpp", "engineAvailable": True, "enabled": False, "model": ""},
]

# GET /api/profiles/model/<id>: the one model profile page that is there. A model
# profile with no values is a real shape: every key of the model profile file is
# unset.
MODEL_PROFILE_DOC = {
    "id": "Qwen3-27B",
    "engine": "llama.cpp",
    "engineAvailable": True,
    "enabled": True,
    "revision": "r1",
    "projectUrl": "https://huggingface.co/unsloth/Qwen3-27B-GGUF",
    "values": {
        "model": "/m/qwen/Qwen3-27B-Q8_0.gguf",
        "mmproj": "/m/qwen/mmproj-F16.gguf",
        "model-draft": "/m/qwen/Qwen3-0.6B-Q8_0.gguf",
        "spec-type": "ngram-mod,draft-mtp",
        "ctx": "4096",
    },
}

# GET /api/instances/default/server: the server document, as the bff gives it.
SERVER_DOC = {
    "revision": "r1",
    "values": {
        "Port": 8580, "Hostname": "localhost", "OpencodeBaseUrl": None, "OpencodeApiKey": None,
        "Threads": None, "ThreadsBatch": 8, "ModelsMax": None, "ModelsMemoryMargin": 512, "ModelsDir": None,
        "Webui": True, "WebuiMcpProxy": False, "LogVerbosity": 4, "SaveStateOnShutdown": False, "StateDir": None,
        "RocblasUseHipblaslt": None, "RocprofAttach": False, "CudaAllreduce": None, "CudaDisableGraphs": False,
        "AttnRotDisable": False,
    },
    "defaults": {
        "Port": 9931, "ModelsMax": 4, "ModelsMemoryMargin": 0, "OpencodeBaseUrl": "http://localhost:8580",
        "ModelsDir": "/var/lib/llama.cpp/models", "StateDir": "/var/lib/llama.cpp/state",
    },
    "limits": {
        "Port": [1, 65535], "ModelsMax": [0, 2147483647], "ModelsMemoryMargin": [0, 2147483647],
        "Threads": [1, 32], "ThreadsBatch": [1, 32],
    },
    "interfaces": [{"value": "192.168.1.20", "label": "192.168.1.20 (enp5s0)"}],
}


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=SITE, **kwargs)

    def log_message(self, *args):
        pass

    def json(self, code, body):
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        url = urlparse(self.path)
        p = url.path
        if p == "/__set":
            for k, v in parse_qs(url.query).items():
                STATE[k] = v[0]
            return self.json(200, STATE)
        if p == "/api/version":
            return self.json(200, {"version": "3.0.0"})
        if p == "/api/session":
            if STATE["session"] == "out":
                return self.json(401, {"key": "session-required", "args": []})
            return self.json(200, {"user": USER, "canConfigure": STATE["session"] == "admin"})
        if p == "/api/instances":
            return self.json(200, [{"id": "default", "engine": "llama.cpp", "state": STATE["inst"]}])
        # The instance page of the check is the instance "default".
        if p == "/api/instances/default/boot":
            return self.json(200, {"enabled": False})
        if p == "/api/instances/default/server":
            doc = dict(SERVER_DOC)
            if STATE["key"] == "withheld":
                doc["withheld"] = ["OpencodeApiKey"]
            return self.json(200, doc)
        if p == "/api/profiles/model":
            return self.json(200, MODEL_PROFILES)
        if p.startswith("/api/profiles/model/") and p.count("/") == 4:
            pid = unquote(p.split("/")[4])
            if pid == MODEL_PROFILE_DOC["id"]:
                return self.json(200, MODEL_PROFILE_DOC)
            return self.json(404, {"key": "no-model-profile-named", "args": [pid]})
        return super().do_GET()


# The constructor binds and listens, so the port printed is one a request on it
# reaches; a bind failure (a port in use) exits here with its traceback.
SERVER = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
print(SERVER.server_address[1], flush=True)
SERVER.serve_forever()
