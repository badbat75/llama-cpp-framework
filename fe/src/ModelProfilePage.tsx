// A model profile's page (#/profiles/model/<id>, signed in): the way back to
// the list, the id as the heading, and the model profile read once each time
// the page opens from GET /api/profiles/model/<id>, or the message saying why
// it is not there (fe/briefs/02-models.md, sections 2d.1 to 2d.4). Below the
// heading, the summary line naming the engine and the model file (H11a.1),
// the Assets card as text behind a disclosure in its own heading (H11a.3), its
// first row the Engine (H11b.1), and the cards of the settings
// (fe/briefs/02e-model-profile-cards.md, section 1). The Embeddings card's two
// controls, the Resource / context card's number fields, the KV cache
// card's controls, the Vision card's controls and the Hardware card's rows are
// editable (the two MoE rows of the last stay read-only), and
// the save bar at the end of the settings writes what changed
// (fe/briefs/02h-model-profile-save.md, sub-slices 2h.1, 2h.4, 2h.5, 2h.7 and 2h.8,
// criteria 130 to 149, 155 to 157, 164, 175 to 182 and 183 to 188, read by key in
// save.ts as the Server page reads its own); every other control stays
// disabled, and its settings are changed in pai-config. The jump list under the
// h2 names the cards and moves the focus to one of them, its list opened below
// 1280 px by a disclosure (fe/briefs/08-hearth-h10-h11.md, H10), the jump to
// Assets opening that card (H11a.6), and a save refusal
// naming a field that takes no focus says what it is and which card it sits on
// (fe/briefs/02h-model-profile-save.md, sub-slice 2h.10, criteria 194 to 198a).
import {
  Fragment,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react'
import { flushSync } from 'react-dom'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import { tickBox, type Boxed } from './boxed.ts'
import { DiscardDialog } from './Dialog.tsx'
import { Icon } from './Icon.tsx'
import PageHeading from './PageHeading.tsx'
import ModelProfileCards, {
  cardHeadingId,
  cardJumps,
  checkRows,
  checkboxDirty,
  checkboxError,
  checkboxRows,
  checkboxSend,
  checkboxState,
  deviceRows,
  deviceSend,
  deviceState,
  editableRows,
  fieldId,
  fieldPlace,
  numberError,
  numberRows,
  numberSend,
  numberStates,
  ruleLocked,
  selectSend,
  selectState,
  specTypeEntries,
  textError,
  textRows,
  textSend,
  unlockControl,
} from './ModelProfileCards.tsx'
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
import { modelProfileIdUnreadable } from './route.ts'
import { SignInDialog, useSignInDialog } from './SignIn.tsx'
import type { Session, SignedOut } from './session.ts'
import { t } from './strings.ts'

/** One model profile as the read gives it, the shape of criterion 81 with the
 * `revision` 2h.1's 131 adds: an opaque string the save sends back. */
type ModelProfileDoc = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
  values: Record<string, string>
  projectUrl: string
  revision: string
}

/** What the read gave: the model profile, the key of its message, signed out. */
type Answer =
  | { doc: ModelProfileDoc }
  | {
      error:
        'client-not-authorized' | 'client-unavailable' | 'not-found' | 'failed'
    }
  | 'signed-out'
  | 'revoked'

function isModelProfileDoc(value: unknown): value is ModelProfileDoc {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.engine === 'string' &&
    typeof value.engineAvailable === 'boolean' &&
    typeof value.enabled === 'boolean' &&
    typeof value.projectUrl === 'string' &&
    typeof value.revision === 'string' &&
    isRecord(value.values) &&
    Object.values(value.values).every((v) => typeof v === 'string')
  )
}

/** The Drafter: the first entry that is not an n-gram one. */
function specTypeDrafter(specType: string): string {
  return (
    specTypeEntries(specType).find((entry) => !entry.startsWith('ngram-')) ?? ''
  )
}

/** The N-gram drafters: the `ngram-` entries, in the value's order. */
function specTypeNgrams(specType: string): string {
  return specTypeEntries(specType)
    .filter((entry) => entry.startsWith('ngram-'))
    .join(', ')
}

/** The label the option carries: the engine, marked when it is not installed. */
function engineOption(doc: ModelProfileDoc): string {
  return doc.engineAvailable
    ? doc.engine
    : t('model-profiles-engine-not-available', doc.engine)
}

/** Not a strings key: an acronym, as the GUI labels it. */
const MMPROJ = 'MMProj'

