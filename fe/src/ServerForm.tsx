// The Server form of an instance (fe/briefs/01-server.md, sections 3 and 5):
// the cards and fields of pai-config's Server tab, in its order, each control
// showing the file's value. Every signed-in session edits it: the controls
// follow their dependencies (5.3) and the save bar checks the values, sends
// them typed (5.4) and shows the answer by its key. The answer is read in
// save.ts, shared with the model profile page; this file keeps the keys of its own
// (section 3) and the tables of its check keys.
import { useId, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { call, isRecord } from './api.ts'
import { boxText, tickBox, type Boxed } from './boxed.ts'
import Card from './Card.tsx'
import { DiscardDialog } from './Dialog.tsx'
import {
  CheckboxField,
  CheckListField,
  DefaultBoxField,
  SecretField,
  SelectField,
  TextField,
} from './Field.tsx'
import { useFieldErrors, type RefusalTable } from './refusal.ts'
import { sendFailure, saveAnswer, type SaveAnswer } from './save.ts'
import {
  SaveBar,
  useAsk,
  useDirtyReport,
  useSaveAlert,
  useSaveBarPadding,
  useWarnOnLeave,
} from './SaveBar.tsx'
import type { Session } from './session.ts'
import { SignInDialog, useSignInDialog } from './SignIn.tsx'
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

/** The check keys of 5.6 this form knows: the text of each, and the field each
 * names when the bff names none. One literal t() call per key. */
const REFUSALS: RefusalTable = {
  text: (key, args) => {
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
      case 'bind-all-with-others':
        return t('api-bind-all-with-others', ...args)
      default:
        return undefined
    }
  },
  field: (key, args) => {
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
      case 'bind-all-with-others':
        return 'Hostname'
      default:
        return undefined
    }
  },
}

/** The keys of this page beside the shared ones of `saveAnswer`. */
function serverKey(id: string, key: string): SaveAnswer | undefined {
  switch (key) {
    case 'server-changed-elsewhere':
      return { kind: 'changed-elsewhere' }
    case 'client-no-instance':
      return { kind: 'failed', text: t('instance-not-found', id) }
    default:
      return undefined
  }
}

async function sendSave(
  id: string,
  revision: unknown,
  values: Keys,
): Promise<SaveAnswer> {
  let reply
  try {
    reply = await call(`/api/instances/${encodeURIComponent(id)}/server`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ revision, values }),
    })
  } catch (e) {
    // A timeout may have written the file (4.8, 47d).
    return { kind: 'failed', text: sendFailure(e) }
  }
  return saveAnswer(reply, (key) => serverKey(id, key), REFUSALS)
}

/**
 * The page's run actions: how many started, and the number (from 1) of the
 * last one that settled, 0 for none.
 */
export type Runs = { started: number; settled: number }

