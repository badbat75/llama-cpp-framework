-- bench.db: every measurement behind the docs\*.md guides, with what produced it.
-- One run = one model load with one configuration; its results are the requests sent to it.
PRAGMA foreign_keys = ON;

-- The hardware and OS a run ran on; a new driver or kernel is a new row.
CREATE TABLE IF NOT EXISTS machine (
  id            INTEGER PRIMARY KEY,
  fingerprint   TEXT NOT NULL UNIQUE,   -- sha256 of every column below and of the gpu rows
  cpu           TEXT NOT NULL,          -- lscpu "Model name"
  cores         INTEGER NOT NULL,       -- physical
  threads       INTEGER NOT NULL,       -- logical
  ram_mib       INTEGER NOT NULL,       -- MemTotal
  ram_type      TEXT,                   -- DDR5
  ram_speed_mts INTEGER,                -- configured speed
  ram_modules   TEXT,                   -- "2 x 32 GB CMK64GX5M2B6000Z30"
  os            TEXT NOT NULL,
  kernel        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS gpu (
  machine_id    INTEGER NOT NULL REFERENCES machine(id),
  name          TEXT NOT NULL,          -- "AMD Radeon AI PRO R9700"
  vram_mib      INTEGER NOT NULL,
  backend       TEXT NOT NULL,          -- ROCm, CUDA, Vulkan
  driver        TEXT,                   -- NVIDIA driver, amdgpu kernel module
  PRIMARY KEY (machine_id, name, backend)
);

-- The software stack; framework_version is llama-cpp-config's Cargo.toml version.
CREATE TABLE IF NOT EXISTS software (
  id                INTEGER PRIMARY KEY,
  llama_cpp_tag     TEXT NOT NULL,      -- v0.7.0
  llama_cpp_build   INTEGER,            -- 11669
  llama_cpp_commit  TEXT,               -- 620d2225c
  framework_version TEXT,
  rocm              TEXT,               -- TheRock dist version
  cuda              TEXT,               -- CUDA Toolkit dist version
  vulkan_sdk        TEXT,
  patches           TEXT,               -- patches\llama.cpp\ applied, comma separated
  UNIQUE (llama_cpp_tag, llama_cpp_build, llama_cpp_commit, framework_version, rocm, cuda, vulkan_sdk, patches)
);

CREATE TABLE IF NOT EXISTS model (
  id           INTEGER PRIMARY KEY,
  file         TEXT NOT NULL,           -- file name only (first shard of a split model)
  size_bytes   INTEGER NOT NULL,        -- all shards
  name         TEXT NOT NULL,           -- "Qwen3.8-27B"
  arch         TEXT,                    -- general.architecture: qwen35, qwen4exp
  quant        TEXT,                    -- Q8_0, IQ3_S (GSQ-RCO mix), ...
  params_b     REAL,
  layers       INTEGER,
  experts      TEXT,                    -- "512 per layer, 10 used" for a MoE
  mtp          INTEGER NOT NULL DEFAULT 0,
  notes        TEXT,
  UNIQUE (file, size_bytes)
);

CREATE TABLE IF NOT EXISTS prompt (
  id      INTEGER PRIMARY KEY,
  sha256  TEXT NOT NULL UNIQUE,
  name    TEXT NOT NULL,                -- code, prose, math, long-43k, load-2.9k, warm-up
  tokens  INTEGER,                      -- with the chat template, as the server counted them
  text    TEXT NOT NULL
);

-- A question a set of runs answers, as asked.
CREATE TABLE IF NOT EXISTS campaign (
  id        INTEGER PRIMARY KEY,
  name      TEXT NOT NULL UNIQUE,       -- "27b-kv-f16-20261016"
  question  TEXT NOT NULL,
  started   TEXT NOT NULL,              -- ISO date
  answer    TEXT                        -- the conclusion, once drawn
);

CREATE TABLE IF NOT EXISTS run (
  id           INTEGER PRIMARY KEY,
  campaign_id  INTEGER NOT NULL REFERENCES campaign(id),
  label        TEXT NOT NULL,           -- "K1 f16 163840"
  started_utc  TEXT NOT NULL,
  machine_id   INTEGER NOT NULL REFERENCES machine(id),
  software_id  INTEGER NOT NULL REFERENCES software(id),
  model_id     INTEGER NOT NULL REFERENCES model(id),
  tool         TEXT NOT NULL,           -- live (llama-server requests), llama-bench, load-only
  status       TEXT NOT NULL,           -- ok, load-oom, run-oom, crash, interrupted
  error        TEXT,                    -- the first error line, when status is not ok
  notes        TEXT,
  UNIQUE (campaign_id, label)
);

-- Every parameter of the run, not only the varied ones. scope: preset (presets.ini keys),
-- server (server.ini [Server]), env (environment variables), cli (extra flags).
CREATE TABLE IF NOT EXISTS run_param (
  run_id  INTEGER NOT NULL REFERENCES run(id) ON DELETE CASCADE,
  scope   TEXT NOT NULL,
  key     TEXT NOT NULL,
  value   TEXT NOT NULL,
  PRIMARY KEY (run_id, scope, key)
);

-- The GPUs a run computed on: the preset's device list resolved through llama-server
-- --list-devices at record time (every listed device when the preset sets none).
CREATE TABLE IF NOT EXISTS run_gpu (
  run_id   INTEGER NOT NULL REFERENCES run(id) ON DELETE CASCADE,
  device   TEXT NOT NULL,                -- ROCm0, CUDA0, Vulkan2
  gpu      TEXT NOT NULL,                -- AMD Radeon AI PRO R9700
  PRIMARY KEY (run_id, device)
);

-- Memory per device after the load (and the first prefill, when free_mib was read then).
CREATE TABLE IF NOT EXISTS run_device (
  run_id       INTEGER NOT NULL REFERENCES run(id) ON DELETE CASCADE,
  device       TEXT NOT NULL,           -- ROCm0, CUDA0, CPU, CPU_REPACK, ROCm_Host...
  free_mib     INTEGER,
  model_mib    REAL,
  kv_mib       REAL,
  compute_mib  REAL,
  PRIMARY KEY (run_id, device)
);

-- One request (or one llama-bench test) of a run.
CREATE TABLE IF NOT EXISTS result (
  id             INTEGER PRIMARY KEY,
  run_id         INTEGER NOT NULL REFERENCES run(id) ON DELETE CASCADE,
  prompt_id      INTEGER REFERENCES prompt(id),
  rep            INTEGER NOT NULL DEFAULT 1,
  test           TEXT,                  -- llama-bench test name (pp32768, tg2048 @ d768)
  temperature    REAL,
  cache_prompt   INTEGER,               -- 0: the whole prompt was prefilled
  prompt_tokens  INTEGER,
  prefill_tps    REAL,
  gen_tokens     INTEGER,
  decode_tps     REAL,
  draft_gen      INTEGER,
  draft_acc      INTEGER
);

CREATE VIEW IF NOT EXISTS v_result AS
SELECT c.name AS campaign, r.label, r.status, m.name AS model, m.quant,
       s.llama_cpp_tag AS llama_cpp, p.name AS prompt, x.rep, x.prompt_tokens,
       x.prefill_tps, x.gen_tokens, x.decode_tps, x.draft_gen, x.draft_acc, r.started_utc
FROM result x JOIN run r ON r.id = x.run_id JOIN campaign c ON c.id = r.campaign_id
JOIN model m ON m.id = r.model_id JOIN software s ON s.id = r.software_id
LEFT JOIN prompt p ON p.id = x.prompt_id;

-- The parameters of a run as one "key=value; ..." string per scope, for a quick read.
CREATE VIEW IF NOT EXISTS v_run_params AS
SELECT run_id, scope, group_concat(key || '=' || value, '; ') AS params
FROM (SELECT * FROM run_param ORDER BY run_id, scope, key) GROUP BY run_id, scope;
