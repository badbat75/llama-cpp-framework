// The Server form of an instance (fe/briefs/01-server.md, sections 3 and 5):
// the cards and fields of pai-config's Server tab, in its order, each control
// showing the file's value. Read-only for a user; for an administrator the
// controls follow their dependencies (5.3) and the save bar checks the values,
// sends them typed (5.4) and shows the answer by its key.
import { useEffect, useId, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { apiError, call, isRecord } from './api.ts'
import Card from './Card.tsx'
import Dialog, { DiscardDialog } from './Dialog.tsx'
import {
  CheckboxField,
  CheckListField,
  DefaultBoxField,
  SecretField,
  SelectField,
  TextField,
} from './Field.tsx'
import type { Session } from './session.ts'
import SignIn from './SignIn.tsx'
import { t } from './strings.ts'

/** Values by INI key name, typed JSON. */
type Keys = Record<string, unknown>

/** The server document of GET /api/instances/{id}/server, as far as the form reads it. */
export type ServerDoc = {
  revision?: unknown
  values: Keys
  defaults: Keys
  limits?: unknown
  interfaces: { value: string; label: string }[]
  /** The keys the bff did not send to this session (2.8). */
  withheld?: unknown
}

/** Whether the bff withheld the API key from this session (27a). */
export function withholdsKey(doc: ServerDoc): boolean {
  return Array.isArray(doc.withheld) && doc.withheld.includes('OpencodeApiKey')
}

/** llama-server's log levels, by number (data, not text). */
const LOG_LEVELS = [
  'OUTPUT:0',
  'ERROR:1',
  'WARN:2',
  'INFO:3',
  'TRACE:4',
  'DEBUG:5',
]

/** A text input behind a default, auto or No key box. */
type Boxed = { ticked: boolean; text: string }

/** What the controls hold, by INI key; Hostname is the rows' checked state. */
type Draft = {
  Port: Boxed
  Hostname: boolean[]
  OpencodeBaseUrl: Boxed
  OpencodeApiKey: Boxed
  Threads: Boxed
  ThreadsBatch: Boxed
  ModelsMax: Boxed
  ModelsMemoryMargin: Boxed
  ModelsDir: string
  Webui: boolean
  WebuiMcpProxy: boolean
  LogVerbosity: string
  SaveStateOnShutdown: boolean
  StateDir: string
  RocblasUseHipblaslt: string
  RocprofAttach: boolean
  CudaAllreduce: string
  CudaDisableGraphs: boolean
  AttnRotDisable: boolean
}

type Key = keyof Draft

/** The Bind to rows; localhost is always the first, 0.0.0.0 the second. */
type Row = { value: string; text: string }
const LOCALHOST = 0
const ALL = 1

/** The switch that enables a dependent field (5.3). */
const SWITCH_OF: Partial<Record<Key, Key>> = {
  WebuiMcpProxy: 'Webui',
  StateDir: 'SaveStateOnShutdown',
}

/** The text of a value from the file: a number or a string, else nothing. */
function shown(value: unknown): string {
  return typeof value === 'number' || typeof value === 'string'
    ? String(value)
    : ''
}

function unset(value: unknown): boolean {
  return value === null || value === undefined
}

/** A key with a default box: unset shows the default the API gave. */
function withDefault(value: unknown, fallback: unknown): Boxed {
  const ticked = unset(value)
  return { ticked, text: shown(ticked ? fallback : value) }
}

/** A thread count or the API key: unset (or no key) is ticked, with an empty input. */
function auto(value: unknown): Boxed {
  const ticked = unset(value) || value === ''
  return { ticked, text: ticked ? '' : shown(value) }
}

/** The log level's option: the number clamped to 0..5, unset TRACE:4. */
function logLevel(value: unknown): string {
  if (typeof value !== 'number') return '4'
  return String(Math.min(5, Math.max(0, Math.trunc(value))))
}

/** A select's value: one of `known`, anything else `default`. */
function oneOf(value: unknown, known: string[]): string {
  return typeof value === 'string' && known.includes(value) ? value : 'default'
}

/** The comma list of Hostname, empty entries dropped; empty is localhost. */
function hostnames(hostname: unknown): string[] {
  const list = (typeof hostname === 'string' ? hostname : '')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s !== '')
  return list.length > 0 ? list : ['localhost']
}

