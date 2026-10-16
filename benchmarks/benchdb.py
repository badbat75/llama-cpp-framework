#!/usr/bin/env python3
"""bench.db writer (Linux): records a campaign of runs with the machine, the software stack,
the model and the full parameter set they ran with.

  benchdb.py init                      create bench.db from schema.sql (idempotent)
  benchdb.py record <campaign.json>    add a campaign and its runs (see README.md)
  benchdb.py machine                   print what would be recorded for this machine

Machine and software are detected here; a run names its preset, and the preset's keys are
read from presets.ini and overlaid with the run's "overrides", so every key is stored, not
only the varied ones. server.ini [Server] is stored the same way.
"""
import configparser, hashlib, json, os, re, sqlite3, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
DB = HERE / 'bench.db'
SECRET = re.compile(r'(?i)(api.?key|secret|password|auth|token$|url$)')
CONFIG = Path(os.environ.get('XDG_CONFIG_HOME') or Path.home() / '.config') / 'llama.cpp'


def sh(*cmd):
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        return r.stdout + r.stderr   # llama-server --version writes to stderr
    except (OSError, subprocess.TimeoutExpired):
        return ''


def connect():
    db = sqlite3.connect(DB)
    db.execute('PRAGMA foreign_keys = ON')
    db.executescript((HERE / 'schema.sql').read_text(encoding='utf-8'))
    return db


def detect_machine():
    lscpu = dict(re.findall(r'^([^:]+):\s*(.*)$', sh('lscpu'), re.M))
    cores = int(lscpu.get('Core(s) per socket', 0)) * int(lscpu.get('Socket(s)', 1))
    mem = int(re.search(r'MemTotal:\s+(\d+)', Path('/proc/meminfo').read_text()).group(1)) // 1024
    dmi = sh('sudo', '-n', 'dmidecode', '-t', 'memory')
    mods = [b for b in dmi.split('\n\n') if 'Memory Device' in b and re.search(r'Size: \d', b)]
    def field(b, k):
        m = re.search(rf'^\s*{k}: (.+)$', b, re.M)
        return m.group(1).strip() if m else None
    speed = field(mods[0], 'Configured Memory Speed') if mods else None
    modules = None
    if mods:
        kinds = {}
        for b in mods:
            k = f"{field(b, 'Size')} {field(b, 'Part Number')}"
            kinds[k] = kinds.get(k, 0) + 1
        modules = ', '.join(f'{n} x {k}' for k, n in kinds.items())
    osr = dict(re.findall(r'^(\w+)="?([^"\n]*)"?$', Path('/etc/os-release').read_text(), re.M))
    kernel = sh('uname', '-r').strip()
    gpus = []
    for line in sh('nvidia-smi', '--query-gpu=name,memory.total,driver_version',
                   '--format=csv,noheader,nounits').strip().splitlines():
        name, vram, drv = [x.strip() for x in line.split(',')]
        gpus.append({'name': name, 'vram_mib': int(vram), 'backend': 'CUDA', 'driver': drv})
    for card in sorted(Path('/sys/class/drm').glob('card[0-9]')):
        dev = card / 'device'
        if (dev / 'vendor').exists() and (dev / 'vendor').read_text().strip() == '0x1002' \
                and (dev / 'mem_info_vram_total').exists():
            vram = int((dev / 'mem_info_vram_total').read_text()) // 2**20
            if vram < 8192:      # the iGPU's carve-out
                continue
            names = [n for n in re.findall(r'Marketing Name:\s*(.+)', sh('rocminfo')) if 'Radeon' in n and 'Graphics' not in n]
            vbios = (dev / 'vbios_version').read_text().strip() if (dev / 'vbios_version').exists() else ''
            gpus.append({'name': names[0].strip() if names else card.name, 'vram_mib': vram,
                         'backend': 'ROCm', 'driver': f'amdgpu (kernel {kernel}), vbios {vbios}'})
    m = {'cpu': lscpu.get('Model name', '?'), 'cores': cores, 'threads': int(lscpu.get('CPU(s)', 0)),
         'ram_mib': mem, 'ram_type': field(mods[0], 'Type') if mods else None,
         'ram_speed_mts': int(speed.split()[0]) if speed and speed[0].isdigit() else None,
         'ram_modules': modules, 'os': osr.get('PRETTY_NAME', '?'), 'kernel': kernel, 'gpus': gpus}
    m['fingerprint'] = hashlib.sha256(json.dumps(m, sort_keys=True).encode()).hexdigest()
    return m


