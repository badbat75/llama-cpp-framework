// The New model profile form (brief 22, slices 22C1 and 22C2, as the "22P/22C
// check (2026-11-26)" amends it): the Models list's `+` and the form it opens,
// the Name field, the Start from picker with its three choices (a model file on
// this node, a Hugging Face repo, a copy of another model profile), Create and
// Cancel, the checks, the create and its answers. It is the list page's own
// (ModelProfiles.tsx), split out of it as RestartOffer.tsx and
// InstanceServerSwitch.tsx were. Every text is in strings/en_US.json.
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react'
import { flushSync } from 'react-dom'
import {
  apiError,
  call,
  isRecord,
  sessionEnded,
  sessionRevoked,
  type Reply,
} from './api.ts'
import Dialog from './Dialog.tsx'
import { FieldError, Hint } from './Field.tsx'
import { Icon } from './Icon.tsx'
import type { ModelProfile } from './ModelProfiles.tsx'
import { refusalOf, type RefusalTable } from './refusal.ts'
import { modelProfileHref } from './route.ts'
import type { Session } from './session.ts'
import SignIn from './SignIn.tsx'
import { t } from './strings.ts'

/* ── The New model profile form (brief 22, slice 22C1) ─────────────────── */

/**
 * One row of `GET /api/node/model-files` (fe/briefs/api-contract.md 226): the
 * full path of the manager's walk, its bytes, the shard count of a split model
 * and the first subfolder it sits in.
 */
type ModelFile = {
  path: string
  size: number
  shards: number
  category: string | null
}

/** The folder read the file picker works with, and why it gave nothing. */
type Folder =
  | { state: 'loading' }
  | { state: 'ready'; dir: string; rows: ModelFile[]; truncated: boolean }
  | { state: 'empty'; dir: string }
  | { state: 'unreadable'; arg: string }
  | { state: 'failed' }
  | { state: 'session' }

function isModelFile(value: unknown): value is ModelFile {
  return (
    isRecord(value) &&
    typeof value.path === 'string' &&
    typeof value.size === 'number' &&
    typeof value.shards === 'number'
  )
}

/** A file's own name, the place 22 P.5a's six words are looked for: the part
 * after the last `/` or `\`. */
function fileBaseName(path: string): string {
  const at = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  return path.slice(at + 1)
}

/** The files no picker offers: a projector, an imatrix and a draft's file,
 * named as llama.cpp names them (22 P.5a, `NOT_A_MODEL` in
 * `crates/core/src/hf.rs`): the file's own name, letter case kept. */
const NOT_A_MODEL = /mmproj|imatrix|mtp-|eagle3-|dflash-|dspark-/

/** The path as the folder's own `dir` spells it, what the typed text filters
 * (22 P.5a). A path outside `dir` is kept whole. */
function fileRelative(path: string, dir: string): string {
  return path.startsWith(`${dir}/`) ? path.slice(dir.length + 1) : path
}

/** The bytes as one option reads them: GiB with one decimal, MiB with none
 * below 0.1 GiB (brief 22, section 2). */
function fileSize(bytes: number): string {
  const gib = bytes / 2 ** 30
  return gib < 0.1
    ? `${Math.round(bytes / 2 ** 20)} MiB`
    : `${gib.toFixed(1)} GiB`
}

/** What one option reads: the path under `dir`, the size, and the shard count
 * of a split model (22 P.5). */
function fileOption(row: ModelFile, dir: string): string {
  const parts =
    row.shards > 1 ? `, ${t('model-file-parts', String(row.shards))}` : ''
  return `${fileRelative(row.path, dir)}, ${fileSize(row.size)}${parts}`
}

/** The options the list shows at most (22 P.5). */
const OPTION_MAX = 50

/** What one Hugging Face quant gives the create: the repo with the quant's tag,
 * and the file inside the repo when the row names one
 * (fe/briefs/api-contract.md 740). */
type HfValue = { 'hf-repo': string; 'hf-file'?: string }

/** One row of `GET /api/hf/files` (fe/briefs/api-contract.md 736): one
 * quantization of the repo, its bytes, its shard count, whether llama.cpp calls
 * it the default, and the pair the create sends. */
type HfFile = {
  tag: string
  shards: number
  size: number
  default: boolean
  value: HfValue
}

function isHfValue(value: unknown): value is HfValue {
  return (
    isRecord(value) &&
    typeof value['hf-repo'] === 'string' &&
    (value['hf-file'] === undefined || typeof value['hf-file'] === 'string')
  )
}

function isHfFile(value: unknown): value is HfFile {
  return (
    isRecord(value) &&
    typeof value.tag === 'string' &&
    typeof value.shards === 'number' &&
    typeof value.size === 'number' &&
    typeof value.default === 'boolean' &&
    isHfValue(value.value)
  )
}