/**
 * The Bind to rows, fixed from the file read: localhost, 0.0.0.0, each chosen
 * value matching no row, then the interfaces.
 */
function bindRows(hostname: unknown, interfaces: ServerDoc['interfaces']) {
  const fixed: Row[] = [
    { value: 'localhost', text: t('server-bind-localhost') },
    { value: '0.0.0.0', text: t('server-bind-all') },
  ]
  const ifaces = interfaces.map((i) => ({ value: i.value, text: i.label }))
  const known = [...fixed, ...ifaces].map((r) => r.value.toLowerCase())
  const gone = hostnames(hostname)
    .filter((v) => !known.includes(v.toLowerCase()))
    .filter(
      (v, i, all) =>
        all.findIndex((w) => w.toLowerCase() === v.toLowerCase()) === i,
    )
    .map((v) => ({ value: v, text: t('server-bind-gone', v) }))
  return [...fixed, ...gone, ...ifaces]
}

/** The controls of the values `v`: a row is checked when the list names it (no case). */
function fromValues(v: Keys, d: Keys, rows: Row[]): Draft {
  const picked = hostnames(v.Hostname).map((s) => s.toLowerCase())
  return {
    Port: withDefault(v.Port, d.Port),
    Hostname: rows.map((r) => picked.includes(r.value.toLowerCase())),
    OpencodeBaseUrl: withDefault(v.OpencodeBaseUrl, d.OpencodeBaseUrl),
    OpencodeApiKey: auto(v.OpencodeApiKey),
    Threads: auto(v.Threads),
    ThreadsBatch: auto(v.ThreadsBatch),
    ModelsMax: withDefault(v.ModelsMax, d.ModelsMax),
    ModelsMemoryMargin: withDefault(v.ModelsMemoryMargin, d.ModelsMemoryMargin),
    ModelsDir: shown(v.ModelsDir),
    Webui: v.Webui == null || v.Webui === true,
    WebuiMcpProxy: v.WebuiMcpProxy === true,
    LogVerbosity: logLevel(v.LogVerbosity),
    SaveStateOnShutdown: v.SaveStateOnShutdown === true,
    StateDir: shown(v.StateDir),
    RocblasUseHipblaslt: oneOf(v.RocblasUseHipblaslt, ['on', 'off']),
    RocprofAttach: v.RocprofAttach === true,
    CudaAllreduce: oneOf(v.CudaAllreduce, ['nccl', 'internal', 'none']),
    CudaDisableGraphs: v.CudaDisableGraphs === true,
    AttnRotDisable: v.AttnRotDisable === true,
  }
}

const WHOLE = /^-?\d+$/

/** A number box: unset when ticked, else the integer (the trimmed text when it is none). */
function number(b: Boxed): number | string | null {
  if (b.ticked) return null
  const s = b.text.trim()
  return WHOLE.test(s) ? Number(s) : s
}

/**
 * The Base URL as the GUI saves it (`server_form.rs` `form_to_config`):
 * trimmed, every trailing `/` removed, then a trailing `/v1`, then every
 * trailing `/` again; blank is unset.
 */
function baseUrl(b: Boxed): string | null {
  if (b.ticked) return null
  let s = b.text.trim().replace(/\/+$/, '')
  if (s.endsWith('/v1')) s = s.slice(0, -3)
  s = s.replace(/\/+$/, '')
  return s === '' ? null : s
}

/** A folder: trimmed, blank is unset. */
function folder(text: string): string | null {
  const s = text.trim()
  return s === '' ? null : s
}

/** What a save sends (5.4); dirty compares these too. */
function toValues(d: Draft, rows: Row[]): Keys {
  return {
    Port: number(d.Port),
    Hostname: rows
      .filter((_, i) => d.Hostname[i])
      .map((r) => r.value)
      .join(','),
    OpencodeBaseUrl: baseUrl(d.OpencodeBaseUrl),
    OpencodeApiKey:
      d.OpencodeApiKey.ticked || d.OpencodeApiKey.text.trim() === ''
        ? null
        : d.OpencodeApiKey.text,
    Threads: number(d.Threads),
    ThreadsBatch: number(d.ThreadsBatch),
    ModelsMax: number(d.ModelsMax),
    ModelsMemoryMargin: number(d.ModelsMemoryMargin),
    ModelsDir: folder(d.ModelsDir),
    Webui: d.Webui,
    WebuiMcpProxy: d.WebuiMcpProxy,
    LogVerbosity: Number(d.LogVerbosity),
    SaveStateOnShutdown: d.SaveStateOnShutdown,
    StateDir: folder(d.StateDir),
    RocblasUseHipblaslt:
      d.RocblasUseHipblaslt === 'default' ? null : d.RocblasUseHipblaslt,
    RocprofAttach: d.RocprofAttach,
    CudaAllreduce: d.CudaAllreduce === 'default' ? null : d.CudaAllreduce,
    CudaDisableGraphs: d.CudaDisableGraphs,
    AttnRotDisable: d.AttnRotDisable,
  }
}