/** The model file for the summary line (H11a.1): the last non-empty segment of
 * the path, split on `/` and `\`, so a Windows path and a POSIX one read the
 * same. Empty when the key is absent, empty, or every segment is: the line
 * then says there is no model file. */
function modelFileName(values: Record<string, string>): string {
  return (
    (values['model'] ?? '')
      .split(/[/\\]/)
      .filter((part) => part !== '')
      .pop() ?? ''
  )
}

/** One Assets row. `path` is false for the engine id, which is not a path and
 * so sits in the body font, with no mono (H11b.1). */
type AssetRow = { label: string; value: string; path: boolean }

/** A row whose value is a path, the class of 2d.74. */
function pathRow(label: string, value: string): AssetRow {
  return { label, value, path: true }
}

/** The value's class: a path is mono, an empty value is the muted "(none)",
 * and a value that is neither carries none. */
function assetClass(row: AssetRow): string | undefined {
  return row.path ? (row.value === '' ? 'muted' : 'mono') : undefined
}

/** The Assets card's pairs, in the GUI's order, values as the file holds them.
 * The Engine row leads (H11b.1: the Engine field of 70 became this row). */
function assetRows(doc: ModelProfileDoc): AssetRow[] {
  const values = doc.values
  const specType = values['spec-type'] ?? ''
  return [
    { label: t('model-profile-engine'), value: engineOption(doc), path: false },
    pathRow(t('model-profile-model-file'), values['model'] ?? ''),
    pathRow(t('model-profile-project-url'), doc.projectUrl),
    pathRow(MMPROJ, values['mmproj'] ?? ''),
    pathRow(t('model-profile-draft-model'), values['model-draft'] ?? ''),
    pathRow(t('model-profile-drafter'), specTypeDrafter(specType)),
    pathRow(t('model-profile-ngram-drafters'), specTypeNgrams(specType)),
  ]
}

async function readModelProfile(id: string): Promise<Answer> {
  try {
    const reply = await call(`/api/profiles/model/${encodeURIComponent(id)}`)
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      const key = apiError(reply.body)?.key
      switch (key) {
        case 'client-not-authorized':
        case 'client-unavailable':
          return { error: key }
        case 'no-model-profile-named':
          return { error: 'not-found' }
        default:
          return { error: 'failed' }
      }
    }
    return isModelProfileDoc(reply.body)
      ? { doc: reply.body }
      : { error: 'failed' }
  } catch {
    return { error: 'failed' }
  }
}

export default function ModelProfilePage({
  id,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  /** The id from the address, while the read has not answered. */
  id: string
  onSignedOut: (reason: SignedOut) => void
  /** A sign-in in the bar's dialog: true when the edits belong to it and stay. */
  onRenewed: (session: Session) => boolean
  /** Told whether the form holds unsaved changes; false when it goes. */
  onDirty: (dirty: boolean) => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // Raised by the bar's Reload (145): `reads` starts the read again, `asked`
  // holds the model profile the read was asked for.
  const [reads, setReads] = useState(0)
  const asked = useRef<string | null>(null)
  // The sign-ins in the bar's dialog (152): `rereads` remounts the form and
  // reads the model profile again for the new session, `dropped` says the edits
  // went with it, `kept` counts the sign-ins that kept the page (ends the alert).
  const [signIns, setSignIns] = useState({
    rereads: 0,
    dropped: false,
    kept: 0,
  })
  // Whether the form on screen has edits to lose (152).
  const dirty = useRef(false)

  useEffect(() => {
    // `.` and `..` are sent to no read at all (59a): the page says there is no
    // such model profile, as a 404 would.
    if (modelProfileIdUnreadable(id)) return
    // The read a Reload asked for is the one asked for THIS model profile: the
    // mark is spent here, so it cannot outlive the model profile that asked
    // for it (145).
    const reload = asked.current === id
    asked.current = null
    let live = true
    readModelProfile(id).then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked') {
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
        return
      }
      // A read a Reload asked for answers the page like a first visit: the
      // message its key gives (145).
      setAnswer(a)
      if (reload) document.getElementById('page-heading')?.focus()
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut, reads])

  const trackDirty = useCallback(
    (d: boolean) => {
      dirty.current = d
      onDirty(d)
    },
    [onDirty],
  )

  /** Reads the model profile again, the edits discarded: the settings give place to
   * the page's loading status until the read lands (145). */
  function reload() {
    asked.current = id
    setAnswer(undefined)
    setReads((n) => n + 1)
  }

  /** A sign-in in the bar's dialog (152): the form stays for the same user;
   * for another one its edits are dropped and the model profile is read again, and
   * the app gives the focus to the h1. */
  function renewed(session: Session) {
    const dropped = dirty.current
    const kept = onRenewed(session)
    setSignIns((s) =>
      kept
        ? { ...s, kept: s.kept + 1 }
        : { ...s, rereads: s.rereads + 1, dropped: s.dropped || dropped },
    )
    if (!kept) {
      setAnswer(undefined)
      setReads((n) => n + 1)
    }
    return kept
  }

  // The heading is the id from the address, then the file's spelling as the
  // read gave it (the bff matches ids case-insensitively).
  const shown =
    answer !== undefined && typeof answer === 'object' && 'doc' in answer
      ? answer.doc.id
      : id
  return (
    <>
      <p className="model-profile-back">
        <a href="#/profiles/model">
          <Icon name="chevron-left" />
          {t('model-profile-back')}
        </a>
      </p>
      <PageHeading text={shown} />
      <Body
        key={signIns.rereads}
        id={id}
        answer={answer}
        dropped={signIns.dropped}
        renewals={signIns.kept}
        onDirty={trackDirty}
        onReload={reload}
        onRenewed={renewed}
      />
    </>
  )
}