def detect_software():
    v = re.search(r'version: (\S+) \(build (\d+), commit (\w+)\)', sh('llama-server', '--version') +
                  sh(str(REPO / 'build/linux/llama.cpp-cmake/bin/llama-server'), '--version'))
    cache = REPO / 'build/linux/llama.cpp-cmake/CMakeCache.txt'
    cc = cache.read_text(errors='replace') if cache.exists() else ''
    def ver(rx):
        m = re.search(rx, cc)
        return m.group(1) if m else None
    cfg = REPO / 'build/linux/config-build.sh'
    rocm = re.search(r"CFG_HIP_PATH='[^']*/TheRock/([^/]+)/", cfg.read_text()) if cfg.exists() else None
    cargo = re.search(r'^version = "([^"]+)"', (REPO / 'llama-cpp-config/Cargo.toml').read_text(), re.M)
    return {'llama_cpp_tag': 'v' + v.group(1) if v else '?', 'llama_cpp_build': int(v.group(2)) if v else None,
            'llama_cpp_commit': v.group(3) if v else None, 'framework_version': cargo.group(1),
            'rocm': rocm.group(1) if rocm else None, 'cuda': ver(r'CUDAToolkit_ROOT:PATH=.*/CUDA/([^/]+)/'),
            'vulkan_sdk': ver(r'Vulkan_INCLUDE_DIR:PATH=.*/VulkanSDK/([^/]+)/'),
            'patches': ','.join(sorted(p.name for p in (REPO / 'patches/llama.cpp').glob('*.patch')))}


def list_devices():
    """{device id: name} from llama-server --list-devices, the iGPU's Vulkan/ROCm entries included."""
    devs = {}
    for m in re.finditer(r'^\s+(\w+\d+): (.+?) \(\d+ MiB', sh('llama-server', '--list-devices'), re.M):
        devs[m.group(1)] = re.sub(r' \(RADV [^)]*\)', '', m.group(2)).strip()
    return devs


def ini_section(path, section):
    cp = configparser.ConfigParser(interpolation=None, strict=False, comment_prefixes=(';', '#'))
    cp.optionxform = str
    cp.read(path, encoding='utf-8')
    return dict(cp[section]) if cp.has_section(section) else {}


def upsert(db, table, row, key):
    cols = ', '.join(row)
    db.execute(f'INSERT OR IGNORE INTO {table} ({cols}) VALUES ({", ".join("?" * len(row))})', list(row.values()))
    where = ' AND '.join(f'{k} IS ?' for k in key)
    return db.execute(f'SELECT id FROM {table} WHERE {where}', [row[k] for k in key]).fetchone()[0]