/** `[min, max]` of a key from the API's limits, or null when it gave none. */
function range(limits: unknown, key: string): [number, number] | null {
  const pair = isRecord(limits) ? limits[key] : undefined
  return Array.isArray(pair) &&
    pair.length === 2 &&
    pair.every((n) => typeof n === 'number')
    ? [pair[0], pair[1]]
    : null
}

/** The error of a number box's text, or undefined when it passes. */
function numberError(b: Boxed, limits: unknown, key: string) {
  const r = range(limits, key)
  if (b.ticked || r === null) return undefined
  const s = b.text.trim()
  const [min, max] = r
  if (WHOLE.test(s) && Number(s) >= min && Number(s) <= max) return undefined
  return max === 2147483647
    ? t('field-error-number-min', String(min))
    : t('field-error-number', String(min), String(max))
}

/** The error of a text that the INI file would cut at `;` or `#`. */
function markersError(text: string, checked: boolean) {
  return checked && /[;#]/.test(text)
    ? t('field-error-comment-markers')
    : undefined
}

/** The client checks of 5.4, failures in DOM order. */
function checks(d: Draft, limits: unknown): [Key, string][] {
  const all: [Key, string | undefined][] = [
    ['Port', numberError(d.Port, limits, 'Port')],
    [
      'OpencodeBaseUrl',
      markersError(d.OpencodeBaseUrl.text, !d.OpencodeBaseUrl.ticked),
    ],
    [
      'OpencodeApiKey',
      markersError(d.OpencodeApiKey.text, !d.OpencodeApiKey.ticked),
    ],
    ['Threads', numberError(d.Threads, limits, 'Threads')],
    ['ThreadsBatch', numberError(d.ThreadsBatch, limits, 'ThreadsBatch')],
    ['ModelsMax', numberError(d.ModelsMax, limits, 'ModelsMax')],
    [
      'ModelsMemoryMargin',
      numberError(d.ModelsMemoryMargin, limits, 'ModelsMemoryMargin'),
    ],
    ['ModelsDir', markersError(d.ModelsDir, true)],
    ['StateDir', markersError(d.StateDir, d.SaveStateOnShutdown)],
  ]
  return all.filter((c): c is [Key, string] => c[1] !== undefined)
}

/** The text of a check key the configurator shares (5.6), else undefined. */
function refusalText(key: string, args: string[]): string | undefined {
  switch (key) {
    case 'not-whole-number-in-range':
      return t('api-not-whole-number-in-range', ...args)
    case 'comment-markers':
      return t('api-comment-markers', ...args)
    case 'load-on-startup-over-max':
      return t('api-load-on-startup-over-max', ...args)
    case 'load-on-startup-unlimited':
      return t('api-load-on-startup-unlimited', ...args)
    case 'client-address-in-use':
      return t('api-client-address-in-use', ...args)
    case 'client-unresolvable-host':
      return t('api-client-unresolvable-host', ...args)
    default:
      return undefined
  }
}

/** The field of a check key when the bff names none (5.6's fallback table). */
function fallbackField(key: string, args: string[]): string | undefined {
  switch (key) {
    case 'not-whole-number-in-range':
    case 'comment-markers':
      return args[0]
    case 'load-on-startup-over-max':
    case 'load-on-startup-unlimited':
      return 'ModelsMax'
    case 'client-address-in-use':
      return 'Port'
    case 'client-unresolvable-host':
      return 'Hostname'
    default:
      return undefined
  }
}

/** What the PUT answered, read by key in the order of the brief (section 3). */
type SaveAnswer =
  | { kind: 'saved'; revision: string; running: boolean }
  | { kind: 'signed-out' }
  | { kind: 'changed-elsewhere' }
  | { kind: 'refused'; text: string; field?: string }
  | { kind: 'failed'; text: string }

async function sendSave(
  id: string,
  revision: unknown,
  values: Keys,
): Promise<SaveAnswer> {
  const failed = (text: string): SaveAnswer => ({ kind: 'failed', text })
  let reply
  try {
    reply = await call(`/api/instances/${encodeURIComponent(id)}/server`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ revision, values }),
    })
  } catch {
    return failed(t('save-failed'))
  }
  if (reply.status === 401) return { kind: 'signed-out' }
  const body = reply.body
  if (reply.ok) {
    return isRecord(body) &&
      typeof body.revision === 'string' &&
      typeof body.running === 'boolean'
      ? { kind: 'saved', revision: body.revision, running: body.running }
      : failed(t('save-failed'))
  }
  const error = apiError(body)
  if (error === null) return failed(t('save-failed'))
  switch (error.key) {
    case 'server-changed-elsewhere':
      return { kind: 'changed-elsewhere' }
    case 'client-not-authorized':
      return failed(t('api-client-not-authorized'))
    case 'client-unavailable':
      return failed(t('api-client-unavailable'))
    case 'client-no-instance':
      return failed(t('instance-not-found', id))
  }
  const text = refusalText(error.key, error.args)
  if (text !== undefined) {
    const field =
      isRecord(body) && typeof body.field === 'string'
        ? body.field
        : fallbackField(error.key, error.args)
    return { kind: 'refused', text, field }
  }
  if (reply.status === 422)
    return { kind: 'refused', text: t('api-other', error.key) }
  return failed(t('save-failed'))
}

