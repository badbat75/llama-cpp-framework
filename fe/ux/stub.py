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
# when the bff hides OpencodeApiKey from the reader; list: "one" (today's single
# instance) or "many" (the four tiles of 08-hearth-h6-overview.md section 3).
STATE = {"session": "admin", "inst": "active", "key": "withheld", "list": "one"}

# A long unbroken name: the banner has to hold it at 320 px.
USER = "abcdefghijklmnopqrstuvwxyz012345"

# GET /api/instances with list=many (08-hearth-h6-overview.md section 3): the
# tiles' shapes. One tile per state shape, a second engine so the engine line
# shows, and an id of 40 characters with no break point, which has to break
# inside its tile. The port and the two lists of tags (brief 12) give the rows
# the tile shows: one tile with no port and no tags, and one with the highest
# port and eight tags of 32 characters, the widest facts at 320 px. The `boot`
# of brief 13 (13 N.1) is on for two tiles, off for one, and left out for one,
# the tile with no Start at boot row.
MANY_ID = "ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567890123"
LONG_TAG = "abcdefghijklmnopqrstuvwxyz01234"  # 31 characters
INSTANCES_MANY = [
    {"id": "default", "engine": "llama.cpp", "state": "active", "port": 8080, "tags": ["edge", "v2", "gpu"], "instanceTags": ["v2", "gpu"], "boot": True},
    {"id": "coder", "engine": "llama.cpp", "state": "inactive", "port": 8580, "tags": ["coding", "fast"], "instanceTags": ["coding"], "boot": False},
    {"id": "embed", "engine": "llama.cpp", "state": "failed", "port": 0, "tags": [], "instanceTags": []},
    {"id": MANY_ID, "engine": "strata", "state": "activating", "port": 65535, "tags": [LONG_TAG + str(n) for n in range(8)], "instanceTags": [LONG_TAG + str(n) for n in range(2)], "boot": True},
]

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
        # Brief 14: a profile split over two GPUs, so the GPU distribution card
        # shows its two checked rows with their weight fields, shares and the
        # reorder buttons (GD2.1).
        "split-mode": "tensor",
        "device": "CUDA0,ROCm0",
        "tensor-split": "0.6,0.4",
    },
}

# GET /api/node/devices (brief 14, GD): the engine's probe, in probe order, the
# CPU left out. The names are the long ones a real probe answers with: the
# widest row is the one the check measures at 320 px and at 200 % text, where a
# long id or name must wrap inside its li and ask for no sideways scroll (GD1.12).
DEVICES = {
    "probedAt": "2026-11-20T09:14:03Z",
    "devices": [
        {"id": "CUDA0", "name": "NVIDIA GeForce RTX 5080", "backend": "CUDA", "totalMib": 16304, "freeMib": 15200},
        {"id": "ROCm0", "name": "AMD Radeon AI PRO R9700 / RDNA4", "backend": "ROCm", "totalMib": 32624, "freeMib": 32420},
        {"id": "Vulkan0", "name": "Intel(R) Arc(TM) A770 Graphics", "backend": "Vulkan", "totalMib": 8129, "freeMib": 7900},
    ],
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


# GET /api/node (brief 13, 13 N.3): the host's name, the framework's version,
# and the engines with their backends, the reply of the brief's section 0. The
# name goes in the nav's instance group and in the Overview's h2 (13 N.4,
# 13 N.5), the versions in the About page's lists (13 N.8).
NODE = {
    "name": "gpu-box",
    "api": "/api",
    "version": "2.99.1",
    "engines": [
        {
            "engine": "llama.cpp",
            "version": "1:1.0.0+pai2",
            "backends": [
                {"name": "cuda", "version": "13.5.0+nccl.2.32.3"},
                {"name": "rocm", "version": "10.1.0"},
                {"name": "vulkan", "version": "1.4.363.1"},
            ],
        },
    ],
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
        # The bff from item 13a (fe/briefs/api-contract.md): with no session,
        # every route beside the session one answers 401 session-required, so
        # the signed-out pages can only be the sign-in page.
        if p.startswith("/api/") and p != "/api/session" and STATE["session"] == "out":
            return self.json(401, {"key": "session-required", "args": []})
        if p == "/api/version":
            return self.json(200, {"version": "3.0.0"})
        # The host and its versions (brief 13): the nav's label, the Overview's
        # h2 and the About page all read this one.
        if p == "/api/node":
            return self.json(200, NODE)
        # The host's GPUs, for the model profile page's GPU distribution card
        # (brief 14): the probe the manager caches, with the moment it ran.
        if p == "/api/node/devices":
            return self.json(200, DEVICES)
        if p == "/api/session":
            if STATE["session"] == "out":
                return self.json(401, {"key": "session-required", "args": []})
            return self.json(200, {"user": USER, "canConfigure": STATE["session"] == "admin"})
        if p == "/api/instances":
            if STATE["list"] == "many":
                return self.json(200, INSTANCES_MANY)
            # The one instance of the page the check opens: its port, and tags
            # where the profile brings one the instance does not carry (12), and
            # its boot (13 N.1), on, as its /boot read says.
            return self.json(200, [{"id": "default", "engine": "llama.cpp", "state": STATE["inst"], "port": 8080, "tags": ["edge", "v2", "gpu"], "instanceTags": ["v2", "gpu"], "boot": True}])
        # The boot row of the tiles and the instance page's boot field
        # (08-hearth-h6-overview.md section 3): on for default, off for every
        # other instance.
        parts = p.split("/")
        if len(parts) == 5 and parts[1:3] == ["api", "instances"] and parts[4] == "boot":
            return self.json(200, {"enabled": unquote(parts[3]) == "default"})
        # The instance page of the check is the instance "default".
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