/** The repo id as the bff's `valid_repo_id` judges it (22 P.8,
 * `crates/core/src/hf.rs`): two segments of ASCII letters, digits and `_`, each
 * of them run together by a single `.`, `-` or `_`, one `/` between the two,
 * the whole at most 256 characters. */
const REPO_ID =
  /^[A-Za-z0-9_]+([._-][A-Za-z0-9_]+)*\/[A-Za-z0-9_]+([._-][A-Za-z0-9_]+)*$/

/** The repo the field's text gives (22 P.8): a huggingface.co,
 * www.huggingface.co or hf.co address first reduced to its first two path
 * segments, then a `:TAG` end set apart. `null` for text that is no repo id. */
function repoOf(text: string): { repo: string; tag: string } | null {
  let s = text.trim()
  const host = /^(?:https?:\/\/)?(?:www\.)?(?:huggingface|hf)\.co\/(.+)$/i.exec(
    s,
  )
  if (host !== null)
    s = host[1]!
      .split('/')
      .filter((p) => p !== '')
      .slice(0, 2)
      .join('/')
  const at = s.lastIndexOf(':')
  const tag = at < 0 ? '' : s.slice(at + 1)
  if (at >= 0) s = s.slice(0, at)
  return s.length <= 256 && REPO_ID.test(s) ? { repo: s, tag } : null
}

/** What one quant reads (22 P.9): its tag, the size, the shard count of a model
 * split in parts, and llama.cpp's own mark on the default one. */
function quantLabel(row: HfFile): string {
  const parts =
    row.shards > 1 ? `, ${t('model-file-parts', String(row.shards))}` : ''
  const mark = row.default ? `, ${t('hf-quant-default')}` : ''
  return `${row.tag}, ${fileSize(row.size)}${parts}${mark}`
}

/** The name a model profile accepts (22 C.4), the rule the manager answers with
 * in `invalid-model-profile-id`. */
const NAME_ALLOWED = /^[A-Za-z0-9._-]+$/

/** The create answers with the words of its own keys; each key is one literal
 * t() call, as every page's table is. The field a refusal marks is the one the
 * reply names, except the taken name: the manager's 409 names no field and it
 * is Name's (22 C.6). */
const CREATE_REFUSALS: RefusalTable = {
  text: (key, args) => {
    switch (key) {
      case 'invalid-model-profile-id':
        return t('api-invalid-model-profile-id', ...args)
      case 'model-profile-name-taken':
        return t('api-model-profile-name-taken', ...args)
      case 'no-model-profile-named':
        return t('api-no-model-profile-named', ...args)
      case 'comment-markers':
        return t('api-comment-markers', ...args)
      case 'model-profile-no-model':
        return t('api-model-profile-no-model', ...args)
      default:
        return undefined
    }
  },
  field: (key) => (key === 'model-profile-name-taken' ? 'id' : undefined),
}

/** The listing's refusals (22 P.10, P.10a), each with the words of its own key,
 * one literal t() call per key. The bff's 400 `hf-repo-invalid` is the same rule
 * as the page's own check, so it goes under the field as that does; every other
 * refusal is the alert's, and a key the table does not know is `request-failed`.
 * The focus stays on Show quants for all of them. */
const HF_REFUSALS: RefusalTable = {
  text: (key, args) => {
    switch (key) {
      case 'hf-repo-invalid':
        return t('api-hf-repo-invalid')
      case 'hf-repo-unavailable':
        return t('api-hf-repo-unavailable', ...args)
      case 'hf-repo-moved':
        return t('api-hf-repo-moved', ...args)
      case 'hf-unreachable':
        return t('api-hf-unreachable')
      case 'hf-bad-answer':
        return t('api-hf-bad-answer')
      case 'hf-tool-missing':
        return t('api-hf-tool-missing')
      case 'hf-busy':
        return t('api-hf-busy')
      default:
        return undefined
    }
  },
  field: (key) => (key === 'hf-repo-invalid' ? 'hf' : undefined),
}

/**
 * The list's add (22 C.1a): a `+` right after the note that belongs to the
 * table, disclosing the New model profile form after it. Everything the visitor
 * typed is the form's own, so closing it clears all of it (22 C.3).
 */