/** The bar's alert, with the button that answers it when there is one. */
type Alert = {
  text: string
  action?: 'reload' | 'sign-in'
  /** A session-ended alert: the page's sign-ins kept when it was set. */
  at?: number
}

/**
 * The page's run actions: how many started, and the number (from 1) of the
 * last one that settled, 0 for none.
 */
export type Runs = { started: number; settled: number }

export default function ServerForm({
  id,
  doc,
  canConfigure,
  renewals,
  runs,
  onDirty,
  onReload,
  onRenewed,
}: {
  id: string
  doc: ServerDoc
  canConfigure: boolean
  /** Raised by every sign-in that kept the page: a session-ended alert goes. */
  renewals: number
  /** A settled action started after a save-saved-restart ends it (44a). */
  runs: Runs
  /** Told whether the form holds unsaved changes; false when it goes. */
  onDirty: (dirty: boolean) => void
  /** Reads the file again, the edits discarded. */
  onReload: () => void
  /** A sign-in in the dialog: true when the edits belong to it and stay. */
  onRenewed: (session: Session) => boolean
}) {
  const fid = useId()
  const readOnly = !canConfigure
  const [rows] = useState(() => bindRows(doc.values.Hostname, doc.interfaces))
  const [loaded, setLoaded] = useState({
    revision: doc.revision,
    values: doc.values,
  })
  const [draft, setDraft] = useState(() =>
    fromValues(doc.values, doc.defaults, rows),
  )
  const [errors, setErrors] = useState<Partial<Record<Key, string>>>({})
  const [stored, setAlert] = useState<Alert | null>(null)
  const alert =
    stored?.action === 'sign-in' && stored.at !== renewals ? null : stored
  const [phase, setPhase] = useState<
    'idle' | 'saving' | 'saved' | 'saved-restart'
  >('idle')
  // The run actions started when the last save-saved-restart answered.
  const [restartAt, setRestartAt] = useState(0)
  const [asking, setAsking] = useState<'reload' | 'sign-in' | null>(null)
  // A second Save while one runs sends nothing.
  const saving = useRef(false)
  const bar = useRef<HTMLDivElement>(null)

  const values = toValues(draft, rows)
  const dirty =
    JSON.stringify(values) !==
    JSON.stringify(
      toValues(fromValues(loaded.values, doc.defaults, rows), rows),
    )

  useEffect(() => {
    onDirty(dirty)
    return () => onDirty(false)
  }, [dirty, onDirty])

  // Closing or reloading the tab asks only while there is something to lose.
  useEffect(() => {
    if (!dirty) return
    const ask = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      // For browsers that predate preventDefault() here (brief, 38).
      e.returnValue = true
    }
    window.addEventListener('beforeunload', ask)
    return () => window.removeEventListener('beforeunload', ask)
  }, [dirty])

  // The scroll padding follows the bar's rendered height (DESIGN.md 4).
  useEffect(() => {
    const el = bar.current
    if (el === null || typeof ResizeObserver === 'undefined') return
    const root = document.documentElement.style
    const observer = new ResizeObserver(() =>
      root.setProperty('--save-bar-height', `${el.offsetHeight}px`),
    )
    observer.observe(el)
    return () => {
      observer.disconnect()
      root.removeProperty('--save-bar-height')
    }
  }, [readOnly])

  const element = (suffix: string) =>
    document.getElementById(`${fid}-${suffix}`) as HTMLInputElement | null

  /** Focus on a field: its control, else its default box, else its switch (34b). */
  function focusField(key: string) {
    if (key === 'Hostname') {
      // The rendered rows, not this closure's draft: Bind to may have
      // changed while a PUT ran (34a).
      const first = rows.findIndex(
        (_, i) => element(`Hostname-row-${i}`)?.checked === true,
      )
      element(`Hostname-row-${first}`)?.focus()
      return
    }
    const control = element(`${key}-control`)
    if (control !== null && !control.disabled) {
      control.focus()
      return
    }
    const fallback =
      element(`${key}-box`) ?? element(`${SWITCH_OF[key as Key]}-control`)
    fallback?.focus()
  }

  function edit<K extends Key>(key: K, value: Draft[K]) {
    setDraft((d) => ({ ...d, [key]: value }))
    setErrors((e) => {
      if (!(key in e)) return e
      const rest = { ...e }
      delete rest[key]
      return rest
    })
    setPhase((p) => (p === 'saving' ? p : 'idle'))
  }

  type BoxedKey = {
    [K in Key]: Draft[K] extends Boxed ? K : never
  }[Key]

  const tick = (key: BoxedKey) => (ticked: boolean) =>
    edit(key, { ...draft[key], ticked })
  const text = (key: BoxedKey) => (text: string) =>
    edit(key, { ...draft[key], text })

  /** A thread count's auto box: unticking focuses its emptied textbox (5.2). */
  const tickAuto = (key: 'Threads' | 'ThreadsBatch') => (ticked: boolean) => {
    flushSync(() => tick(key)(ticked))
    if (!ticked) element(`${key}-control`)?.focus()
  }

  /** Bind to keeps one row checked and 0.0.0.0 alone (5.3). */
  function toggleRow(i: number) {
    const checked = [...draft.Hostname]
    if (i === ALL) {
      const on = !checked[ALL]
      checked.fill(false)
      checked[on ? ALL : LOCALHOST] = true
    } else {
      checked[i] = !checked[i]
      if (checked[i]) checked[ALL] = false
      if (!checked.includes(true)) checked[LOCALHOST] = true
    }
    edit('Hostname', checked)
  }

  function discard() {
    if (!dirty || saving.current) return
    setDraft(fromValues(loaded.values, doc.defaults, rows))
    setErrors({})
    setAlert(null)
    setPhase('idle')
  }

  async function save() {
    if (!dirty || saving.current) return
    const failures = checks(draft, doc.limits)
    if (failures.length > 0) {
      // Rendered first, so the focused control is announced with its error.
      flushSync(() => {
        setErrors(Object.fromEntries(failures))
        setAlert({ text: t('save-invalid') })
      })
      focusField(failures[0]![0])
      return
    }
    saving.current = true
    setErrors({})
    setAlert(null)
    setPhase('saving')
    const sent = draft
    const answer = await sendSave(id, loaded.revision, values)
    saving.current = false
    if (answer.kind === 'saved') {
      setLoaded({ revision: answer.revision, values })
      // What the user typed while it ran is kept (33b).
      setDraft((d) => (d === sent ? fromValues(values, doc.defaults, rows) : d))
      setPhase(answer.running ? 'saved-restart' : 'saved')
      setRestartAt(runs.started)
      return
    }
    setPhase('idle')
    switch (answer.kind) {
      case 'signed-out':
        setAlert({ text: t('session-ended'), action: 'sign-in', at: renewals })
        return
      case 'changed-elsewhere':
        setAlert({ text: t('api-server-changed-elsewhere'), action: 'reload' })
        return
      case 'refused': {
        const field =
          answer.field !== undefined && Object.hasOwn(draft, answer.field)
            ? answer.field
            : undefined
        flushSync(() => {
          setAlert({ text: t('save-refused', answer.text) })
          if (field !== undefined) setErrors({ [field]: answer.text })
        })
        if (field !== undefined) focusField(field)
        return
      }
      case 'failed':
        setAlert({ text: answer.text })
    }
  }

  /** Closes the bar's dialog, then puts the focus on `suffix`'s element. */
  function closeAsking(suffix: string) {
    flushSync(() => setAsking(null))
    element(suffix)?.focus()
  }

  function signedIn(session: Session) {
    if (!onRenewed(session)) return
    flushSync(() => {
      setAsking(null)
      setAlert(null)
    })
    element('save')?.focus()
  }

  const v = draft
  const d = doc.defaults
  const ended = phase === 'saved-restart' && runs.settled > restartAt
  const status =
    phase === 'saving' ? phase : dirty ? 'unsaved' : ended ? 'idle' : phase
  const statusText = {
    idle: '',
    saving: t('save-saving'),
    unsaved: t('save-unsaved'),
    saved: t('save-saved'),
    'saved-restart': t('save-saved-restart'),
  }[status]
  const shownBox = (b: Boxed, fallback: unknown) =>
    b.ticked ? shown(fallback) : b.text
  const autoBox = (b: Boxed) => (b.ticked ? '' : b.text)

  return (
    <>
      {readOnly && <p className="notice">{t('server-read-only')}</p>}
      <Card title={t('server-network')}>
        <DefaultBoxField
          id={`${fid}-Port`}
          label={t('server-port')}
          hint={t('server-port-hint')}
          error={errors.Port}
          box={t('field-default')}
          numeric
          ticked={v.Port.ticked}
          text={shownBox(v.Port, d.Port)}
          readOnly={readOnly}
          onTick={tick('Port')}
          onText={text('Port')}
        />
        <CheckListField
          id={`${fid}-Hostname`}
          label={t('server-bind-to')}
          hint={t('server-bind-to-hint')}
          error={errors.Hostname}
          rows={rows.map((r, i) => ({
            ...r,
            checked: v.Hostname[i] === true,
            disabled: readOnly || (v.Hostname[ALL] === true && i !== ALL),
          }))}
          onToggle={toggleRow}
        />
        <DefaultBoxField
          id={`${fid}-OpencodeBaseUrl`}
          label={t('server-base-url')}
          hint={t('server-base-url-hint')}
          error={errors.OpencodeBaseUrl}
          box={t('field-default')}
          ticked={v.OpencodeBaseUrl.ticked}
          text={shownBox(v.OpencodeBaseUrl, d.OpencodeBaseUrl)}
          readOnly={readOnly}
          onTick={tick('OpencodeBaseUrl')}
          onText={text('OpencodeBaseUrl')}
        />
        <SecretField
          id={`${fid}-OpencodeApiKey`}
          label={t('server-api-key')}
          hint={t('server-api-key-hint')}
          error={errors.OpencodeApiKey}
          box={t('field-no-key')}
          ticked={v.OpencodeApiKey.ticked}
          value={autoBox(v.OpencodeApiKey)}
          readOnly={readOnly}
          withheld={withholdsKey(doc)}
          onTick={tick('OpencodeApiKey')}
          onText={text('OpencodeApiKey')}
        />
      </Card>
      <Card title={t('server-resources')}>
        <DefaultBoxField
          id={`${fid}-Threads`}
          label={t('server-cpu-threads')}
          hint={t('server-cpu-threads-hint')}
          error={errors.Threads}
          box={t('field-auto')}
          numeric
          ticked={v.Threads.ticked}
          text={autoBox(v.Threads)}
          readOnly={readOnly}
          onTick={tickAuto('Threads')}
          onText={text('Threads')}
        />
        <DefaultBoxField
          id={`${fid}-ThreadsBatch`}
          label={t('server-batch-threads')}
          hint={t('server-batch-threads-hint')}
          error={errors.ThreadsBatch}
          box={t('field-auto')}
          numeric
          ticked={v.ThreadsBatch.ticked}
          text={autoBox(v.ThreadsBatch)}
          readOnly={readOnly}
          onTick={tickAuto('ThreadsBatch')}
          onText={text('ThreadsBatch')}
        />
        <DefaultBoxField
          id={`${fid}-ModelsMax`}
          label={t('server-max-models')}
          hint={t('server-max-models-hint')}
          error={errors.ModelsMax}
          box={t('field-default')}
          numeric
          ticked={v.ModelsMax.ticked}
          text={shownBox(v.ModelsMax, d.ModelsMax)}
          readOnly={readOnly}
          onTick={tick('ModelsMax')}
          onText={text('ModelsMax')}
        />
        <DefaultBoxField
          id={`${fid}-ModelsMemoryMargin`}
          label={t('server-memory-margin')}
          hint={t('server-memory-margin-hint')}
          error={errors.ModelsMemoryMargin}
          box={t('field-default')}
          numeric
          unit="MiB"
          ticked={v.ModelsMemoryMargin.ticked}
          text={shownBox(v.ModelsMemoryMargin, d.ModelsMemoryMargin)}
          readOnly={readOnly}
          onTick={tick('ModelsMemoryMargin')}
          onText={text('ModelsMemoryMargin')}
        />
        <TextField
          id={`${fid}-ModelsDir`}
          label={t('server-models-folder')}
          hint={t('server-models-folder-hint')}
          error={errors.ModelsDir}
          value={v.ModelsDir}
          placeholder={t('server-folder-instance-default', shown(d.ModelsDir))}
          disabled={readOnly}
          onChange={(s) => edit('ModelsDir', s)}
        />
      </Card>
      <Card title={t('server-advanced')}>
        <CheckboxField
          id={`${fid}-Webui`}
          label={t('server-web-ui')}
          hint={t('server-web-ui-hint')}
          error={errors.Webui}
          toggle={t('server-web-ui-toggle')}
          checked={v.Webui}
          disabled={readOnly}
          onChange={(c) => edit('Webui', c)}
        />
        <CheckboxField
          id={`${fid}-WebuiMcpProxy`}
          label={t('server-mcp-proxy')}
          hint={t('server-mcp-proxy-hint')}
          error={errors.WebuiMcpProxy}
          toggle={t('server-mcp-proxy-toggle')}
          checked={v.WebuiMcpProxy}
          disabled={readOnly || !v.Webui}
          onChange={(c) => edit('WebuiMcpProxy', c)}
        />
        <SelectField
          id={`${fid}-LogVerbosity`}
          error={errors.LogVerbosity}
          label={t('server-log-level')}
          hint={t('server-log-level-hint')}
          options={LOG_LEVELS.map((text, i) => ({ value: String(i), text }))}
          value={v.LogVerbosity}
          disabled={readOnly}
          onChange={(s) => edit('LogVerbosity', s)}
        />
      </Card>
      <Card title={t('server-snapshots')} intro={t('server-snapshots-intro')}>
        <CheckboxField
          id={`${fid}-SaveStateOnShutdown`}
          label={t('server-save-on-shutdown')}
          hint={t('server-save-on-shutdown-hint')}
          error={errors.SaveStateOnShutdown}
          toggle={t('server-save-on-shutdown-toggle')}
          checked={v.SaveStateOnShutdown}
          disabled={readOnly}
          onChange={(c) => edit('SaveStateOnShutdown', c)}
        />
        <TextField
          id={`${fid}-StateDir`}
          label={t('server-snapshot-folder')}
          hint={t('server-snapshot-folder-hint')}
          error={errors.StateDir}
          value={v.StateDir}
          placeholder={t('server-folder-instance-default', shown(d.StateDir))}
          disabled={readOnly || !v.SaveStateOnShutdown}
          onChange={(s) => edit('StateDir', s)}
        />
      </Card>
      <Card title={t('server-sdk')} intro={t('server-sdk-intro')}>
        <div className="subgroup">
          <h4>{'CUDA'}</h4>
          <p className="intro">{t('server-cuda-nothing')}</p>
        </div>
        <div className="subgroup">
          <h4>{'ROCm'}</h4>
          <SelectField
            id={`${fid}-RocblasUseHipblaslt`}
            error={errors.RocblasUseHipblaslt}
            label="hipBLASLt"
            hint={t('server-hipblaslt-hint')}
            options={[
              { value: 'default', text: t('option-default') },
              { value: 'on', text: t('option-on') },
              { value: 'off', text: t('option-off') },
            ]}
            value={v.RocblasUseHipblaslt}
            disabled={readOnly}
            onChange={(s) => edit('RocblasUseHipblaslt', s)}
          />
          <CheckboxField
            id={`${fid}-RocprofAttach`}
            label={t('server-profiler-attach')}
            hint={t('server-profiler-attach-hint')}
            error={errors.RocprofAttach}
            toggle={t('server-profiler-attach-toggle')}
            checked={v.RocprofAttach}
            disabled={readOnly}
            onChange={(c) => edit('RocprofAttach', c)}
          />
        </div>
        <div className="subgroup">
          <h4>{t('server-cuda-and-rocm')}</h4>
          <SelectField
            id={`${fid}-CudaAllreduce`}
            error={errors.CudaAllreduce}
            label={t('server-allreduce')}
            hint={t('server-allreduce-hint')}
            options={[
              { value: 'default', text: t('option-default') },
              { value: 'nccl', text: 'nccl' },
              { value: 'internal', text: t('option-internal') },
              { value: 'none', text: t('option-none') },
            ]}
            value={v.CudaAllreduce}
            disabled={readOnly}
            onChange={(s) => edit('CudaAllreduce', s)}
          />
          <CheckboxField
            id={`${fid}-CudaDisableGraphs`}
            label={t('server-gpu-graphs')}
            hint={t('server-gpu-graphs-hint')}
            error={errors.CudaDisableGraphs}
            toggle={t('server-gpu-graphs-toggle')}
            checked={v.CudaDisableGraphs}
            disabled={readOnly}
            onChange={(c) => edit('CudaDisableGraphs', c)}
          />
        </div>
        <div className="subgroup">
          <h4>{'llama.cpp'}</h4>
          <CheckboxField
            id={`${fid}-AttnRotDisable`}
            label={t('server-kv-rotation')}
            hint={t('server-kv-rotation-hint')}
            error={errors.AttnRotDisable}
            toggle={t('server-kv-rotation-toggle')}
            checked={v.AttnRotDisable}
            disabled={readOnly}
            onChange={(c) => edit('AttnRotDisable', c)}
          />
        </div>
      </Card>
      {!readOnly && (
        <div className="save-bar" ref={bar}>
          <div className="save-messages">
            <p
              role="status"
              className={
                status === 'saved' || status === 'saved-restart'
                  ? `save-status ${status}`
                  : 'save-status'
              }
            >
              {statusText}
            </p>
            {alert !== null && (
              <p role="alert" className="save-alert">
                {alert.text}
              </p>
            )}
            {alert?.action === 'reload' && (
              <button
                id={`${fid}-reload`}
                type="button"
                className="secondary"
                onClick={() => setAsking('reload')}
              >
                {t('save-reload')}
              </button>
            )}
            {alert?.action === 'sign-in' && (
              <button
                id={`${fid}-sign-in`}
                type="button"
                className="secondary"
                onClick={() => setAsking('sign-in')}
              >
                {t('session-sign-in')}
              </button>
            )}
          </div>
          <div className="save-actions">
            <button
              type="button"
              className="secondary"
              aria-disabled={!dirty || phase === 'saving' || undefined}
              onClick={discard}
            >
              {t('save-discard')}
            </button>
            <button
              id={`${fid}-save`}
              type="button"
              className="primary"
              aria-disabled={!dirty || phase === 'saving' || undefined}
              onClick={save}
            >
              {phase === 'saving' ? t('save-saving') : t('save-button')}
            </button>
          </div>
        </div>
      )}
      {asking === 'reload' && (
        <DiscardDialog
          onKeep={() => closeAsking('reload')}
          onDiscard={() => {
            setAsking(null)
            onReload()
          }}
        />
      )}
      {asking === 'sign-in' && (
        <Dialog
          title={t('signin-title')}
          onClose={() => closeAsking('sign-in')}
        >
          <SignIn
            required={false}
            inDialog
            onSignedIn={signedIn}
            onCancel={() => closeAsking('sign-in')}
          />
        </Dialog>
      )}
    </>
  )
}
