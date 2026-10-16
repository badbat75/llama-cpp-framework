#!/usr/bin/env python3
"""bench.db queries (any OS: only the standard library's sqlite3).

  benchq.py campaigns                   the campaigns: question, answer, runs
  benchq.py models                      the models tested, with runs and campaigns
  benchq.py machines                    the machines: CPU, RAM, GPUs, OS
  benchq.py prompts                     the prompts, with their token counts
  benchq.py runs                        one line per run (--show adds parameter columns)
  benchq.py summary                     one line per run: prefill and decode on the long prompt, decode on the short ones
  benchq.py results                     one line per request
  benchq.py run <id>                    everything about one run
  benchq.py diff <id> <id>              the parameters that differ between two runs
  benchq.py sql "<SELECT ...>"          any query (read-only)

Filters (runs, summary, results; campaigns and models take the ones that apply):
  --campaign TEXT   campaign name contains TEXT        --model TEXT    model name, file or quant contains TEXT
  --gpu TEXT        the run computed on a GPU named like TEXT (r9700, 4070)
  --device TEXT     the preset's device contains TEXT (ROCm0, Vulkan)
  --llama TAG       llama.cpp tag (v0.7.0)              --status S      ok, load-oom, run-oom, crash, interrupted
  --param K=V       a parameter equals V (K~V: contains V); repeatable, any scope
  --prompt NAME     results only: one prompt (long-43k, code, prose, math, warm-up)
  --since DATE      runs started on or after DATE
Output: --format table (default), csv or json; --show K1,K2 adds parameter columns to runs and summary;
--full keeps long text cells whole.
"""
import argparse, csv, json, sqlite3, sys
from pathlib import Path

DB = Path(__file__).resolve().parent / 'bench.db'
SHORT = ('code', 'prose', 'math')


def connect():
    if not DB.exists():
        sys.exit(f'{DB} not found')
    db = sqlite3.connect(f'file:{DB}?mode=ro', uri=True)
    db.row_factory = sqlite3.Row
    return db


def where(a, alias='r'):
    """WHERE clause and arguments for the run filters."""
    w, args = [], []
    if getattr(a, 'run_id', None):
        w.append(f'{alias}.id = ?'); args.append(a.run_id)
    if a.campaign:
        w.append(f'{alias}.campaign_id IN (SELECT id FROM campaign WHERE name LIKE ?)'); args.append(f'%{a.campaign}%')
    if a.model:
        w.append(f'{alias}.model_id IN (SELECT id FROM model WHERE name LIKE ? OR file LIKE ? OR quant LIKE ?)')
        args += [f'%{a.model}%'] * 3
    if a.gpu:
        w.append(f'{alias}.id IN (SELECT run_id FROM run_gpu WHERE gpu LIKE ?)'); args.append(f'%{a.gpu}%')
    if a.device:
        w.append(f"{alias}.id IN (SELECT run_id FROM run_param WHERE scope = 'preset' AND key = 'device' AND value LIKE ?)")
        args.append(f'%{a.device}%')
    if a.llama:
        w.append(f'{alias}.software_id IN (SELECT id FROM software WHERE llama_cpp_tag = ?)'); args.append(a.llama)
    if a.status:
        w.append(f'{alias}.status = ?'); args.append(a.status)
    if a.since:
        w.append(f'{alias}.started_utc >= ?'); args.append(a.since)
    for p in a.param or []:
        k, op, v = (p.partition('~') if '~' in p else p.partition('='))
        if not op:
            sys.exit(f'--param {p}: expected K=V or K~V')
        w.append(f"{alias}.id IN (SELECT run_id FROM run_param WHERE key = ? AND value {'LIKE' if op == '~' else '='} ?)")
        args += [k, f'%{v}%' if op == '~' else v]
    return (' WHERE ' + ' AND '.join(w)) if w else '', args


def params(db, run_id, keys):
    rows = db.execute(f"SELECT key, value FROM run_param WHERE run_id = ? AND key IN ({','.join('?' * len(keys))})",
                      [run_id, *keys]).fetchall()
    d = {r['key']: r['value'] for r in rows}
    return [d.get(k, '') for k in keys]