export function NewProfile({
  rows,
  onRenewed,
  onRefresh,
}: {
  rows: ModelProfile[]
  onRenewed: (session: Session) => boolean
  onRefresh: () => void
}) {
  const opener = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  // A create is in flight: the form stays open with it (17b review R1).
  const sendingRef = useRef(false)

  /** Cancel, Escape, or the `+` pressed again: the form goes with every field,
   * error and alert, `aria-expanded` is false, and the focus returns to the `+`
   * (22 C.3). Nothing closes while a create runs. */
  function close() {
    if (sendingRef.current) return
    setOpen(false)
    opener.current?.focus()
  }

  return (
    <>
      <button
        ref={opener}
        type="button"
        id="model-profiles-new"
        className="ot-add"
        aria-expanded={open}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <Icon name="plus" />
        <span className="visually-hidden">{t('model-profiles-new')}</span>
      </button>
      {open && (
        <NewProfileForm
          rows={rows}
          sendingRef={sendingRef}
          onClose={close}
          onRenewed={onRenewed}
          onRefresh={onRefresh}
        />
      )}
    </>
  )
}

/**
 * The form itself, mounted while it is open (22 C.1): the note, the Name, the
 * Start from picker with its three choices (a model file on this node, a
 * Hugging Face repo, a copy of another profile), Create and Cancel, the folder's
 * and the listing's state lines, the form's own status line and alert place.
 */
