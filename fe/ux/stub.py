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
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlparse

SITE = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 0

# session: "admin" (a signed-in `pai` member), "reader" (signed in, no
# configure), "out" (no session); inst: the instance's state; key: "withheld"
# when the bff hides OpencodeApiKey from the reader; list: "one" (today's single
# instance) or "many" (the four tiles of 08-hearth-h6-overview.md section 3);
# tags: "few" (the three of brief 12) or "long" (the widest the rule allows, the
# 8 tags of 32 characters of brief 15 section 7); load: "fast" (every read
# answers at once, every state beside the two loading ones), "slow" (the page's
# reads are held, the session answering: the page shows the skeleton of its
# first read) or "session" (only the session read is held: the boot skeleton);
# node: "ok" (the host reply of brief 18 section 0), "down" (the manager did not
# answer: `os`, `vulkan` and `gpus` are null and `nodeInfo` says
# client-unavailable) or "long" (32 GPUs of 256 unbroken characters, a host name
# of 253, the widest the node page takes).
STATE = {
    "session": "admin",
    "inst": "active",
    "key": "withheld",
    "list": "one",
    "tags": "few",
    # Brief 16 17a: `pool=long` adds the widest row to the server profile pool,
    # for the pool list's state.
    "pool": "short",
    "load": "fast",
    "node": "ok",
    # The GPU distribution card's split mode (brief 14 GD1.2, brief 19 section 7):
    # `tensor` as the document carries it, `layer` with the key removed, the file's
    # mode where the block controls show.
    "split": "tensor",
    # Brief 21 (21 T.2 to 21 T.9): `ot=1` puts the state's three rules in the
    # profile, so the Tensor placement card shows its rows and their legends.
    "ot": "none",
    # Brief 22 slice IP5 (IP5.6, the state `model-profile-delete-tip` of
    # check.js): `used=1` reads the model profile listed by the `default` instance
    # profile, where its Delete is switched off.
    "used": "0",
    # Brief 22 slice IP5 review (the state `model-profiles-narrow` of check.js):
    # `usedby=long` lists each model by the widest instance profile ids the pool
    # takes, so the check reads the Used by cell where an id has to break.
    "usedby": "short",
    # Brief 16 slice 17c (17c.6a, the state `instance-remove` of check.js):
    # `removable=1` says the instance's record can be removed, where the page
    # offers the remove verb.
    "removable": "0",
    # Brief 26, 26.3, as the IP6a review reads it (the state
    # `instance-profiles-in-use` of check.js): `instuse=race` refuses the DELETE
    # of `coder`, the profile the list says no instance runs, with the bff's 409
    # `instance-profile-in-use`: the page raced an instance taking it.
    "instuse": "free",
}

# How long a held read stays open, for the loading states of brief 17 U17a:
# long past the 300 ms the skeleton needs, and long past the axe run, so the
# page is the same page from the marker to the last reading.
HOLD_S = 8.0

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
#
# The `serverProfile` of brief 20 (20 N.5, section 6) rides the same rows, for
# the nav's instance links: `default` names one, `coder` answers `null`, `embed`
# carries no key at all (the reader makes both `null`), and the 40-character id
# carries a 40-character profile with no break point, so the open nav at 320 px
# and at 200 % text has to break both inside one pill.
MANY_ID = "ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567890123"
LONG_SERVER_PROFILE = "QWEN327BTHIRTYCHARACTERSPROFILE012345678"  # 40, no break point
LONG_LABEL = "QWEN327BTHIRTYCHARACTERSTILELABEL0123456789012345678901234567890"  # 64, no break point
LONG_TAG = "abcdefghijklmnopqrstuvwxyz01234"  # 31 characters
INSTANCES_MANY = [
    {"id": "default", "engine": "llama.cpp", "state": "active", "port": 8080, "tags": ["edge", "v2", "gpu"], "instanceTags": ["v2", "gpu"], "boot": True, "serverProfile": "qwen-27b", "label": "Coding assistant"},
    {"id": "coder", "engine": "llama.cpp", "state": "inactive", "port": 8580, "tags": ["coding", "fast"], "instanceTags": ["coding"], "boot": False, "serverProfile": None, "label": None},
    {"id": "embed", "engine": "llama.cpp", "state": "failed", "port": 0, "tags": [], "instanceTags": [], "label": None},
    {"id": MANY_ID, "engine": "strata", "state": "activating", "port": 65535, "tags": [LONG_TAG + str(n) for n in range(8)], "instanceTags": [LONG_TAG + str(n) for n in range(2)], "boot": True, "serverProfile": LONG_SERVER_PROFILE, "label": LONG_LABEL},
]