def record(path):
    doc = json.loads(Path(path).read_text(encoding='utf-8'))
    db = connect()
    m = detect_machine()
    gpus = m.pop('gpus')
    mid = upsert(db, 'machine', m, ['fingerprint'])
    for g in gpus:
        db.execute('INSERT OR IGNORE INTO gpu VALUES (?, ?, ?, ?, ?)', (mid, g['name'], g['vram_mib'], g['backend'], g['driver']))
    sw = {**detect_software(), **doc.get('software', {})}
    sid = upsert(db, 'software', sw, list(sw))
    md = dict(doc['model'])
    mf = Path(md.pop('path'))
    shards = sorted(mf.parent.glob(re.sub(r'-00001-of-(\d+)', r'-*-of-\1', mf.name))) or [mf]
    md.update(file=mf.name, size_bytes=sum(p.stat().st_size for p in shards))
    modid = upsert(db, 'model', md, ['file', 'size_bytes'])
    pids = {}
    for name, p in doc.get('prompts', {}).items():
        text = Path(p['file']).read_text(encoding='utf-8') if 'file' in p else p['text']
        pids[name] = upsert(db, 'prompt', {'sha256': hashlib.sha256(text.encode()).hexdigest(), 'name': name,
                                           'tokens': p.get('tokens'), 'text': text}, ['sha256'])
    c = doc['campaign']
    cid = upsert(db, 'campaign', {'name': c['name'], 'question': c['question'], 'started': c['started'],
                                  'answer': c.get('answer')}, ['name'])
    preset = ini_section(CONFIG / 'presets.ini', doc['preset'])
    devices = list_devices()
    # bench.db is committed: no credentials and no endpoints of other services
    server = {k: v for k, v in ini_section(CONFIG / 'server.ini', 'Server').items() if not SECRET.search(k)}
    for r in doc['runs']:
        if db.execute('SELECT 1 FROM run WHERE campaign_id = ? AND label = ?', (cid, r['label'])).fetchone():
            print(f"skip (already recorded): {r['label']}")
            continue
        rid = db.execute('INSERT INTO run (campaign_id, label, started_utc, machine_id, software_id, model_id, tool, '
                         'status, error, notes) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
                         (cid, r['label'], r['started_utc'], mid, sid, modid, r.get('tool', 'live'),
                          r.get('status', 'ok'), r.get('error'), r.get('notes'))).lastrowid
        params = {'preset': {**doc.get('base_preset', preset), **doc.get('overrides', {}), **r.get('overrides', {})},
                  'server': {**server, **r.get('server', {})}, 'env': r.get('env', {}), 'cli': r.get('cli', {})}
        for scope, kv in params.items():
            for k, v in kv.items():
                if v is not None and v != 'unset' and not SECRET.search(k):
                    db.execute('INSERT INTO run_param VALUES (?, ?, ?, ?)', (rid, scope, k, str(v)))
        used = [d.strip() for d in params['preset'].get('device', '').split(',') if d.strip()] or list(devices)
        for d in used:
            db.execute('INSERT INTO run_gpu VALUES (?, ?, ?)', (rid, d, devices.get(d, d)))
        for d in r.get('devices', []):
            db.execute('INSERT INTO run_device VALUES (?, ?, ?, ?, ?, ?)', (rid, d['device'], d.get('free_mib'),
                       d.get('model_mib'), d.get('kv_mib'), d.get('compute_mib')))
        for x in r.get('results', []):
            db.execute('INSERT INTO result (run_id, prompt_id, rep, test, temperature, cache_prompt, prompt_tokens, '
                       'prefill_tps, gen_tokens, decode_tps, draft_gen, draft_acc) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
                       (rid, pids.get(x.get('prompt')), x.get('rep', 1), x.get('test'), x.get('temperature'),
                        x.get('cache_prompt'), x.get('prompt_tokens'), x.get('prefill_tps'), x.get('gen_tokens'),
                        x.get('decode_tps'), x.get('draft_gen'), x.get('draft_acc')))
        print(f"recorded: {r['label']}")
    db.commit()


if __name__ == '__main__':
    cmd = sys.argv[1] if len(sys.argv) > 1 else ''
    if cmd == 'init':
        connect().close(); print(DB)
    elif cmd == 'record' and len(sys.argv) == 3:
        record(sys.argv[2])
    elif cmd == 'machine':
        print(json.dumps({**detect_machine(), 'software': detect_software()}, indent=2))
    else:
        sys.exit(__doc__)