function NewProfileForm({
  rows,
  sendingRef,
  onClose,
  onRenewed,
  onRefresh,
}: {
  rows: ModelProfile[]
  /** Set while the POST runs, read by the close (17b review R1). */
  sendingRef: { current: boolean }
  onClose: () => void
  onRenewed: (session: Session) => boolean
  /** The list read again with the form left open (22 C.7). */
  onRefresh: () => void
}) {
  const uid = useId()
  const titleId = `${uid}-title`
  const nameId = `${uid}-name`
  const fileRadioId = `${uid}-file-radio`
  const hfRadioId = `${uid}-hf-radio`
  const copyRadioId = `${uid}-copy-radio`
  const fileId = `${uid}-file`
  const listId = `${uid}-list`
  const hfId = `${uid}-hf`
  const showId = `${uid}-show`
  const quantId = `${uid}-quant`
  const copyId = `${uid}-copy`
  const [name, setName] = useState('')
  const [source, setSource] = useState<'file' | 'hf' | 'copy'>('file')
  const [from, setFrom] = useState('')
  const [text, setText] = useState('')
  const [picked, setPicked] = useState('')
  const [folder, setFolder] = useState<Folder>({ state: 'loading' })
  // The Hugging Face choice (22 P.7 to P.9): the field's text, the quants the
  // last listing gave with the one checked and the value it gives, and the read
  // in flight. The read is the field's own, started by Show quants.
  const [hfText, setHfText] = useState('')
  const [quants, setQuants] = useState<HfFile[] | null>(null)
  const [quantAt, setQuantAt] = useState<number | null>(null)
  const [hfPick, setHfPick] = useState<HfValue | null>(null)
  const [reading, setReading] = useState(false)
  const readingRef = useRef(false)
  // The retry stays mounted while the read it started is open, so its focus
  // survives the retry (22 P.6).
  const [canRetry, setCanRetry] = useState(false)
  // The list starts closed and stays closed when the folder's rows land: the
  // state line announces them. Down, an edit or a click in the box opens it,
  // Escape, a pick or the focus leaving closes it (22 P.3, 22 P.4).
  const [listOpen, setListOpen] = useState(false)
  const [active, setActive] = useState<number | null>(null)
  const [errors, setErrors] = useState<{
    id?: string
    model?: string
    hf?: string
    from?: string
  }>({})
  const [sent, setSent] = useState(false)
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  const [asking, setAsking] = useState(false)
  // Where a sign-in in the dialog sends the focus back to (22 C.7).
  const signedInTo = useRef<'create' | 'folder' | 'quants'>('create')

  // The Name takes the focus when the form opens (22 C.1).
  useEffect(() => {
    document.getElementById(nameId)?.focus()
  }, [nameId])

  /** The folder read answers the folder's state from the manager's reply, and
   * writes nothing: whoever starts the read writes the line it shows while it
   * is open (22 P.4, 22 P.6). */
  const readFiles = useCallback(async (): Promise<Folder> => {
    let answer: Reply | undefined
    try {
      answer = await call('/api/node/model-files')
    } catch {
      answer = undefined
    }
    const reply = answer
    if (reply === undefined) return { state: 'failed' }
    if (sessionEnded(reply)) return { state: 'session' }
    const body = reply.body
    if (
      !reply.ok ||
      !isRecord(body) ||
      !Array.isArray(body.files) ||
      !body.files.every(isModelFile)
    ) {
      // A read that did not answer with the folder says so and offers the
      // retry: the manager being down is one of them (22 P.6).
      return { state: 'failed' }
    }
    const dir = typeof body.dir === 'string' ? body.dir : ''
    if (isRecord(body.error) && typeof body.error.key === 'string') {
      const args = Array.isArray(body.error.args) ? body.error.args : []
      return {
        state: 'unreadable',
        arg: typeof args[0] === 'string' ? args[0] : dir,
      }
    }
    const files = body.files as ModelFile[]
    if (files.length === 0) return { state: 'empty', dir }
    return {
      state: 'ready',
      dir,
      rows: files,
      truncated: body.truncated === true,
    }
  }, [])

  // The showing of the file field reads the folder once (22 P.4): switching the
  // picker hides the field, it never unmounts, so nothing is read again.
  useEffect(() => {
    let live = true
    readFiles().then((next) => {
      if (!live) return
      setFolder(next)
      setCanRetry(next.state !== 'ready' && next.state !== 'session')
    })
    return () => {
      live = false
    }
  }, [readFiles])

  /** A read the visitor starts: the retry (22 P.6), and the sign-in on the
   * folder's line. The retry stays offered while it runs, so its focus
   * survives. */
  async function readFolder() {
    const next = await readFiles()
    setFolder(next)
    setCanRetry(next.state !== 'ready' && next.state !== 'session')
  }

  /** The rows the list shows: the folder's main models, narrowed by the typed
   * text. Text equal to a picked path keeps the list whole with that row marked
   * picked, so the visitor sees what is picked (22 P.3, 22 P.5a). */
  const shown = useMemo(() => {
    if (folder.state !== 'ready') return []
    const keep = folder.rows.filter(
      (r) => !NOT_A_MODEL.test(fileBaseName(r.path)),
    )
    if (picked !== '' && keep.some((r) => r.path === picked)) return keep
    const q = text.trim().toLowerCase()
    if (q === '') return keep
    return keep.filter((r) =>
      fileRelative(r.path, folder.dir).toLowerCase().includes(q),
    )
  }, [folder, text, picked])

  const expanded = listOpen && shown.length > 0
  const activeAt =
    expanded && active !== null ? Math.min(active, shown.length - 1) : null
  const dir = folder.state === 'ready' ? folder.dir : ''

  /** The folder's one state line (22 P.6): one text at a time, the walk's own
   * truncation said after the count. */
  const folderText = folderTextOf(folder, shown.length)

  /** A pick writes the row's whole path and closes the list (22 P.3). */
  function pick(row: ModelFile) {
    setText(row.path)
    setPicked(row.path)
    setListOpen(false)
    setActive(null)
    setErrors((e) => (e.model === undefined ? e : { ...e, model: undefined }))
  }

  function onFileKey(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      if (shown.length === 0) return
      e.preventDefault()
      if (!listOpen) {
        setListOpen(true)
        setActive(0)
        return
      }
      setActive((at) => {
        const last = shown.length - 1
        const cur = at ?? (e.key === 'ArrowDown' ? -1 : last)
        const next = e.key === 'ArrowDown' ? cur + 1 : cur - 1
        return Math.max(Math.min(next, last), 0)
      })
      return
    }
    if (e.key === 'Enter') {
      // Enter picks the active option; with none active, and with the list
      // closed, it is the form's own and submits (22 C.8).
      if (activeAt === null) return
      e.preventDefault()
      const row = shown[activeAt]
      if (row !== undefined) pick(row)
      return
    }
    if (e.key === 'Escape') {
      // Escape closes the list and keeps the text, and the form stays open
      // (22 P.3): the key never reaches the form's own Escape.
      if (!listOpen) return
      e.stopPropagation()
      e.preventDefault()
      setListOpen(false)
      setActive(null)
    }
  }

  /** Show quants (22 P.8, P.9, P.9a, P.10, P.10a): the field's text reduced to
   * a repo id and read once, the loading line and the inactive button while the
   * read runs, a second press sending nothing. The listing's pick on arrival is
   * the row of the field's `:TAG`, else the default one, else none. */
  async function showQuants() {
    if (readingRef.current) return
    const parsed = repoOf(hfText)
    if (parsed === null) {
      // The page's own check of the rule the bff checks: the words under the
      // field, the focus there, nothing sent.
      flushSync(() =>
        setErrors((e) => ({ ...e, hf: t('field-error-hf-repo') })),
      )
      document.getElementById(hfId)?.focus()
      return
    }
    readingRef.current = true
    setReading(true)
    setAlert(null)
    setQuants(null)
    setQuantAt(null)
    setHfPick(null)
    setStatus(t('hf-quants-loading', parsed.repo))
    let answer: Reply | undefined
    try {
      answer = await call(
        `/api/hf/files?repo=${encodeURIComponent(parsed.repo)}`,
      )
    } catch {
      answer = undefined
    }
    readingRef.current = false
    setReading(false)
    setStatus('')
    const reply = answer
    if (reply === undefined) {
      setAlert({ text: t('request-failed'), signIn: false })
      return
    }
    if (sessionEnded(reply)) {
      // A sign-in from here returns the focus to the button (22 P.10).
      signedInTo.current = 'quants'
      setAlert({ text: t('session-ended-retry'), signIn: true })
      return
    }
    if (sessionRevoked(reply)) {
      setAlert({ text: t('session-revoked'), signIn: false })
      return
    }
    const files =
      reply.ok && isRecord(reply.body) && Array.isArray(reply.body.files)
        ? (reply.body.files as HfFile[])
        : null
    if (reply.ok) {
      // A 200 that lists no file, or lists what the page cannot read, is the
      // same words: the bff answers a repo with no GGUF 502 hf-bad-answer, so a
      // 200 with nothing is an answer that cannot be read (22 P.9a).
      if (files === null || files.length === 0 || !files.every(isHfFile)) {
        setAlert({ text: t('api-hf-bad-answer'), signIn: false })
        return
      }
      setQuants(files)
      const tagged =
        parsed.tag === ''
          ? -1
          : files.findIndex(
              (r) => r.tag.toLowerCase() === parsed.tag.toLowerCase(),
            )
      const at = tagged >= 0 ? tagged : files.findIndex((r) => r.default)
      if (at >= 0) {
        const row = files[at]!
        // The pick writes its value into the field (22 P.9) and leaves the
        // focus on the button: the radios are the answer.
        setQuantAt(at)
        setHfPick(row.value)
        setHfText(row.value['hf-repo'])
      }
      return
    }
    const refusal = refusalOf(reply.body, HF_REFUSALS)
    if (refusal?.field === 'hf') {
      // 400 hf-repo-invalid is the bff's check of the rule the page checked
      // first, so it goes under the field, as that does.
      flushSync(() => setErrors((e) => ({ ...e, hf: refusal.text })))
      document.getElementById(hfId)?.focus()
      return
    }
    setAlert({ text: refusal?.text ?? t('request-failed'), signIn: false })
  }

  /** An edit of the repo field (22 P.11): what the listing gave goes, the pick
   * with it, the status line and the alert. Silently, and no focus moves. */
  function onHfChange(value: string) {
    setHfText(value)
    setQuants(null)
    setQuantAt(null)
    setHfPick(null)
    setStatus('')
    setAlert(null)
    setErrors((prev) =>
      prev.hf === undefined ? prev : { ...prev, hf: undefined },
    )
  }

  /** Checking a quant (22 P.9): its pair is the pick, its `hf-repo` is what the
   * field shows, and the focus stays where the visitor left it. */
  function pickQuant(at: number) {
    const row = quants?.[at]
    if (row === undefined) return
    setQuantAt(at)
    setHfPick(row.value)
    setHfText(row.value['hf-repo'])
  }

  /** Create: the page's checks in their order, the focus on the first field
   * that fails, nothing sent while any fails (22 C.4). */
  function create() {
    if (sendingRef.current) return
    const id = name.trim()
    const next: {
      id?: string
      model?: string
      hf?: string
      from?: string
    } = {}
    if (id === '') next.id = t('field-error-required')
    else if (!NAME_ALLOWED.test(id))
      next.id = t('api-invalid-model-profile-id', id)
    else if (rows.some((r) => r.id.toLowerCase() === id.toLowerCase()))
      next.id = t('api-model-profile-name-taken', id)
    // A file choice with no pick sends nothing, whatever the box holds. The
    // words go under the box only when it has text in them to point at (22 C.4).
    let needsPick = false
    if (source === 'file') {
      if (picked === '') {
        needsPick = true
        if (text.trim() !== '') next.model = t('field-error-pick-file')
      } else if (/[;#]/.test(picked))
        next.model = t('field-error-comment-markers')
    } else if (source === 'hf') {
      // Typed text is never a value: only a picked quant is (22 P.2, C.4).
      if (hfPick === null) next.hf = t('field-error-pick-quant')
    } else if (from === '') {
      next.from = t('field-error-choose')
    }
    const field =
      next.id !== undefined
        ? nameId
        : next.model !== undefined
          ? fileId
          : next.hf !== undefined
            ? hfId
            : next.from !== undefined
              ? copyId
              : null
    if (field !== null) {
      // The error is in the DOM, and names the field, before the focus lands
      // (17b review D1).
      flushSync(() => setErrors(next))
      document.getElementById(field)?.focus()
      return
    }
    if (needsPick) {
      // An empty box is the one field that has to be filled: the focus goes
      // there, its state line already says what the folder holds (22 C.4).
      document.getElementById(fileId)?.focus()
      return
    }
    setErrors({})
    void send(id)
  }

  async function send(id: string) {
    sendingRef.current = true
    setSent(true)
    setAlert(null)
    setStatus(t('create-creating'))
    const body =
      source === 'copy'
        ? { id, from }
        : source === 'hf'
          ? // The create checked the pick (22 C.4), so it is there.
            { id, values: hfPick! }
          : { id, values: { model: picked } }
    let answer: Reply | undefined
    try {
      answer = await call('/api/profiles/model', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      })
    } catch {
      answer = undefined
    }
    sendingRef.current = false
    setSent(false)
    setStatus('')
    const reply = answer
    if (reply === undefined) {
      setAlert({ text: t('request-failed'), signIn: false })
      return
    }
    if (reply.status === 201) {
      // The new profile's page, whose h1 takes the focus (22 C.5a).
      window.location.hash = modelProfileHref(id)
      return
    }
    if (sessionEnded(reply)) {
      // A sign-in from here returns the focus to Create (22 C.7), whatever an
      // earlier read of this form asked for.
      signedInTo.current = 'create'
      setAlert({ text: t('session-ended-retry'), signIn: true })
      return
    }
    if (sessionRevoked(reply)) {
      setAlert({ text: t('session-revoked'), signIn: false })
      return
    }
    const key = apiError(reply.body)?.key
    if (key === 'client-timeout') {
      // It may or may not have been created: the list read again says (22 C.7).
      setAlert({ text: t('create-unknown'), signIn: false })
      onRefresh()
      return
    }
    if (key === 'client-unavailable') {
      setAlert({ text: t('api-client-unavailable'), signIn: false })
      return
    }
    const refusal = refusalOf(reply.body, CREATE_REFUSALS)
    const field = refusal?.field
    // The field the reply names, as this form's own key: the two Hugging Face
    // spellings are the one repo field (22 C.6).
    const at =
      field === 'id'
        ? 'id'
        : field === 'from'
          ? 'from'
          : field === 'model'
            ? 'model'
            : field === 'hf-repo' || field === 'hf-file'
              ? 'hf'
              : null
    const box =
      at === 'id'
        ? nameId
        : at === 'from'
          ? copyId
          : at === 'model'
            ? fileId
            : at === 'hf'
              ? hfId
              : null
    if (refusal !== undefined && at !== null && box !== null) {
      flushSync(() => setErrors({ [at]: refusal.text }))
      document.getElementById(box)?.focus()
      return
    }
    setAlert({
      text:
        refusal?.text ??
        (reply.status === 422 && key !== undefined
          ? t('api-other', key)
          : t('request-failed')),
      signIn: false,
    })
  }

  /** A sign-in in the dialog (22 C.7): the form keeps what was typed and
   * picked, nothing is sent again, and the focus returns to what asked (Create,
   * Show quants, the folder's line). */
  function signedIn(session: Session) {
    const kept = onRenewed(session)
    const to = signedInTo.current
    flushSync(() => {
      setAsking(false)
      if (!kept) return
      if (to === 'folder') setFolder({ state: 'loading' })
      else setAlert(null)
    })
    if (!kept) return
    if (to === 'create') document.getElementById('create-button')?.focus()
    else if (to === 'quants') document.getElementById(showId)?.focus()
    else void readFolder()
  }

  function closeAsking() {
    flushSync(() => setAsking(false))
    document.getElementById('session-sign-in')?.focus()
  }

  return (
    <>
      <form
        aria-labelledby={titleId}
        onSubmit={(e) => {
          e.preventDefault()
          create()
        }}
        onKeyDown={(e) => {
          // Escape closes the form, as Cancel does; not while a create runs.
          if (e.key === 'Escape' && !sendingRef.current) onClose()
        }}
      >
        <h2 id={titleId}>{t('model-profiles-new-title')}</h2>
        <p className="note">{t('model-profiles-new-note')}</p>
        <CreateField
          id={nameId}
          label={t('model-profile-name')}
          hint={t('model-profile-name-hint')}
          error={errors.id}
        >
          <input
            id={nameId}
            type="text"
            className="mono"
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            autoComplete="off"
            value={name}
            onChange={(e) => {
              setName(e.target.value)
              setErrors((prev) =>
                prev.id === undefined ? prev : { ...prev, id: undefined },
              )
            }}
            {...(errors.id === undefined
              ? {}
              : {
                  'aria-invalid': true,
                  'aria-describedby': `${nameId}-error`,
                })}
          />
        </CreateField>
        <fieldset className="field">
          <legend className="field-label">
            {t('model-profile-start-from')}
          </legend>
          <div className="field-control">
            <label htmlFor={fileRadioId}>
              <input
                id={fileRadioId}
                type="radio"
                name={`${uid}-source`}
                checked={source === 'file'}
                onChange={() => setSource('file')}
              />
              {t('model-source-file')}
            </label>
            <label htmlFor={hfRadioId}>
              <input
                id={hfRadioId}
                type="radio"
                name={`${uid}-source`}
                checked={source === 'hf'}
                onChange={() => setSource('hf')}
              />
              {t('model-source-hf')}
            </label>
            {rows.length > 0 && (
              <label htmlFor={copyRadioId}>
                <input
                  id={copyRadioId}
                  type="radio"
                  name={`${uid}-source`}
                  checked={source === 'copy'}
                  onChange={() => setSource('copy')}
                />
                {t('model-source-copy')}
              </label>
            )}
          </div>
        </fieldset>
        <CreateField
          id={fileId}
          label={t('model-profile-model-file')}
          hint={t('model-file-hint')}
          error={errors.model}
          hidden={source !== 'file'}
          after={
            <>
              {/* The suggestions the box points at (22 P.3): a listbox in the
                form's flow under the input, never floating, `hidden` while
                closed. */}
              <ul
                id={listId}
                className="tag-options"
                role="listbox"
                aria-label={t('model-profile-model-file')}
                hidden={!expanded}
              >
                {shown.slice(0, OPTION_MAX).map((row, at) => (
                  <li
                    key={row.path}
                    id={`${listId}-option-${at}`}
                    role="option"
                    aria-selected={row.path === picked}
                    onMouseDown={(e) => {
                      // Chosen as the click ends, so the box never loses it first
                      // (15 C2.4).
                      e.preventDefault()
                    }}
                    onClick={() => pick(row)}
                  >
                    {fileOption(row, dir)}
                  </li>
                ))}
              </ul>
              <p role="status" className="save-status" aria-live="polite">
                {/* The folder's line is the file choice's own: with another
                    choice checked it says nothing (22 P.6, P.11). */}
                {source === 'file' ? folderText : ''}
              </p>
              {canRetry && (
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    setFolder({ state: 'loading' })
                    void readFolder()
                  }}
                >
                  {t('model-files-retry')}
                </button>
              )}
              {folder.state === 'session' && (
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    signedInTo.current = 'folder'
                    setAsking(true)
                  }}
                >
                  {t('session-sign-in')}
                </button>
              )}
            </>
          }
        >
          <input
            id={fileId}
            type="text"
            className="mono"
            role="combobox"
            aria-autocomplete="list"
            aria-controls={listId}
            aria-expanded={expanded}
            {...(activeAt === null
              ? {}
              : { 'aria-activedescendant': `${listId}-option-${activeAt}` })}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            autoComplete="off"
            value={text}
            onChange={(e) => {
              // Typing filters what the folder gave and leaves no option picked
              // (22 P.3).
              setText(e.target.value)
              setPicked('')
              setListOpen(true)
              setActive(null)
              setErrors((prev) =>
                prev.model === undefined ? prev : { ...prev, model: undefined },
              )
            }}
            onKeyDown={onFileKey}
            onClick={() => {
              // A click in the box asks for the list (22 P.3).
              setListOpen(true)
            }}
            onBlur={() => {
              // A box the visitor left has no list open under it (22 P.3). The
              // options keep the focus, so a click on one is no blur.
              setListOpen(false)
            }}
            {...(errors.model === undefined
              ? {}
              : {
                  'aria-invalid': true,
                  'aria-describedby': `${fileId}-error`,
                })}
          />
        </CreateField>
        <CreateField
          id={hfId}
          label={t('model-profile-hf-repo')}
          hint={t('model-profile-hf-repo-hint')}
          error={errors.hf}
          hidden={source !== 'hf'}
          after={
            <button
              id={showId}
              type="button"
              className="secondary"
              aria-disabled={reading ? 'true' : undefined}
              onClick={() => void showQuants()}
            >
              {t('hf-show-quants')}
            </button>
          }
        >
          <input
            id={hfId}
            type="text"
            className="mono"
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            autoComplete="off"
            value={hfText}
            onChange={(e) => onHfChange(e.target.value)}
            {...(errors.hf === undefined
              ? {}
              : {
                  'aria-invalid': true,
                  'aria-describedby': `${hfId}-error`,
                })}
          />
        </CreateField>
        {quants !== null && (
          /* The quants the listing gave (22 P.9): one radio per file, in the
             reply's order, the fieldset named by its legend. An edit of the
             field drops it (22 P.11), and it is closed with the choice. */
          <fieldset className="field hf-quants" hidden={source !== 'hf'}>
            <legend className="field-label">{t('hf-quant-legend')}</legend>
            <div className="field-control">
              {quants.map((row, at) => (
                <label key={row.tag}>
                  <input
                    type="radio"
                    name={quantId}
                    checked={quantAt === at}
                    onChange={() => pickQuant(at)}
                  />
                  {quantLabel(row)}
                </label>
              ))}
            </div>
          </fieldset>
        )}
        {rows.length > 0 && (
          <CreateField
            id={copyId}
            label={t('model-profile-copy-from')}
            hint={t('model-profile-copy-hint')}
            error={errors.from}
            hidden={source !== 'copy'}
          >
            <select
              id={copyId}
              value={from}
              onChange={(e) => {
                setFrom(e.target.value)
                setErrors((prev) =>
                  prev.from === undefined ? prev : { ...prev, from: undefined },
                )
              }}
              {...(errors.from === undefined
                ? {}
                : {
                    'aria-invalid': true,
                    'aria-describedby': `${copyId}-error`,
                  })}
            >
              <option value="">{t('model-profile-copy-choose')}</option>
              {rows.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.id}
                </option>
              ))}
            </select>
          </CreateField>
        )}
        <p role="status" className="save-status" aria-live="polite">
          {status}
        </p>
        {alert !== null && (
          <p role="alert" className="save-alert">
            {alert.text}
          </p>
        )}
        {alert?.signIn && (
          <button
            type="button"
            id="session-sign-in"
            className="secondary"
            onClick={() => {
              // Where the focus returns after a sign-in is what asked: Create
              // for a create, Show quants for a listing (22 C.7, P.10). The
              // read that raised the alert says which.
              setAsking(true)
            }}
          >
            {t('session-sign-in')}
          </button>
        )}
        <div className="save-actions">
          <button
            type="submit"
            id="create-button"
            className="primary"
            aria-disabled={sent || undefined}
          >
            {t('create-button')}
          </button>
          <button
            type="button"
            className="secondary"
            aria-disabled={sent || undefined}
            onClick={onClose}
          >
            {t('create-cancel')}
          </button>
        </div>
      </form>
      {asking && (
        <Dialog title={t('signin-title')} onClose={closeAsking}>
          <SignIn
            required={false}
            inDialog
            onSignedIn={signedIn}
            onCancel={closeAsking}
          />
        </Dialog>
      )}
    </>
  )
}