# The tags of the one instance the check opens (brief 15 section 7), as
# `tags=` picks them. Each tuple is the pair the list item carries: the union
# `tags` and the instance's own `instanceTags` (section 0). "few" is the shape
# of brief 12 (HEAD's fixture); "long" is the widest the rule allows
# (section 10 R3), both lists at its cap: the instance owns 8 tags of 32
# characters, and its server profile brings 8 of its own the instance does not
# carry, so the union is 16. The check then renders the 8 removable chips and
# the 8 fixed ones of `.chips-profile` (fe/ux/check.js).
LONG_OWN = [LONG_TAG + str(n) for n in range(8)]  # 32 characters, TAG_MAX
LONG_PROFILE = [LONG_TAG[:30] + "p" + str(n) for n in range(8)]  # 32 as well
OWN_TAGS = {
    "few": (["edge", "v2", "gpu"], ["v2", "gpu"]),
    "long": (LONG_OWN + LONG_PROFILE, LONG_OWN),
}

# GET /api/profiles/server (brief 15 section 6, slice C2): the pool the new-tag
# pill of the instance page reads once at each opening, for the suggestions
# under the box. One row, carrying the instance's own tags and labels it does
# not carry, so the pill has suggestions to show at 320 px and at 200 % text.
# Brief 26 slice IP6b (26.6, the state `instance-profile` of check.js): a second
# row, so an instance profile that runs one of them has another to switch to and
# the section's field is on screen.
SERVER_PROFILES = [
    {
        "id": "default",
        "engine": "llama.cpp",
        "port": 8080,
        "tags": [
            "edge",
            "v2",
            "gpu",
            "prod",
            "coding",
            "fast",
            "arm",
            "zz-top",
            "t0",
        ],
        "instance": "default",
    },
    {"id": "fast", "engine": "llama.cpp", "port": 8081, "tags": [], "instance": None},
]

# Brief 16, slice 17a: the pool list's page (`pool=long`, the state
# `server-profiles` of fe/ux/check.js). The widest row the rule allows: a
# 32-character id (ID_MAX) with the 8 tags of 32 characters the tags states
# use, run by no instance. The other states keep the pool above, so the
# new-tag pill's suggestions do not change.
LONG_POOL_ROW = {
    "id": LONG_TAG + "x",  # 32 characters, the cap of an id
    "engine": "llama.cpp",
    "port": 9931,
    "tags": LONG_OWN,
    "instance": None,
}

# GET /api/profiles/instance (brief 26 section 0, the states
# `instance-profiles` and `instance-profiles-new` of check.js): three profiles,
# one of them used by the `default` instance so its row's trash can is switched
# off, one with two models and one with none. Brief 26 slice IP6b (26.7, the
# states `instance-profile`, `instance-profile-add` and `instance-profile-remove`
# of check.js): `coder` carries a third model, and the writes below change this
# list in place, so a check that presses a verb sees the row move.
INSTANCE_PROFILES = [
    {"id": "default", "server": "default", "models": ["Qwen3-27B"], "usedBy": ["default"]},
    {"id": "coder", "server": "coder", "models": ["Qwen3.6-35B-A3B-UD-Q4_K_XL", "Qwen3-27B", "Swift-Flash-Next-Draft"], "usedBy": []},
    {"id": "office", "server": "default", "models": [], "usedBy": []},
]

# The placement keys of each entry of each profile (brief 26, 26.7): the page's
# read counts an entry's keys, so the Placement column says
# `instance-profile-placement-default` for 0 and `instance-profile-placement-count`
# for the rest. `coder` carries all three shapes.
PROFILE_ENTRIES = {
    "default": {"Qwen3-27B": 4},
    "coder": {
        "Qwen3.6-35B-A3B-UD-Q4_K_XL": 0,
        "Qwen3-27B": 3,
        "Swift-Flash-Next-Draft": 1,
    },
    "office": {},
}

# GET /api/profiles/model: rows with every engine shape the list shows. Brief 22
# slice IP5 (IP5.0): a row carries no `enabled` and no `assignments`, and carries
# `usedBy`, the instance profiles that list the model in their file order; the
# row's Delete can act when it is empty (IP5.3). Two rows are used, by one
# instance profile and by two, and two are free.
MODEL_PROFILES = [
    {"id": "Qwen3.6-35B-A3B-UD-Q4_K_XL", "engine": "llama.cpp", "engineAvailable": True, "usedBy": ["default"], "model": ""},
    {"id": "Qwen3-27B", "engine": "llama.cpp", "engineAvailable": True, "usedBy": ["default", "coder"], "model": "/m/qwen/Qwen3-27B-Q8_0.gguf"},
    {"id": "Swift-Flash-Next-Draft", "engine": "Strata", "engineAvailable": False, "usedBy": ["default", "coder"], "model": ""},
    {"id": "Old-Coder", "engine": "llama.cpp", "engineAvailable": True, "usedBy": [], "model": ""},
]