def out(a, head, rows):
    rows = [['' if v is None else (f'{v:.1f}' if isinstance(v, float) else str(v)) for v in r] for r in rows]
    if a.format == 'json':
        print(json.dumps([dict(zip(head, r)) for r in rows], indent=1, ensure_ascii=False)); return
    if a.format == 'csv':
        w = csv.writer(sys.stdout); w.writerow(head); w.writerows(rows); return
    if not a.full:
        rows = [[c if len(c) <= 70 else c[:67] + '...' for c in r] for r in rows]
    width = [max(len(str(x)) for x in col) for col in zip(head, *rows)] if rows else [len(h) for h in head]
    num = [all(c.replace('.', '', 1).lstrip('-').isdigit() or c == '' for c in col) for col in zip(*rows)] if rows else []
    line = lambda r: '  '.join((c.rjust(w) if num and num[i] else c.ljust(w)) for i, (c, w) in enumerate(zip(r, width))).rstrip()
    print(line(head)); print('  '.join('-' * w for w in width))
    for r in rows:
        print(line(r))
    print(f'({len(rows)} rows)')


def cmd_campaigns(db, a):
    w, args = where(a)
    q = f'''SELECT c.name, c.started, count(r.id) AS runs, sum(r.status = 'ok') AS ok, c.question, c.answer
            FROM campaign c JOIN run r ON r.campaign_id = c.id {w} GROUP BY c.id ORDER BY c.started, c.name'''
    out(a, ['campaign', 'started', 'runs', 'ok', 'question', 'answer'], db.execute(q, args).fetchall())


def cmd_models(db, a):
    w, args = where(a)
    q = f'''SELECT m.name, m.quant, m.arch, m.mtp, round(m.size_bytes / 1073741824.0, 2) AS gib, m.file,
                   count(r.id) AS runs, count(DISTINCT r.campaign_id) AS campaigns
            FROM model m JOIN run r ON r.model_id = m.id {w} GROUP BY m.id ORDER BY m.name, m.quant'''
    out(a, ['model', 'quant', 'arch', 'mtp', 'GiB', 'file', 'runs', 'campaigns'], db.execute(q, args).fetchall())


def cmd_machines(db, a):
    rows = []
    for m in db.execute('SELECT * FROM machine ORDER BY id'):
        gpus = '; '.join(f"{g['name']} {g['vram_mib']} MiB ({g['backend']} {g['driver'] or ''})".strip()
                         for g in db.execute('SELECT * FROM gpu WHERE machine_id = ? ORDER BY name', (m['id'],)))
        ram = f"{round(m['ram_mib'] / 1024)} GiB" + (f" {m['ram_type'] or ''} {m['ram_speed_mts'] or ''} MT/s" if m['ram_speed_mts'] else '')
        runs = db.execute('SELECT count(*) FROM run WHERE machine_id = ?', (m['id'],)).fetchone()[0]
        rows.append([m['id'], m['cpu'], f"{m['cores']}c/{m['threads']}t", ram, m['ram_modules'], gpus, f"{m['os']}, {m['kernel']}", runs])
    out(a, ['id', 'cpu', 'cores', 'ram', 'modules', 'gpus', 'os', 'runs'], rows)


def cmd_prompts(db, a):
    q = '''SELECT p.id, p.name, p.tokens, length(p.text) AS chars, count(DISTINCT x.run_id) AS runs, substr(p.text, 1, 60) AS start
           FROM prompt p LEFT JOIN result x ON x.prompt_id = p.id GROUP BY p.id ORDER BY p.id'''
    out(a, ['id', 'prompt', 'tokens', 'chars', 'runs', 'text (start)'], db.execute(q).fetchall())


def run_rows(db, a):
    w, args = where(a)
    return db.execute(f'''SELECT r.id, c.name AS campaign, r.label, m.name AS model, m.quant, s.llama_cpp_tag AS llama,
                                 r.status, r.started_utc, r.notes, r.error
                          FROM run r JOIN campaign c ON c.id = r.campaign_id JOIN model m ON m.id = r.model_id
                          JOIN software s ON s.id = r.software_id {w} ORDER BY c.started, c.name, r.id''', args).fetchall()


def cmd_runs(db, a):
    keys = a.show.split(',') if a.show else []
    rows = [[r['id'], r['campaign'], r['label'], f"{r['model']} {r['quant']}", r['llama'], r['status'], *params(db, r['id'], keys),
             r['error'] or r['notes']] for r in run_rows(db, a)]
    out(a, ['id', 'campaign', 'label', 'model', 'llama.cpp', 'status', *keys, 'error / notes'], rows)