/** The page's content: the loaded model profile, or the message. */
function Body({
  id,
  answer,
  dropped,
  renewals,
  onDirty,
  onReload,
  onRenewed,
}: {
  id: string
  answer: Answer | undefined
  /** The edits were dropped by a sign-in as another session (152). */
  dropped: boolean
  /** Raised by every sign-in that kept the page: a session-ended alert goes. */
  renewals: number
  onDirty: (dirty: boolean) => void
  onReload: () => void
  onRenewed: (session: Session) => boolean
}) {
  if (modelProfileIdUnreadable(id)) {
    return <p>{t('model-profile-not-found', id)}</p>
  }
  if (answer === undefined || answer === 'signed-out' || answer === 'revoked') {
    return <p role="status">{t('model-profile-loading')}</p>
  }
  if ('doc' in answer)
    return (
      <Content
        doc={answer.doc}
        dropped={dropped}
        renewals={renewals}
        onDirty={onDirty}
        onReload={onReload}
        onRenewed={onRenewed}
      />
    )
  if (answer.error === 'client-not-authorized') {
    return <p>{t('api-client-not-authorized')}</p>
  }
  if (answer.error === 'client-unavailable') {
    return <p>{t('api-client-unavailable')}</p>
  }
  if (answer.error === 'not-found') {
    return <p>{t('model-profile-not-found', id)}</p>
  }
  return <p>{t('model-profile-load-failed')}</p>
}

/** What a save answered: the values and the revision the form is dirty
 * against now, and what the bar says about it. */
type Loaded = { revision: string; values: Record<string, string> }

/** The values of the read with the sent keys applied: a key sent as `null` is
 * gone (138), and the rest stay as the read gave them. */
function applied(
  values: Record<string, string>,
  sent: Record<string, string | null>,
): Record<string, string> {
  const next = { ...values }
  for (const [key, value] of Object.entries(sent)) {
    if (value === null) delete next[key]
    else next[key] = value
  }
  return next
}

/** The check keys a model profile save answers (2h's 150): the text of each,
 * and the field each names when the reply does not. One literal t() call per
 * key. */
const REFUSALS: RefusalTable = {
  text: (key, args) => {
    switch (key) {
      case 'not-whole-number-in-range':
        return t('api-not-whole-number-in-range', ...args)
      case 'comment-markers':
        return t('api-comment-markers', ...args)
      case 'not-a-number':
        return t('api-not-a-number', ...args)
      case 'not-one-of':
        return t('api-not-one-of', ...args)
      case 'not-json-object':
        return t('api-not-json-object', ...args)
      case 'min-above-max':
        return t('api-min-above-max', ...args)
      case 'fit-target-format':
        return t('api-fit-target-format', ...args)
      case 'moe-cache-with-tensor-split':
        return t('api-moe-cache-with-tensor-split', ...args)
      case 'load-on-startup-over-max':
        return t('api-load-on-startup-over-max', ...args)
      case 'load-on-startup-unlimited':
        return t('api-load-on-startup-unlimited', ...args)
      case 'model-profile-no-model':
        return t('api-model-profile-no-model', ...args)
      case 'override-no-pattern':
        return t('api-override-no-pattern', ...args)
      case 'override-no-device':
        return t('api-override-no-device', ...args)
      case 'invalid-model-profile-id':
        return t('api-invalid-model-profile-id', ...args)
      case 'model-profile-name-taken':
        return t('api-model-profile-name-taken', ...args)
      case 'model-profile-over-memory':
        return t('api-model-profile-over-memory', ...args)
      case 'model-profile-model-missing':
        return t('api-model-profile-model-missing', ...args)
      default:
        return undefined
    }
  },
  field: (key, args) => {
    switch (key) {
      case 'not-whole-number-in-range':
      case 'comment-markers':
      case 'not-a-number':
      case 'not-one-of':
      case 'not-json-object':
      case 'min-above-max':
        return args[0]
      case 'fit-target-format':
        return 'fit-target'
      case 'moe-cache-with-tensor-split':
        return 'moe-cache-mib'
      case 'load-on-startup-over-max':
      case 'load-on-startup-unlimited':
        return 'load-on-startup'
      case 'model-profile-no-model':
        return 'model'
      case 'override-no-pattern':
      case 'override-no-device':
        return 'override-tensor'
      default:
        return undefined
    }
  },
}