# GET /api/profiles/model with `usedby=long` (brief 22, slice IP5 review A5, the
# state `model-profiles-narrow` of check.js): the same rows listed by instance
# profile ids at the 32 character cap of an id, each carrying its break points.
# The Used by cell of these is the narrowest the id reaches, so the check reads
# where an id breaks.
USED_BY_LONG = ["office-gpu-alpha-node-0123", "private-ai-coder-profile-01"]
MODEL_PROFILES_USED_BY_LONG = [
    dict(MODEL_PROFILES[0], usedBy=[USED_BY_LONG[0]]),
    dict(MODEL_PROFILES[1], usedBy=list(USED_BY_LONG)),
    dict(MODEL_PROFILES[2], usedBy=[USED_BY_LONG[1]]),
    MODEL_PROFILES[3],
]

# GET /api/node/model-files (brief 22, slice 22C1, 22 C.10a, the state
# `model-profiles-new` of check.js): the folder the New model profile form's
# Model file picker reads. The fixture carries the 40-character path 22 C.10a
# names (the file sitting in the folder itself, `category` null), a model in a
# subfolder, and a split model listed once as its first shard (4 shards), so the
# option's text, its size and its parts line are all read at 320 px.
MODELS_DIR = "/var/lib/llama.cpp/models"
MODEL_FILES = {
    "source": "local",
    "dir": MODELS_DIR,
    "files": [
        {
            "path": f"{MODELS_DIR}/qwen3-27b.gguf",  # 40 characters (22 C.10a)
            "size": 17_400_000_000,
            "shards": 1,
            "category": None,
        },
        {
            "path": f"{MODELS_DIR}/qwen/qwen2.5-coder-7b-q5_k_m.gguf",
            "size": 5_300_000_000,
            "shards": 1,
            "category": "qwen",
        },
        {
            "path": f"{MODELS_DIR}/deepseek/DeepSeek-R1-Q4_K_M.gguf",
            "size": 5_000_000_000,
            "shards": 4,
            "category": "deepseek",
        },
    ],
    "truncated": False,
    "error": None,
}

# GET /api/hf/files?repo=<id> (brief 22, slice 22C2, 22 C.10a, the state
# `model-profiles-new-hf` of check.js): the quants the New model profile form's
# Hugging Face half lists. 12 quants, one of them split in 3 parts (22 C.10a),
# one llama.cpp's default with a vision projector beside it, one under 0.1 GiB
# (the ` MiB` spelling) and one carrying a `hf-file`, so the radio's text, the
# size and the parts line are all read at 320 px and at 200 % text.
HF_REPO = "ggml-org/gemma-3-1b-it-GGUF"
HF_OID = "a" * 40


def hf_quant(tag, size, shards=1, default=False, mmproj=None, hf_file=None):
    value = {"hf-repo": f"{HF_REPO}:{tag}"}
    if hf_file:
        value["hf-file"] = hf_file
    return {
        "tag": tag,
        "file": f"gemma-3-1b-it-{tag}.gguf",
        "shards": shards,
        "size": size,
        "sha256": HF_OID,
        "sidecars": {"mmproj": mmproj},
        "value": value,
        "default": default,
    }


HF_FILES = {
    "source": "hf",
    "repo": HF_REPO,
    "revision": HF_OID,
    "files": [
        hf_quant("Q2_K", 52_943_360),
        hf_quant("Q3_K_S", 1_100_000_000),
        hf_quant("Q3_K_M", 1_300_000_000),
        hf_quant("Q4_K_S", 2_200_000_000),
        hf_quant(
            "Q4_K_M",
            2_400_000_000,
            default=True,
            mmproj="mmproj-F16.gguf",
        ),
        hf_quant("Q4_K_XL", 2_600_000_000),
        hf_quant("Q5_K_S", 2_700_000_000),
        hf_quant("Q5_K_M", 2_900_000_000),
        hf_quant("Q6_K", 3_400_000_000),
        hf_quant("Q8_0", 4_100_000_000),
        hf_quant("F16", 5_300_000_000, shards=3),
        hf_quant(
            "BF16",
            5_300_000_000,
            hf_file="gemma-3-1b-it-BF16-00001-of-00002.gguf",
        ),
    ],
}

# GET /api/instances/<id>/profiles/model (brief 16, slice 17e, the state
# `instance-models` of check.js): the pool's rows with the words of the
# instance's record. The list's own switch is on (`all` inherit), so one profile
# on in the list follows it, one off in the list is served by an explicit on,
# and one on in the list is refused by an explicit off: the three shapes the
# section's rows show.
INSTANCE_MODELS = {
    "all": "inherit",
    "profiles": [
        dict(MODEL_PROFILES[1], value="inherit", viaAll=True, effective=True),
        dict(MODEL_PROFILES[3], value="on", viaAll=False, effective=True),
        dict(MODEL_PROFILES[0], value="off", viaAll=False, effective=False),
    ],
}