/** The folder's one state line's text (22 P.6): one of the states, the walk's
 * own truncation said after the count. */
function folderTextOf(folder: Folder, matching: number): string {
  switch (folder.state) {
    case 'loading':
      return t('model-files-loading')
    case 'empty':
      return t('model-files-empty', folder.dir)
    case 'unreadable':
      return t('model-files-unreadable', folder.arg)
    case 'failed':
      return t('model-files-failed')
    case 'session':
      return t('session-ended-retry')
    case 'ready': {
      const one =
        matching > OPTION_MAX
          ? t('model-files-more', String(OPTION_MAX), String(matching))
          : matching === 1
            ? t('model-files-count-one')
            : matching === 0
              ? t('model-files-no-match')
              : t('model-files-count', String(matching))
      return folder.truncated ? `${one} ${t('model-files-truncated')}` : one
    }
  }
}

/** One field of the create form (fe/DESIGN.md 5.2): its label with the About
 * toggle, the hint behind it, the control, what the control's own line adds
 * under it, and the refusal's text last. `hidden` keeps the field of the choice
 * that is not checked mounted, so what was typed in it stays (22 P.1). */
function CreateField({
  id,
  label,
  hint,
  error,
  hidden = false,
  after,
  children,
}: {
  id: string
  label: string
  hint: string
  /** The refusal's text under the control, when there is one. */
  error: string | undefined
  hidden?: boolean
  /** What the control adds under it: the listbox and the folder's line. */
  after?: ReactNode
  children: ReactNode
}) {
  const [open, setOpen] = useState(false)
  const hintId = `${id}-hint`
  return (
    <div className="field" hidden={hidden}>
      <div className="field-label">
        <label htmlFor={id}>{label}</label>
        {/* The toggle of Field.tsx's About, with its name built from the label
            as an `aria-label` instead of a hidden span: the form's field names
            the control and nothing else carries that text, so a reader and a
            test find the label once (22 C.2, 22 C.8). */}
        <button
          type="button"
          className="about"
          aria-expanded={open}
          aria-controls={hintId}
          aria-label={`${t('field-about')} ${label}`}
          onClick={() => setOpen((o) => !o)}
        >
          {t('field-about')}
        </button>
      </div>
      <Hint id={hintId} text={hint} open={open} />
      <div className="field-control">{children}</div>
      {/* What the control adds goes in one element of its own, so the 720 px
          field rule places the whole of it in the control column: left as the
          field's own children, the list and the line fell into the 200 px
          label column (22C1 review D1, as 19 LS1 D1 and 21 TP1 D1). */}
      {after !== undefined && <div className="field-after">{after}</div>}
      {error !== undefined && <FieldError id={`${id}-error`} text={error} />}
    </div>
  )
}