/** The model profile save's keys, beside the shared ones `saveAnswer` reads
 * (145, 146). The 404 text takes the id of the page: the bff sends empty `args`
 * for an id no model profile can carry, and the id it echoes is not the one the
 * user came for (146). */
function modelProfileKey(key: string, id: string): SaveAnswer | undefined {
  switch (key) {
    case 'model-profile-changed-elsewhere':
      return { kind: 'changed-elsewhere' }
    case 'no-model-profile-named':
      return { kind: 'failed', text: t('api-no-model-profile-named', id) }
    default:
      return undefined
  }
}

/** Writes the changed keys (133) and reads the answer by key. */
async function sendModelProfileSave(
  id: string,
  revision: string,
  values: Record<string, string | null>,
): Promise<SaveAnswer> {
  let reply
  try {
    reply = await call(`/api/profiles/model/${encodeURIComponent(id)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ revision, values }),
    })
  } catch (e) {
    // The fe's own 120 s limit of a PUT (148, D7).
    return { kind: 'failed', text: sendFailure(e) }
  }
  return saveAnswer(reply, (key) => modelProfileKey(key, id), REFUSALS)
}

/** The control with the given id, when this page renders it. */
function controlWithId(
  id: string,
): HTMLInputElement | HTMLSelectElement | null {
  const control = document.getElementById(id)
  return control instanceof HTMLInputElement ||
    control instanceof HTMLSelectElement
    ? control
    : null
}

/** The control of the field a refusal names, when this page renders it: a key
 * of the Assets card, or one 2h shows nowhere, has no control here (143). */
function fieldControl(
  key: string,
): HTMLInputElement | HTMLSelectElement | null {
  return controlWithId(`${fieldId(key)}-control`)
}

/** Puts the focus on the marked field's control (142). A disabled control
 * takes no focus, so the field's next one is tried: the default box, first in
 * the field (144). With no enabled control the focus stays where it is. */
function focusField(key: string) {
  const id = fieldId(key)
  for (const control of [
    controlWithId(`${id}-control`),
    controlWithId(`${id}-box`),
  ]) {
    if (control !== null && !control.disabled) {
      control.focus()
      return
    }
  }
}

/** Whether the field of a key can be worked on: its control is enabled, or the
 *  box that ticks it on is. A field the page cannot edit has no control and no
 *  box at all, so nothing to mark and nowhere to go. */
function fieldTakesFocus(key: string): boolean {
  const id = fieldId(key)
  return [controlWithId(`${id}-control`), controlWithId(`${id}-box`)].some(
    (control) => control !== null && !control.disabled,
  )
}

/** The rows the save of its own check reads: the kind `checkRows` answers. */
type CheckedRow = ReturnType<typeof checkRows>[number]

/** Puts the focus where a check of the save sends the user (142, and 176's
 * "what the page sends, it checks"): the marked field's control when the field
 * is enabled, and for a field a rule of 176 locks the control that rule
 * follows, the one control that unlocks it. A disabled control takes no focus,
 * so with no enabled control the focus stays where it is. */
function focusFailure(row: CheckedRow, locked: boolean) {
  if (locked) {
    const id = unlockControl(row)
    const control = id === undefined ? null : controlWithId(id)
    if (control !== null && !control.disabled) {
      control.focus()
      return
    }
  }
  focusField(row.key)
}

/** The loaded model profile as a form: the settings of 2e, the two Embeddings
 * controls of 2h.1, the number fields of 2h.4 and the KV cache card of 2h.5
 * editable, and the save bar of the Server page writing the changed keys
 * (criteria 130 to 141, 155 to 157, 164). */
function Content({
  doc,
  dropped,
  renewals,
  onDirty,
  onReload,
  onRenewed,
}: {
  doc: ModelProfileDoc
  /** The edits were dropped by a sign-in as another session (152). */
  dropped: boolean
  /** Raised by every sign-in that kept the page: a session-ended alert goes. */
  renewals: number
  onDirty: (dirty: boolean) => void
  /** Reads the model profile again, the edits discarded (145). */
  onReload: () => void
  /** A sign-in in the bar's dialog: true when the edits belong to it and stay. */
  onRenewed: (session: Session) => boolean
}) {
  const fid = useId()
  // H11a.1: the model file the summary line names, as the read gave it.
  const fileName = modelFileName(doc.values)
  const [loaded, setLoaded] = useState<Loaded>({
    revision: doc.revision,
    values: doc.values,
  })
  const [draft, setDraft] = useState<Record<string, string>>(doc.values)
  // The box state of the number fields the form edits (156): the text a field
  // holds is kept while its box is ticked, so unticking gives it back.
  const [boxed, setBoxed] = useState<Record<string, Boxed>>(() =>
    numberStates(doc.values),
  )
  const [phase, setPhase] = useState<
    'idle' | 'saving' | 'saved' | 'saved-restart'
  >('idle')
  // H10.2 (fe/briefs/08-hearth-h10-h11.md): the jump list is closed every time
  // the form mounts, and a save keeps the state it is in: no storage, no
  // memory of a page that is gone.
  const [jumpOpen, setJumpOpen] = useState(false)
  // H11a.4 (fe/briefs/08-hearth-h10-h11.md): the Assets card is closed every
  // time the form mounts, on the same rule as the jump list: no storage for a
  // view state, and a save keeps the state it is in.
  const [assetsOpen, setAssetsOpen] = useState(false)
  // The field a save refusal marked (142).
  const fields = useFieldErrors<string>()
  // A second Save while one runs sends nothing (137).
  const saving = useRef(false)
  const bar = useSaveBarPadding()
  // The bar's Sign in, and the dialog it opens (151).
  const saveId = 'model-profile-save'
  const signInId = `${fid}-sign-in`
  const askSignIn = useSignInDialog(signInId)

  // 132: a field is changed when its control's state is not the state the read
  // gave it, so picking the same option, or the extra option of 87 that a read
  // value became, is no change. The number fields count the same way, so the
  // dirtiness and the body of the save come from one function (157).
  const changes = changed()
  const dirty = Object.keys(changes).length > 0
  useDirtyReport(dirty, onDirty)
  useWarnOnLeave(dirty)
  // The bar's alert, with the rule of 2h.4b (198b, 198c, 198g): it knows the
  // form's dirtiness and whether a save is in flight (the flag, not the
  // status: a pick made while the PUT runs sets the status back to idle), so
  // an alert about edits goes with them.
  const alerts = useSaveAlert(renewals, dirty, phase === 'saving')

  // The bar's Reload asks before dropping the edits, and the focus goes back
  // to the button when the answer is kept (145).
  const reloadId = `${fid}-reload`
  const askReload = useAsk(dirty, onReload, () =>
    document.getElementById(reloadId)?.focus(),
  )

  const status = phase === 'saving' ? 'saving' : dirty ? 'unsaved' : phase

  /** 196: a jump button goes to the card whose title it carries: the card's
   * heading is the target, named by its id, so the page never moves the focus
   * into a field the user has not asked for, and no hash is written. H10.4:
   * the list it was picked from closes first, the close committed to the DOM
   * (flushSync) before the heading is focused and scrolled, so the closed
   * list is not left above a heading already at the top of the page. H11a.6:
   * the card that opens behind a disclosure is opened by the same commit, so
   * the user lands on the content they asked for: a card already open stays
   * open, and the other jumps leave Assets as it is. */
  function jump(id: string) {
    const h3 = document.getElementById(id)
    if (h3 === null) return
    const assets = id === cardHeadingId('assets')
    flushSync(() => {
      setJumpOpen(false)
      if (assets) setAssetsOpen(true)
    })
    h3.focus()
    h3.scrollIntoView()
  }

  /** One control picked: the value it now holds, in the draft. The pick ends
   * the message the last save left in the bar: what is on screen is what the
   * bar speaks of now, and a field picked back to the saved state is clean
   * with nothing said about it (138). */
  function edit(key: string, value: string) {
    setDraft((d) => ({ ...d, [key]: value }))
    // 142: the edit answers the refusal that named this field, its own error
    // only: the bar's alert stays for the rest to read.
    fields.clear(key)
    setPhase((p) => (p === 'saving' ? p : 'idle'))
  }

  /** A number field's box (156): the tick keeps the text the input held, and an
   * input that comes back empty takes the focus, so it is written before the
   * rest of the page is read (the mechanics in `boxed.ts`). */
  function tickBoxed(key: string, ticked: boolean) {
    const b = boxed[key]
    if (b === undefined) return
    tickBox(
      b,
      ticked,
      (next) => setBoxed((p) => ({ ...p, [key]: next })),
      fieldControl(key),
      'when-empty',
    )
    fields.clear(key)
    setPhase((p) => (p === 'saving' ? p : 'idle'))
  }

  /** What the user typed in a number field. The box is unticked, that being the
   * only state in which its input is writable. */
  function textBoxed(key: string, text: string) {
    setBoxed((p) => ({ ...p, [key]: { ticked: false, text } }))
    fields.clear(key)
    setPhase((p) => (p === 'saving' ? p : 'idle'))
  }

  /** The changed keys as the body of 133 spells them. A number field with its
   * box ticked is a key to remove, and a key the file never had is sent as
   * nothing; with the box unticked, the field sends the number its text holds
   * (157). A text field is changed when the text it would send differs from
   * the loaded value (a key the file lacks is loaded as nothing): the sent
   * text is the typed text trimmed, so spaces typed into a key the file lacks
   * and a trailing space on an unchanged value leave the form clean, and an
   * emptied text is the key's removal when the file had the key (132, 133,
   * 166). */
  function changed(): Record<string, string | null> {
    const values: Record<string, string | null> = {}
    for (const row of editableRows()) {
      const state = selectState(row, draft)
      if (state === selectState(row, loaded.values)) continue
      values[row.key] = selectSend(row, state)
    }
    for (const row of checkboxRows()) {
      // 2h.8's 185: an invalid `fit` is a state of its own, so a click off it
      // is a change even when the box ends at the state an `off` value shows.
      const state = checkboxDirty(row, draft)
      if (state === checkboxDirty(row, loaded.values)) continue
      values[row.key] = checkboxSend(row, checkboxState(row, draft))
    }
    // 2h.7's 179: a device select at the empty option is the key's removal.
    for (const row of deviceRows()) {
      const state = deviceState(row, draft)
      if (state === deviceState(row, loaded.values)) continue
      values[row.key] = deviceSend(state)
    }
    const was = numberStates(loaded.values)
    for (const row of numberRows()) {
      const b = boxed[row.key]
      if (b === undefined) continue
      if (b.ticked) {
        if (loaded.values[row.key] !== undefined) values[row.key] = null
      } else if (b.text !== was[row.key]?.text) {
        values[row.key] = numberSend(row, b)
      }
    }
    for (const row of textRows()) {
      const send = textSend(draft[row.key] ?? '')
      if (send === (loaded.values[row.key] ?? null)) continue
      values[row.key] = send
    }
    return values
  }

  /** Save changes: the bar of 135 to 149. */
  async function save() {
    if (saving.current) return
    // 157, 166, 185: every enabled number field whose box is unticked, every
    // text field with a text that is not empty, and every editable checkbox
    // holding a value the engine refuses, is checked, changed or not (the
    // manager checks the whole section anyway). 176: a field a rule locks
    // right now is checked too when its draft differs from the loaded value,
    // because the save sends that draft; a locked field that still holds the
    // loaded value is sent as it is and stays unchecked, by the gate that
    // disables it. A failure marks its field, tells the bar, and puts the
    // focus on the first failing control in the order the fields are on
    // screen: for a row a rule locks, on the control the rule follows.
    // Nothing is sent, and the bar keeps saying what is not saved.
    const failures = checkRows()
      .filter(
        (row) =>
          !ruleLocked(row, draft, loaded.values, boxed) || row.key in changes,
      )
      .map(
        (row) =>
          [
            row,
            row.kind === 'number'
              ? numberError(row, boxed[row.key])
              : row.kind === 'text'
                ? textError(row, draft[row.key] ?? '')
                : checkboxError(row, draft),
          ] as const,
      )
      .filter((one): one is [CheckedRow, string] => one[1] !== undefined)
    if (failures.length > 0) {
      // Rendered first, so the focused control is announced with its error.
      flushSync(() => {
        alerts.setAlert({ text: t('save-invalid') })
        fields.set(
          Object.fromEntries(failures.map(([row, error]) => [row.key, error])),
        )
      })
      const [row] = failures[0]!
      focusFailure(row, ruleLocked(row, draft, loaded.values, boxed))
      return
    }
    if (!dirty) return
    const sent = draft
    const sentBoxed = boxed
    const values = changes
    saving.current = true
    setPhase('saving')
    alerts.setAlert(null)
    fields.set({})
    const answer = await sendModelProfileSave(doc.id, loaded.revision, values)
    saving.current = false
    if (answer.kind === 'saved') {
      const next = {
        revision: answer.revision,
        values: applied(loaded.values, values),
      }
      setLoaded(next)
      // What was sent is the clean state now. An edit made during the PUT
      // stays on screen, dirty against the new loaded values (01's 33b). The
      // box memory is reset with it, so a key the save removed comes back
      // empty (156).
      setDraft((d) => (d === sent ? next.values : d))
      setBoxed((b) => (b === sentBoxed ? numberStates(next.values) : b))
      setPhase(answer.running ? 'saved-restart' : 'saved')
      return
    }
    setPhase('idle')
    switch (answer.kind) {
      // The session's answers: the bar's alert with Sign in after it, the
      // edits kept (151, 153).
      case 'signed-out':
        alerts.sessionEnded(false)
        return
      case 'revoked':
        alerts.sessionEnded(true)
        return
      case 'changed-elsewhere':
        alerts.setAlert({
          text: t('api-model-profile-changed-elsewhere'),
          action: 'reload',
        })
        return
      case 'refused': {
        // 142 to 144: a 422 whose key the table knows marks its field and is
        // told in the bar; a field this page does not render is only told there.
        const field =
          answer.field !== undefined && fieldControl(answer.field) !== null
            ? answer.field
            : undefined
        // 198a: a field whose control and box are both disabled keeps
        // the focus on Save changes (144) and its mark on the field: the alert
        // names the field and the card it sits on, so the user knows where.
        const place = field === undefined ? null : fieldPlace(field)
        const at =
          place === null || field === undefined || fieldTakesFocus(field)
            ? null
            : t('save-refused-at', answer.text, place.label, place.card)
        // Rendered first, so the focused control is announced with its error.
        flushSync(() => {
          alerts.setAlert({ text: at ?? t('save-refused', answer.text) })
          if (field !== undefined) fields.set({ [field]: answer.text })
        })
        if (field !== undefined) focusField(field)
        return
      }
      case 'failed':
        alerts.setAlert({ text: answer.text })
    }
  }

  /** A sign-in in the bar's dialog (152): the dialog closes, the alert goes,
   * focus on Save changes, the edits stay. For another session the page reads
   * the model profile again and this form goes with it. */
  function signedIn(session: Session) {
    if (!onRenewed(session)) return
    alerts.setAlert(null)
    askSignIn.close(saveId)
  }

  /** Discard: the controls back to the loaded state, the field marks and the
   * status cleared, focus untouched (136). The bar's alert is left to
   * `useSaveAlert`: Discard is one more route to a clean form, so an alert with
   * an answer button stays and one without goes with the edits (198d). */
  function discard() {
    if (saving.current || !dirty) return
    setDraft(loaded.values)
    setBoxed(numberStates(loaded.values))
    fields.set({})
    setPhase('idle')
  }

  return (
    <>
      {/* H11a.1: the summary line is the first thing under the h1: the engine
          of the read and the model file it points at, as plain text (a long
          file name wraps in the column). It says nothing on its own: it is
          none of the page's live regions. */}
      <p className="model-profile-summary">
        {fileName === ''
          ? t('model-profile-summary-no-model', doc.engine)
          : t('model-profile-summary', doc.engine, fileName)}
      </p>
      {!doc.enabled && <p>{t('model-profile-off')}</p>}
      <section>
        <h2>{t('model-profile-settings-title')}</h2>
        {dropped && (
          <p role="alert" className="alert">
            {t('server-edits-dropped')}
          </p>
        )}
        {/* 196: the jump list is the page's TOC, between the heading and the
            engine Notice (or the Engine field when there is none), a list of
            buttons that move the focus. H10.1: the list is labelled by its own
            p, which names the nav too (the one text of H10.5, shown as the
            label and as the disclosure's), and below 1280 px the label is
            hidden and the disclosure opens and closes the list. */}
        <nav className="jump" aria-labelledby="model-profile-jump-label">
          <p id="model-profile-jump-label" className="jump-label">
            {t('model-profile-jump-label')}
          </p>
          {/* H10.3: a native button, so Enter and Space come with it. The focus
              it is pressed with is kept: the list it opens and closes holds
              nothing the user needs to be moved to. */}
          <button
            type="button"
            id="model-profile-jump-toggle"
            className="jump-toggle secondary"
            aria-controls="model-profile-jump-list"
            aria-expanded={jumpOpen}
            onClick={(e) => {
              e.currentTarget.focus()
              setJumpOpen((open) => !open)
            }}
          >
            {t('model-profile-jump-label')}
            {/* Section 0 of the brief: the chevron follows the text, and the
                stylesheet turns it with the state. */}
            <Icon name="chevron-down" />
          </button>
          {/* H10.1: a closed list carries the class, and the stylesheet hides it
              by that class, never by hidden: the 1280 px rail shows it
              whatever the state is. */}
          <ul
            id="model-profile-jump-list"
            className={jumpOpen ? undefined : 'collapsed'}
          >
            {[
              { id: cardHeadingId('assets'), title: t('model-profile-assets') },
              ...cardJumps(),
            ].map(({ id, title }) => (
              <li key={title}>
                {/* H9a.1: no class, so .jump ul button owns the look of the
                    index entry and no .secondary declaration is overridden. */}
                <button type="button" onClick={() => jump(id)}>
                  {title}
                </button>
              </li>
            ))}
          </ul>
        </nav>
        {/* 194: the Notice of 2h.1 is gone: the read-only fields say so where
            they sit; only the engine Notice of 72 stays. */}
        {!doc.engineAvailable && (
          <p className="notice">
            <Icon name="triangle-alert" />
            {t('model-profile-engine-unavailable', doc.engine)}
          </p>
        )}
        {/* H11a.3: the Assets card is the page's one disclosure: its heading
            holds the button that opens and closes the body, so the h3 stays a
            heading (a details summary would lose the role, section 0) and the
            jump target of 196 keeps its id. The body carries hidden, not a
            class: it has no rail of the 1280 px kind (section 0). */}
        <section className="card assets-card">
          {/* H11a.5: a native button, so Enter and Space come with it, and the
              focus it is pressed with stays on it: the body holds nothing
              focusable, so closing strands nothing. */}
          <h3 id={cardHeadingId('assets')} tabIndex={-1}>
            <button
              type="button"
              id="model-profile-assets-toggle"
              aria-controls="model-profile-assets-body"
              aria-expanded={assetsOpen}
              onClick={(e) => {
                e.currentTarget.focus()
                setAssetsOpen((open) => !open)
              }}
            >
              {t('model-profile-assets')}
              {/* Section 0 of the brief: the chevron follows the text, and the
                  stylesheet turns it with the state. */}
              <Icon name="chevron-down" />
            </button>
          </h3>
          {/* The read-only note of 195 stays outside the body: a closed card
              hides the rows, not the words that say why. */}
          <p className="intro">{t('model-profile-assets-read-only')}</p>
          <div id="model-profile-assets-body" hidden={!assetsOpen}>
            <dl className="assets">
              {assetRows(doc).map((row) => (
                <Fragment key={row.label}>
                  <dt>{row.label}</dt>
                  <dd className={assetClass(row)}>
                    {row.value === ''
                      ? t('model-profile-value-none')
                      : row.value}
                  </dd>
                </Fragment>
              ))}
            </dl>
          </div>
        </section>
        {/* 196: the jump targets are the card headings: every card of the page
            gives its heading the id a jump button reaches it by, and the
            tabindex -1 that keeps it out of the Tab path. */}
        <ModelProfileCards
          values={draft}
          loaded={loaded.values}
          onEdit={edit}
          boxed={boxed}
          onTick={tickBoxed}
          onBoxText={textBoxed}
          errors={fields.errors}
        />
        <SaveBar
          bar={bar}
          status={status}
          alert={alerts.alert}
          dirty={dirty}
          saving={phase === 'saving'}
          reloadId={reloadId}
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
      </section>
      {askReload.asking && (
        <DiscardDialog onKeep={askReload.keep} onDiscard={askReload.confirm} />
      )}
      {askSignIn.asking && (
        <SignInDialog onSignedIn={signedIn} onClose={() => askSignIn.close()} />
      )}
    </>
  )
}