export default function ServerForm({
  id,
  doc,
  renewals,
  runs,
  onDirty,
  onReload,
  onRenewed,
}: {
  id: string
  doc: ServerDoc
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
  const [rows] = useState(() => bindRows(doc.values.Hostname, doc.interfaces))
  const [loaded, setLoaded] = useState({
    revision: doc.revision,
    values: doc.values,
  })
  const [draft, setDraft] = useState(() =>
    fromValues(doc.values, doc.defaults, rows),
  )
  const { errors, set: setErrors, clear: clearError } = useFieldErrors<Key>()
  const [phase, setPhase] = useState<
    'idle' | 'saving' | 'saved' | 'saved-restart'
  >('idle')
  // The run actions started when the last save-saved-restart answered.
  const [restartAt, setRestartAt] = useState(0)
  // A second Save while one runs sends nothing.
  const saving = useRef(false)
  const bar = useSaveBarPadding()
  // The bar's Sign in, and the dialog it opens (36, 43a).
  const signInId = `${fid}-sign-in`
  const saveId = `${fid}-save`
  const askSignIn = useSignInDialog(signInId)

  const values = toValues(draft, rows)
  const dirty =
    JSON.stringify(values) !==
    JSON.stringify(
      toValues(fromValues(loaded.values, doc.defaults, rows), rows),
    )

  useDirtyReport(dirty, onDirty)
  useWarnOnLeave(dirty)
  // The bar's alert, with the rule of 2h.4b (198b, 198c, 198g): it knows the
  // form's dirtiness and whether a save is in flight (the flag, not the
  // status: an edit made while the PUT runs leaves the status of the page), so
  // an alert about edits goes with them.
  const alerts = useSaveAlert(renewals, dirty, phase === 'saving')

  const element = (suffix: string) =>
    document.getElementById(`${fid}-${suffix}`) as HTMLInputElement | null

  // The bar's Reload: it asks before dropping the edits, and the focus goes
  // back to the button when the answer is kept (35, 38). A clean form has
  // nothing to lose, so it reads at once, as the preset page's does (145,
  // 2h.4b's 198c).
  const askReload = useAsk(dirty, onReload, () => element('reload')?.focus())

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
    clearError(key)
    setPhase((p) => (p === 'saving' ? p : 'idle'))
  }

  type BoxedKey = {
    [K in Key]: Draft[K] extends Boxed ? K : never
  }[Key]

  const tick = (key: BoxedKey) => (ticked: boolean) =>
    edit(key, { ...draft[key], ticked })
  const text = (key: BoxedKey) => (text: string) =>
    edit(key, { ...draft[key], text })

  /** A thread count's auto box: unticking always focuses its textbox, empty or
   * not, so the count is typed at once (5.2). */
  const tickAuto = (key: 'Threads' | 'ThreadsBatch') => (ticked: boolean) =>
    tickBox(
      draft[key],
      ticked,
      (b) => edit(key, b),
      element(`${key}-control`),
      'always',
    )

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

  /** Discard: the fields back to the loaded values and the field marks cleared
   * (136). The bar's alert is left to `useSaveAlert`: Discard is one more route
   * to a clean form, so an alert with an answer button stays and one without
   * goes with the edits (198d). */
  function discard() {
    if (!dirty || saving.current) return
    setDraft(fromValues(loaded.values, doc.defaults, rows))
    setErrors({})
    setPhase('idle')
  }

  async function save() {
    if (!dirty || saving.current) return
    const failures = checks(draft, doc.limits)
    if (failures.length > 0) {
      // Rendered first, so the focused control is announced with its error.
      flushSync(() => {
        setErrors(Object.fromEntries(failures))
        alerts.setAlert({ text: t('save-invalid') })
      })
      focusField(failures[0]![0])
      return
    }
    saving.current = true
    setErrors({})
    alerts.setAlert(null)
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
      // The session's answers: the bar's alert with Sign in after it (36, 36b).
      case 'signed-out':
        alerts.sessionEnded(false)
        return
      case 'revoked':
        alerts.sessionEnded(true)
        return
      case 'changed-elsewhere':
        alerts.setAlert({
          text: t('api-server-changed-elsewhere'),
          action: 'reload',
        })
        return
      case 'refused': {
        const field =
          answer.field !== undefined && Object.hasOwn(draft, answer.field)
            ? answer.field
            : undefined
        flushSync(() => {
          alerts.setAlert({ text: t('save-refused', answer.text) })
          if (field !== undefined) setErrors({ [field]: answer.text })
        })
        if (field !== undefined) focusField(field)
        return
      }
      case 'failed':
        alerts.setAlert({ text: answer.text })
    }
  }

  /** A sign-in in the bar's dialog: the page stays only for the same user;
   * the dialog closes, the alert goes, focus on Save changes (36). */
  function signedIn(session: Session) {
    if (!onRenewed(session)) return
    alerts.setAlert(null)
    askSignIn.close(saveId)
  }

  const v = draft
  const d = doc.defaults
  const ended = phase === 'saved-restart' && runs.settled > restartAt
  const status =
    phase === 'saving' ? phase : dirty ? 'unsaved' : ended ? 'idle' : phase
  const shownBox = (b: Boxed, fallback: unknown) =>
    b.ticked ? shown(fallback) : b.text

  return (
    <>
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
            disabled: v.Hostname[ALL] === true && i !== ALL,
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
          value={boxText(v.OpencodeApiKey)}
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
          text={boxText(v.Threads)}
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
          text={boxText(v.ThreadsBatch)}
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
          onChange={(c) => edit('Webui', c)}
        />
        <CheckboxField
          id={`${fid}-WebuiMcpProxy`}
          label={t('server-mcp-proxy')}
          hint={t('server-mcp-proxy-hint')}
          error={errors.WebuiMcpProxy}
          toggle={t('server-mcp-proxy-toggle')}
          checked={v.WebuiMcpProxy}
          disabled={!v.Webui}
          onChange={(c) => edit('WebuiMcpProxy', c)}
        />
        <SelectField
          id={`${fid}-LogVerbosity`}
          error={errors.LogVerbosity}
          label={t('server-log-level')}
          hint={t('server-log-level-hint')}
          options={LOG_LEVELS.map((text, i) => ({ value: String(i), text }))}
          value={v.LogVerbosity}
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
          onChange={(c) => edit('SaveStateOnShutdown', c)}
        />
        <TextField
          id={`${fid}-StateDir`}
          label={t('server-snapshot-folder')}
          hint={t('server-snapshot-folder-hint')}
          error={errors.StateDir}
          value={v.StateDir}
          placeholder={t('server-folder-instance-default', shown(d.StateDir))}
          disabled={!v.SaveStateOnShutdown}
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
            onChange={(s) => edit('RocblasUseHipblaslt', s)}
          />
          <CheckboxField
            id={`${fid}-RocprofAttach`}
            label={t('server-profiler-attach')}
            hint={t('server-profiler-attach-hint')}
            error={errors.RocprofAttach}
            toggle={t('server-profiler-attach-toggle')}
            checked={v.RocprofAttach}
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
            onChange={(s) => edit('CudaAllreduce', s)}
          />
          <CheckboxField
            id={`${fid}-CudaDisableGraphs`}
            label={t('server-gpu-graphs')}
            hint={t('server-gpu-graphs-hint')}
            error={errors.CudaDisableGraphs}
            toggle={t('server-gpu-graphs-toggle')}
            checked={v.CudaDisableGraphs}
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
            onChange={(c) => edit('AttnRotDisable', c)}
          />
        </div>
      </Card>
      <SaveBar
        bar={bar}
        status={status}
        alert={alerts.alert}
        dirty={dirty}
        saving={phase === 'saving'}
        reloadId={`${fid}-reload`}
        onReload={
          alerts.alert?.action === 'reload' ? askReload.press : undefined
        }
        signInId={signInId}
        onSignIn={
          alerts.alert?.action === 'sign-in' ? askSignIn.open : undefined
        }
        saveId={saveId}
        onDiscard={discard}
        onSave={save}
      />
      {askReload.asking && (
        <DiscardDialog onKeep={askReload.keep} onDiscard={askReload.confirm} />
      )}
      {askSignIn.asking && (
        <SignInDialog onSignedIn={signedIn} onClose={() => askSignIn.close()} />
      )}
    </>
  )
}