def cmd_summary(db, a):
    keys = a.show.split(',') if a.show else []
    rows = []
    for r in run_rows(db, a):
        x = db.execute(f'''SELECT
               avg(CASE WHEN p.name LIKE 'long%' THEN x.prefill_tps END) AS lp,
               avg(CASE WHEN p.name LIKE 'long%' THEN x.decode_tps END) AS ld,
               avg(CASE WHEN p.name IN ({','.join('?' * len(SHORT))}) THEN x.decode_tps END) AS sd,
               sum(CASE WHEN p.name IN ({','.join('?' * len(SHORT))}) THEN x.draft_acc END) AS acc,
               sum(CASE WHEN p.name IN ({','.join('?' * len(SHORT))}) THEN x.draft_gen END) AS gen
             FROM result x LEFT JOIN prompt p ON p.id = x.prompt_id WHERE x.run_id = ?''', [*SHORT, *SHORT, *SHORT, r['id']]).fetchone()
        free = '; '.join(f"{d['device']} {d['free_mib']}" for d in
                         db.execute('SELECT device, free_mib FROM run_device WHERE run_id = ? AND free_mib IS NOT NULL ORDER BY device', (r['id'],)))
        acc = f"{100 * x['acc'] / x['gen']:.0f}%" if x['gen'] else ''
        rows.append([r['id'], r['campaign'], r['label'], r['status'], *params(db, r['id'], keys),
                     x['lp'], x['ld'], x['sd'], acc, free])
    out(a, ['id', 'campaign', 'label', 'status', *keys, 'prefill long', 'decode long', 'decode short', 'draft acc', 'free MiB'], rows)


def cmd_results(db, a):
    w, args = where(a)
    if a.prompt:
        w += (' AND ' if w else ' WHERE ') + 'p.name = ?'; args.append(a.prompt)
    q = f'''SELECT r.id, c.name, r.label, p.name AS prompt, x.rep, x.prompt_tokens, x.prefill_tps, x.gen_tokens, x.decode_tps,
                   x.draft_gen, x.draft_acc
            FROM result x JOIN run r ON r.id = x.run_id JOIN campaign c ON c.id = r.campaign_id
            LEFT JOIN prompt p ON p.id = x.prompt_id {w} ORDER BY c.started, c.name, r.id, x.id'''
    out(a, ['run', 'campaign', 'label', 'prompt', 'rep', 'prompt tok', 'prefill t/s', 'gen tok', 'decode t/s', 'drafts', 'accepted'],
        db.execute(q, args).fetchall())