# GET /api/profiles/model/<id>: the one model profile page that is there. A model
# profile with no values is a real shape: every key of the model profile file is
# unset.
MODEL_PROFILE_DOC = {
    "id": "Qwen3-27B",
    "engine": "llama.cpp",
    "engineAvailable": True,
    # Brief 22 slice IP5 (IP5.0): the document carries `usedBy`, and the page
    # reads no `enabled`: the switch is gone (IP5.4), so the stub sends none and
    # every state proves the page works without it. The two states of `usedBy`
    # are the shape the Delete verb reads: the default is a profile no instance
    # profile lists, whose Save bar offers its Delete (IP5.6, 22 D4.10);
    # `used=1` (below) is the profile an instance profile lists, whose Delete is
    # switched off.
    "usedBy": [],
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

# Brief 21 (fe/briefs/21-tensor-placement.md section 7): the rules of the
# `model-profile-placement` state, `ot=1`: the experts of a run of layers on the
# CPU, the embedding table on a GPU, and a hand-written pattern no kind owns, so
# the card shows three rows, their legends, and a Custom row with its Pattern.
OT_RULES = (
    r"blk\.(20|21|22|23)\.ffn_(up|down|gate|gate_up)_(ch|)exps=CPU,"
    r"^token_embd\.weight=ROCm0,"
    r"blk\.1[0-9]\..*_exps=CPU"
)

# GET /api/profiles/model/<id>/model-info (brief 19, section 7): the facts of the
# profile's model file, read: 64 blocks and a Mixture of Experts with 8 of 128
# experts used, so the GPU card is in blocks mode and the MoE pair is enabled.
# 21 T.17a (the TP3 check): the model's tensors ride along, so the placement
# state shows a coverage line under each of its three rules: 64 blocks, each
# with its three expert tensors and its attention, plus the embedding table and
# the output head. The embedding is 1.21 GiB and an expert tensor 1 MiB, so the
# one and the many texts both show, at 320 px and at 200 % text.
MODEL_TENSORS = (
    [{"name": "token_embd.weight", "bytes": 1299227607}]
    + [
        {"name": f"blk.{b}.ffn_{p}_exps.weight", "bytes": 1 << 20}
        for b in range(64)
        for p in ("up", "down", "gate")
    ]
    + [{"name": f"blk.{b}.attn_q.weight", "bytes": 1 << 20} for b in range(64)]
    + [{"name": "output.weight", "bytes": 1 << 29}]
)

MODEL_INFO = {
    "model": "/m/qwen/Qwen3-27B-Q8_0.gguf",
    "info": {
        "blockCount": 64,
        "moe": True,
        "expertCount": 128,
        "expertUsed": 8,
        "tensors": MODEL_TENSORS,
        "tensorsTruncated": False,
    },
    "reason": None,
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
        "RocblasUseHipblaslt": None, "RocmHostMemoryGtt": True, "RocprofAttach": False, "CudaAllreduce": None, "CudaDisableGraphs": False,
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


# The host's operating system (brief 18 section 0), the same block in both
# replies: the About page's row (18 AN.3) and the node page's top (18 AN.8).
OS = {"name": "Ubuntu", "version": "26.04", "kernel": "7.0.0-43-generic"}

# GET /api/version (brief 18, AN4 and AN.15): the framework's version, the
# host's name (the nav group's label and the Overview's h2) and the operating
# system. No manager: 401 or 200.
VERSION = {"version": "3.0.0", "name": "gpu-box", "os": OS}

# The three GPUs of brief 18 section 7: the AMD card with its Vulkan driver,
# the NVIDIA card with its proprietary driver, and the Intel Arc whose
# in-kernel `xe` driver sets no module version (`version` null, so its version
# is the kernel's) and which Vulkan does not see (`vulkan` null with the node
# answering "ok").
GPUS = [
    {
        "pci": "0000:03:00.0",
        "vendor": "1002",
        "name": "AMD Radeon AI PRO R9700",
        "driver": "amdgpu",
        "version": "7.1.9.31600000",
        "vulkan": {"driver": "radv", "info": "Mesa 25.2.0"},
    },
    {
        "pci": "0000:0a:00.0",
        "vendor": "10de",
        "name": "NVIDIA GeForce RTX 5090",
        "driver": "nvidia",
        "version": "580.95.05",
        "vulkan": {"driver": "NVIDIA", "info": "580.95.05"},
    },
    {
        "pci": "0000:0f:00.0",
        "vendor": "8086",
        "name": "Intel Arc B580",
        "driver": "xe",
        "version": None,
        "vulkan": None,
    },
]

# GET /api/node (brief 13, 13 N.3, and brief 18 section 0): the host's name,
# the framework's version, the engines with their backends, the operating
# system, the Vulkan answer and the GPUs. The node page reads this one (18
# AN.8); the shell and the About page read GET /api/version (18 AN.15, AN.2).
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
    "os": OS,
    "vulkan": "ok",
    "gpus": GPUS,
}

# node=down: the manager did not answer within 10 s. The reply is still 200,
# `os`, `vulkan` and `gpus` are null and `nodeInfo` says why (brief 18 section
# 0), so the node page shows its host and engines and AN.13's notice.
NODE_DOWN = {
    **NODE,
    "os": None,
    "vulkan": None,
    "gpus": None,
    "nodeInfo": {"key": "client-unavailable", "args": []},
}

# node=long: the widest the page takes, 32 GPU rows, every text of a row 256
# unbroken characters (no break point), and a host name of 253, the length
# brief 10 R1.8 measures. The ids stay real, so the rows are 32 separate
# blocks of text that has to wrap inside them.
HEX = "0123456789abcdef"
LONG_TEXT = HEX * 16  # 256 characters, none of them a space
LONG_GPUS = [
    {
        "pci": f"0000:{index:02x}:00.0",
        "vendor": HEX * 16,
        "name": LONG_TEXT,
        "driver": LONG_TEXT,
        "version": LONG_TEXT,
        "vulkan": {"driver": LONG_TEXT, "info": LONG_TEXT},
    }
    for index in range(32)
]
NODE_LONG = {**NODE, "name": LONG_TEXT[:253], "gpus": LONG_GPUS}

NODE_BY_STATE = {"down": NODE_DOWN, "long": NODE_LONG}


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
        # The loading states of brief 17 U17a: `load=slow` holds every read of
        # the page, so the page is the skeleton of its first read;
        # `load=session` holds only the session one, so the page is the boot
        # skeleton. Every other state answers at once.
        if p.startswith("/api/"):
            page_read = p != "/api/session"
            held = (STATE["load"] == "slow" and page_read) or (
                STATE["load"] == "session" and not page_read
            )
            if held:
                time.sleep(HOLD_S)
        # The bff from item 13a (fe/briefs/api-contract.md): with no session,
        # every route beside the session one answers 401 session-required, so
        # the signed-out pages can only be the sign-in page.
        if p.startswith("/api/") and p != "/api/session" and STATE["session"] == "out":
            return self.json(401, {"key": "session-required", "args": []})
        if p == "/api/version":
            return self.json(200, VERSION)
        # The host and its GPUs (brief 13 and brief 18): only the node page
        # reads this one (18 AN.15), and `node=` sets its shape.
        if p == "/api/node":
            return self.json(200, NODE_BY_STATE.get(STATE["node"], NODE))
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
            # The one instance of the page the check opens: its port, its tags
            # (brief 15, `tags=`) where the profile brings one the instance does
            # not carry (12), its boot (13 N.1), on, as its /boot read says, and
            # its server profile (brief 20 section 6). 16 17f.12: no label, so
            # the name the page's heading, the nav's link and the tile show is
            # `instance-name-auto` with the engine and the port, "llama.cpp :9931",
            # and the nav says nothing beside it (17f.2). 16 17c.6a: `removable=1`
            # says the record can be removed, where the page offers the verb.
            tags, profile = OWN_TAGS[STATE["tags"]]
            return self.json(200, [{"id": "default", "engine": "llama.cpp", "state": STATE["inst"], "port": 9931, "tags": tags, "instanceTags": profile, "boot": True, "serverProfile": "qwen-27b", "label": None, "removable": STATE["removable"] == "1"}])
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
        # Brief 16 slice 17e (the state `instance-models` of check.js): the model
        # profiles the instance serves, with the words of its record.
        if p == "/api/instances/default/profiles/model":
            return self.json(200, INSTANCE_MODELS)
        if p == "/api/profiles/server":
            # Brief 16 17a: the pool list's page (`pool=long`) gains the widest
            # row the rule allows; every other state sees today's pool, so the
            # new-tag pill's suggestions are unchanged.
            if STATE["pool"] == "long":
                return self.json(200, SERVER_PROFILES + [LONG_POOL_ROW])
            return self.json(200, SERVER_PROFILES)
        # Brief 26 section 0 (the states `instance-profiles` and
        # `instance-profiles-new` of check.js): the instance profile pool, in
        # file order. The check opens the create form and reads it; it never
        # presses Create, so the pool never changes.
        if p == "/api/profiles/instance":
            return self.json(200, {"rows": INSTANCE_PROFILES})
        # Brief 26 slice IP6b (fe/briefs/api-contract.md IP4, the states
        # `instance-profile`, `instance-profile-add` and `instance-profile-remove`
        # of check.js): one profile's page. The body is `{id, server, models:
        # [{id, keys}], usedBy, revision}`, read off the pool above and
        # PROFILE_ENTRIES; `revision` is a constant, as only the entry's save
        # sends one (IP4's "Revisions"). `/api/profiles/instance/<id>` has four
        # slashes; the model routes below have six.
        if p.startswith("/api/profiles/instance/") and p.count("/") == 4:
            pid = unquote(p.split("/")[4])
            row = next((r for r in INSTANCE_PROFILES if r["id"] == pid), None)
            if row is None:
                return self.json(
                    404, {"key": "no-instance-profile-named", "args": [pid]}
                )
            keys = PROFILE_ENTRIES.get(pid, {})
            return self.json(
                200,
                {
                    "id": row["id"],
                    "server": row["server"],
                    "models": [
                        {"id": m, "keys": keys.get(m, 0)} for m in row["models"]
                    ],
                    "usedBy": row["usedBy"],
                    "revision": "rev-" + row["id"],
                },
            )
        # Brief 16 slice 17a: a server profile's own page. The document is
        # today's server document plus `instance`, the instance that runs it.
        # `/api/profiles/server/<id>` has four slashes (the model profile route
        # above is the same shape), and the refusal key is the bff's
        # `client-no-server-profile` (fe/briefs/api-contract.md), the one the
        # page maps to server-profile-not-found.
        if p.startswith("/api/profiles/server/") and p.count("/") == 4:
            pid = unquote(p.split("/")[4])
            if pid == "default":
                doc = dict(SERVER_DOC)
                doc["instance"] = "default"
                if STATE["key"] == "withheld":
                    doc["withheld"] = ["OpencodeApiKey"]
                return self.json(200, doc)
            # Brief 16 slice 17b (the state `server-profile-delete` of check.js):
            # the widest profile of the `pool=long` list, the page the Confirm in
            # place opens on. The list says it runs nowhere, so the document
            # answers with `instance` null, as 17b.6 asks it to.
            if pid == LONG_TAG + "x":
                doc = dict(SERVER_DOC)
                doc["instance"] = None
                return self.json(200, doc)
            return self.json(404, {"key": "client-no-server-profile", "args": [pid]})
        if p == "/api/profiles/model":
            return self.json(
                200,
                MODEL_PROFILES_USED_BY_LONG
                if STATE["usedby"] == "long"
                else MODEL_PROFILES,
            )
        # Brief 22 slice 22C1 (22 C.10a, the state `model-profiles-new` of
        # check.js): the folder the Model file picker reads.
        if p == "/api/node/model-files":
            return self.json(200, MODEL_FILES)
        # Brief 22 slice 22C2 (22 C.10a, the state `model-profiles-new-hf` of
        # check.js): the quants of the repo the Hugging Face half reads. The
        # stub answers the repo the fixture spells, whatever the page asks for,
        # so the state needs no query of its own.
        if p == "/api/hf/files":
            return self.json(200, HF_FILES)
        # The facts of a model file (brief 19, section 0): the block counts and the
        # experts, read by the manager from the GGUF.
        if p.startswith("/api/profiles/model/") and p.count("/") == 5:
            pid = unquote(p.split("/")[4])
            if p.split("/")[5] == "model-info":
                if pid == MODEL_PROFILE_DOC["id"]:
                    return self.json(200, MODEL_INFO)
                return self.json(404, {"key": "no-model-profile-named", "args": [pid]})
        if p.startswith("/api/profiles/model/") and p.count("/") == 4:
            pid = unquote(p.split("/")[4])
            if pid == MODEL_PROFILE_DOC["id"]:
                doc = dict(MODEL_PROFILE_DOC)
                if STATE["split"] == "layer":
                    # The file's mode: `split-mode` absent, and a ratio of 40 and
                    # 25 the facts read projects to block counts (19 G.4).
                    values = {
                        k: v for k, v in doc["values"].items() if k != "split-mode"
                    }
                    values["tensor-split"] = "40,25"
                    doc["values"] = values
                if STATE["ot"] == "1":
                    # Brief 21: the Tensor placement state's three rules.
                    values = dict(doc["values"])
                    values["override-tensor"] = OT_RULES
                    doc["values"] = values
                if STATE["used"] == "1":
                    # Brief 22 slice IP5 (IP5.6): the profile an instance profile
                    # lists, the page whose Delete is switched off.
                    doc["usedBy"] = ["default"]
                return self.json(200, doc)
            return self.json(404, {"key": "no-model-profile-named", "args": [pid]})
        return super().do_GET()

    def do_PUT(self):
        p = urlparse(self.path).path
        # The body is drained even where the route is not served, so a write
        # never sits in a socket nobody reads.
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        # Brief 16 slice 17f, as its 17f2 moves the form to the heading (17f2.10,
        # the state `instance-name-edit` of check.js): the display name form
        # saves the label here, and the bff's reply is 204. The check opens the
        # form and reads it; it never presses Save, so the answer is a plain 204.
        if p.startswith("/api/instances/") and p.count("/") == 4 and p.endswith("/label"):
            self.send_response(204)
            self.end_headers()
            return
        # Brief 16 slice 17a (17a.7): the Server form of a profile's page saves
        # to this address, and the bff's reply is the file's revision and whether
        # the instance that runs it is up. The check has no manager and starts
        # nothing, so `running` is false; the save bar then reads save-saved.
        if p.startswith("/api/profiles/server/") and p.count("/") == 4:
            pid = unquote(p.split("/")[4])
            if pid == "default":
                return self.json(
                    200, {"revision": SERVER_DOC["revision"], "running": False}
                )
            return self.json(404, {"key": "client-no-server-profile", "args": [pid]})
        # Brief 16 slice 17e (the state `instance-models` of check.js): the
        # section's two writes. The check reads the section and never presses a
        # control, so the answers are the plain ones: the pool-wide write says
        # nothing needs a restart, and a row write answers the row it was asked
        # about (fe/briefs/api-contract.md 878).
        if p.startswith("/api/instances/") and p.endswith("/profiles/model"):
            return self.json(200, {"running": False})
        if p.startswith("/api/instances/") and p.count("/") == 6:
            parts = p.split("/")
            if parts[4:6] == ["profiles", "model"]:
                pid = unquote(parts[6])
                for row in INSTANCE_MODELS["profiles"]:
                    if row["id"] == pid:
                        return self.json(
                            200, {"effective": dict(row), "running": False}
                        )
                return self.json(
                    404, {"key": "no-model-profile-named", "args": [pid]}
                )
        # Brief 22 slice IP5 (IP5.0): the switch route is gone. A write to it
        # answers 404, as the bff does: the model profile is no longer switched
        # on or off, it is used by an instance profile or not.
        if p.startswith("/api/profiles/model/") and p.endswith("/enabled"):
            return self.json(404, {"key": "client-no-route", "args": [p]})
        # Brief 26 slice IP6b (26.6, fe/briefs/api-contract.md IP4, the state
        # `instance-profile` of check.js): the section's switch. The body is
        # `{"server"}`, with no revision, and the answer is the affected answer.
        # The profile an id does not name is the manager's 404, and a server
        # profile the pool has not got is 422 with `field` server (IP4's table).
        # The check opens the field and reads it; it never presses the verb, so
        # the pool stays: `running` is empty, as the stub starts nothing.
        if p.startswith("/api/profiles/instance/") and p.endswith("/server"):
            parts = p.split("/")
            pid = unquote(parts[4])
            row = next((r for r in INSTANCE_PROFILES if r["id"] == pid), None)
            if row is None:
                return self.json(
                    404, {"key": "no-instance-profile-named", "args": [pid]}
                )
            try:
                server = json.loads(body)["server"]
            except (ValueError, KeyError):
                return self.json(400, {"key": "invalid-request", "args": []})
            if not any(s["id"] == server for s in SERVER_PROFILES):
                return self.json(
                    422,
                    {"key": "no-server-profile-named", "args": [server], "field": "server"},
                )
            row["server"] = server
            return self.json(200, {"running": [], "notConverged": []})
        return self.send_error(404, "no PUT of " + p)

    def do_POST(self):
        p = urlparse(self.path).path
        # The body is drained even where the route is not served, so a write
        # never sits in a socket nobody reads.
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        # Brief 16 slice 17b (17b.4, the state `server-profiles-new` of
        # check.js): the list page creates a profile here, and the bff's reply
        # is 201 with the new id. The check opens the form and reads it; it
        # never presses Create, so the answer is a plain 201.
        if p == "/api/profiles/server":
            try:
                new_id = json.loads(body)["id"]
            except (ValueError, KeyError):
                return self.json(400, {"key": "server-profile-malformed", "args": []})
            return self.json(201, {"id": new_id})
        # Brief 26 section 0 (the state `instance-profiles-new` of check.js):
        # the list page creates an instance profile here, and the bff's reply
        # is 201 with the new id. The check opens the form and reads it; it
        # never presses Create, so the answer is a plain 201.
        if p == "/api/profiles/instance":
            try:
                new_id = json.loads(body)["id"]
            except (ValueError, KeyError):
                return self.json(400, {"key": "invalid-instance-profile-id", "args": []})
            return self.json(201, {"id": new_id})
        # Brief 16 slice 17c (17c.4, the state `instances-new` of check.js): the
        # Overview's list creates an instance here, and the bff's reply is 201
        # with the new id. The check opens the form and reads it; it never
        # presses Create, so the answer is a plain 201.
        if p == "/api/instances":
            try:
                new_id = json.loads(body)["id"]
            except (ValueError, KeyError):
                return self.json(400, {"key": "instance-bad-id", "args": [""]})
            return self.json(201, {"id": new_id})
        # Brief 22 slice 22C1 (22 C.5, the state `model-profiles-new` of
        # check.js): the Models list creates a model profile here, and the bff's
        # reply is 201 with the new id. The check opens the form and reads it; it
        # never presses Create, so the answer is a plain 201.
        if p == "/api/profiles/model":
            try:
                new_id = json.loads(body)["id"]
            except (ValueError, KeyError):
                return self.json(400, {"key": "model-profiles-malformed", "args": []})
            return self.json(201, {"id": new_id})
        # Brief 26 slice IP6b (26.8, fe/briefs/api-contract.md IP4, the state
        # `instance-profile-add` of check.js): the section's add. The body is
        # `{"model"}`, the answer is 201 with the affected answer. The model
        # the pool has not got is the manager's 422 with `field` model, one
        # already listed is its 409. The check opens the form and reads it, and
        # the states list no running instance for `office`, so `running` is
        # empty: the stub starts nothing.
        if p.startswith("/api/profiles/instance/") and p.endswith("/models"):
            pid = unquote(p.split("/")[4])
            row = next((r for r in INSTANCE_PROFILES if r["id"] == pid), None)
            if row is None:
                return self.json(
                    404, {"key": "no-instance-profile-named", "args": [pid]}
                )
            try:
                model = json.loads(body)["model"]
            except (ValueError, KeyError):
                return self.json(400, {"key": "invalid-request", "args": []})
            if not any(m["id"] == model for m in MODEL_PROFILES):
                return self.json(
                    422,
                    {"key": "no-model-profile-named", "args": [model], "field": "model"},
                )
            if model in row["models"]:
                return self.json(
                    409, {"key": "instance-profile-model-listed", "args": []}
                )
            row["models"].append(model)
            return self.json(201, {"running": [], "notConverged": []})
        return self.send_error(404, "no POST of " + p)

    def do_DELETE(self):
        p = urlparse(self.path).path
        # Brief 16 slice 17b (17b.8, the state `server-profile-delete` of
        # check.js): a profile's page deletes the file here, and the bff's
        # reply is 204. The check opens the confirm and reads it; it never
        # presses the verb.
        if p.startswith("/api/profiles/server/") and p.count("/") == 4:
            pid = unquote(p.split("/")[4])
            if pid == "default" or pid == LONG_TAG + "x":
                self.send_response(204)
                self.end_headers()
                return
            return self.json(404, {"key": "client-no-server-profile", "args": [pid]})
        # Brief 26 slice IP6b (26.8, fe/briefs/api-contract.md IP4, the state
        # `instance-profile-remove` of check.js): a row's minus removes the
        # entry here. `/api/profiles/instance/<p>/models/<m>` has six slashes,
        # so it is read before the profile's own delete below. The entry the
        # profile does not list is the manager's 404 `no-instance-profile-model`.
        # The check opens the confirm and reads it; it never presses the verb,
        # so the pool stays.
        if (
            p.startswith("/api/profiles/instance/")
            and p.count("/") == 6
            and p.split("/")[5] == "models"
        ):
            pid = unquote(p.split("/")[4])
            model = unquote(p.split("/")[6])
            row = next((r for r in INSTANCE_PROFILES if r["id"] == pid), None)
            if row is None:
                return self.json(
                    404, {"key": "no-instance-profile-named", "args": [pid]}
                )
            if model not in row["models"]:
                return self.json(
                    404, {"key": "no-instance-profile-model", "args": []}
                )
            row["models"].remove(model)
            return self.json(200, {"running": [], "notConverged": []})
        # Brief 26 section 0 (the state `instance-profiles` of check.js): the
        # list page's trash can deletes a profile here, and the bff's reply is
        # 204. The check opens the confirm and reads it; it never presses the
        # verb, so the pool stays. The profile an instance uses refuses with
        # the bff's 409 (brief 26 26.3), as the page shows it.
        if p.startswith("/api/profiles/instance/") and p.count("/") == 4:
            pid = unquote(p.split("/")[4])
            # The bff's refusals are `{key, args}` (fe/briefs/api-contract.md),
            # the body api.ts reads: any other shape is the generic sentence.
            if pid == "default":
                return self.json(
                    409, {"key": "instance-profile-in-use", "args": ["default"]}
                )
            if pid == "coder" and STATE["instuse"] == "race":
                # The state of 26.3's race: the list says no instance runs the
                # profile, and the delete answers that an instance took it.
                return self.json(
                    409, {"key": "instance-profile-in-use", "args": [pid]}
                )
            if pid in [row["id"] for row in INSTANCE_PROFILES]:
                self.send_response(204)
                self.end_headers()
                return
            return self.json(
                404, {"key": "no-instance-profile-named", "args": [pid]}
            )
        # Brief 22 slice 22D (22 D.3, the state `model-profile-delete` of
        # check.js): a model profile's page deletes it here, and the bff's reply
        # is 204. The check opens the confirm and reads it; it never presses the
        # verb, so the profile stays.
        if p.startswith("/api/profiles/model/") and p.count("/") == 4:
            pid = unquote(p.split("/")[4])
            if pid == MODEL_PROFILE_DOC["id"]:
                self.send_response(204)
                self.end_headers()
                return
            return self.json(404, {"key": "no-model-profile-named", "args": [pid]})
        # Brief 16 slice 17c (17c.7a, the state `instance-remove` of check.js):
        # the instance page removes the record here, and the bff's reply is 204.
        # The check opens the confirm and reads it; it never presses the verb,
        # so the instance stays.
        if p.startswith("/api/instances/") and p.count("/") == 4:
            if unquote(p.split("/")[4]) == "default":
                self.send_response(204)
                self.end_headers()
                return
            return self.json(404, {"key": "client-no-instance", "args": []})
        return self.send_error(404, "no DELETE of " + p)


# The constructor binds and listens, so the port printed is one a request on it
# reaches; a bind failure (a port in use) exits here with its traceback.
SERVER = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
print(SERVER.server_address[1], flush=True)
SERVER.serve_forever()