def cmd_run(db, a):
    r = db.execute('''SELECT r.*, c.name AS campaign, c.question, c.answer FROM run r JOIN campaign c ON c.id = r.campaign_id
                      WHERE r.id = ?''', (a.id,)).fetchone()
    if not r:
        sys.exit(f'no run {a.id}')
    m = db.execute('SELECT * FROM machine WHERE id = ?', (r['machine_id'],)).fetchone()
    s = db.execute('SELECT * FROM software WHERE id = ?', (r['software_id'],)).fetchone()
    mo = db.execute('SELECT * FROM model WHERE id = ?', (r['model_id'],)).fetchone()
    if a.format == 'json':
        doc = {'run': dict(r), 'machine': dict(m), 'gpus': [dict(g) for g in db.execute('SELECT * FROM gpu WHERE machine_id = ?', (m['id'],))],
               'software': dict(s), 'model': dict(mo),
               'params': {}, 'devices': [dict(d) for d in db.execute('SELECT * FROM run_device WHERE run_id = ?', (a.id,))],
               'results': [dict(x) for x in db.execute('SELECT * FROM result WHERE run_id = ?', (a.id,))]}
        for p in db.execute('SELECT * FROM run_param WHERE run_id = ? ORDER BY scope, key', (a.id,)):
            doc['params'].setdefault(p['scope'], {})[p['key']] = p['value']
        print(json.dumps(doc, indent=1, ensure_ascii=False)); return
    print(f"run {r['id']}: {r['label']}  [{r['status']}]  {r['started_utc']}")
    print(f"campaign: {r['campaign']}\n  question: {r['question']}\n  answer: {r['answer'] or ''}")
    if r['error'] or r['notes']:
        print(f"error: {r['error'] or ''}\nnotes: {r['notes'] or ''}")
    print(f"model: {mo['name']} {mo['quant']} ({mo['arch']}, {mo['size_bytes'] / 2**30:.2f} GiB{', MTP' if mo['mtp'] else ''}) {mo['file']}")
    print(f"llama.cpp {s['llama_cpp_tag']} (build {s['llama_cpp_build']}, {s['llama_cpp_commit']}), framework {s['framework_version']}, "
          f"ROCm {s['rocm']}, CUDA {s['cuda']}, Vulkan SDK {s['vulkan_sdk']}, patches {s['patches'] or 'none'}")
    print(f"machine: {m['cpu']} {m['cores']}c/{m['threads']}t, {round(m['ram_mib'] / 1024)} GiB {m['ram_type'] or ''} "
          f"{m['ram_speed_mts'] or ''} MT/s, {m['os']}, {m['kernel']}")
    for g in db.execute('SELECT * FROM gpu WHERE machine_id = ? ORDER BY name', (m['id'],)):
        print(f"  gpu: {g['name']}, {g['vram_mib']} MiB, {g['backend']}, {g['driver']}")
    scope = None
    for p in db.execute('SELECT * FROM run_param WHERE run_id = ? ORDER BY scope, key', (a.id,)):
        if p['scope'] != scope:
            scope = p['scope']; print(f'{scope}:')
        print(f"  {p['key']} = {p['value']}")
    devs = db.execute('SELECT * FROM run_device WHERE run_id = ? ORDER BY device', (a.id,)).fetchall()
    if devs:
        print('memory (MiB):')
        for d in devs:
            print('  ' + d['device'] + ': ' + ', '.join(f'{k} {d[k]}' for k in ('free_mib', 'model_mib', 'kv_mib', 'compute_mib') if d[k] is not None))
    a.full = True
    cmd_results(db, argparse.Namespace(**{**vars(a), 'campaign': None, 'model': None, 'gpu': None, 'device': None, 'llama': None,
                                          'status': None, 'since': None, 'prompt': None, 'param': None, 'run_id': a.id}))


def cmd_diff(db, a):
    get = lambda i: {(p['scope'], p['key']): p['value'] for p in db.execute('SELECT * FROM run_param WHERE run_id = ?', (i,))}
    x, y = get(a.a), get(a.b)
    rows = [[s, k, x.get((s, k), '(unset)'), y.get((s, k), '(unset)')] for s, k in sorted(set(x) | set(y))
            if x.get((s, k)) != y.get((s, k))]
    for i in (a.a, a.b):
        r = db.execute('SELECT label FROM run WHERE id = ?', (i,)).fetchone()
        print(f"run {i}: {r['label'] if r else '(none)'}")
    out(a, ['scope', 'key', f'run {a.a}', f'run {a.b}'], rows)


def cmd_sql(db, a):
    try:
        cur = db.execute(a.query)
    except sqlite3.Error as e:
        sys.exit(f'sql: {e}')
    out(a, [d[0] for d in cur.description or []], cur.fetchall())


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter, add_help=True)
    sub = p.add_subparsers(dest='cmd', required=True)
    def add(name, fn, filters=True):
        s = sub.add_parser(name)
        s.set_defaults(fn=fn)
        s.add_argument('--format', choices=['table', 'csv', 'json'], default='table')
        s.add_argument('--full', action='store_true')
        if filters:
            for f in ('campaign', 'model', 'gpu', 'device', 'llama', 'status', 'since', 'prompt', 'show'):
                s.add_argument(f'--{f}')
            s.add_argument('--param', action='append')
        return s
    for name, fn in (('campaigns', cmd_campaigns), ('models', cmd_models), ('runs', cmd_runs),
                     ('summary', cmd_summary), ('results', cmd_results)):
        add(name, fn)
    add('machines', cmd_machines, False); add('prompts', cmd_prompts, False)
    add('run', cmd_run).add_argument('id', type=int)
    d = add('diff', cmd_diff, False); d.add_argument('a', type=int); d.add_argument('b', type=int)
    add('sql', cmd_sql, False).add_argument('query')
    a = p.parse_args()
    db = connect()
    a.fn(db, a)


if __name__ == '__main__':
    main()
