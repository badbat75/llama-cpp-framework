// A preset's page (#/models/<id>, signed in): the way back to the list, the id
// as the heading, and the preset read once each time the page opens from
// GET /api/presets/<id>, or the message saying why it is not there
// (fe/briefs/02-models.md, sections 2d.1 to 2d.4). Below the heading, the
// Engine field, the Assets card as text and the cards of the settings
// (fe/briefs/02e-preset-cards.md, section 1). The Embeddings card's two
// controls, the Resource / context card's number fields and the KV cache
// card's controls are editable, and the save bar at the end of the
// settings writes what changed (fe/briefs/02h-preset-save.md, sub-slices 2h.1,
// 2h.4 and 2h.5, criteria 130 to 149, 155 to 157 and 164, read by key in
// save.ts as the Server page reads its own); every other control stays
// disabled, and its settings are changed in pai-config.
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
import Card from './Card.tsx'
import { DiscardDialog } from './Dialog.tsx'
import { SelectField } from './Field.tsx'
import PageHeading from './PageHeading.tsx'
import PresetCards, {
  checkRows,
  checkboxRows,
  checkboxSend,
  checkboxState,
  editableRows,
  fieldId,
  numberError,
  numberRows,
  numberSend,
  numberStates,
  selectSend,
  selectState,
  textError,
  textRows,
  textSend,
} from './PresetCards.tsx'
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
import { presetIdUnreadable } from './route.ts'
import { SignInDialog, useSignInDialog } from './SignIn.tsx'
import type { Session } from './session.ts'
import { t } from './strings.ts'

/** One preset as the read gives it, the shape of criterion 81 with the
 * `revision` 2h.1's 131 adds: an opaque string the save sends back. */
type PresetDoc = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
  values: Record<string, string>
  projectUrl: string
  revision: string
}

/** What the read gave: the preset, the key of its message, signed out. */
type Answer =
  | { doc: PresetDoc }
  | {
      error:
        'client-not-authorized' | 'client-unavailable' | 'not-found' | 'failed'
    }
  | 'signed-out'
  | 'revoked'

function isPresetDoc(value: unknown): value is PresetDoc {
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

/** The `spec-type` entries, split as `presets.rs` does: on `,`, trimmed, the
 * empty ones and `none` dropped. */
function specTypeEntries(specType: string): string[] {
  return specType
    .split(',')
    .map((entry) => entry.trim())
    .filter((entry) => entry !== '' && entry !== 'none')
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
function engineOption(doc: PresetDoc): string {
  return doc.engineAvailable
    ? doc.engine
    : t('presets-engine-not-available', doc.engine)
}

/** Not a strings key: an acronym, as the GUI labels it. */
const MMPROJ = 'MMProj'

/** The Assets card's pairs, in the GUI's order, values as the file holds them. */
function assetRows(doc: PresetDoc): { label: string; value: string }[] {
  const values = doc.values
  const specType = values['spec-type'] ?? ''
  return [
    { label: t('preset-model-file'), value: values['model'] ?? '' },
    { label: t('preset-project-url'), value: doc.projectUrl },
    { label: MMPROJ, value: values['mmproj'] ?? '' },
    { label: t('preset-draft-model'), value: values['model-draft'] ?? '' },
    { label: t('preset-drafter'), value: specTypeDrafter(specType) },
    { label: t('preset-ngram-drafters'), value: specTypeNgrams(specType) },
  ]
}

async function readPreset(id: string): Promise<Answer> {
  try {
    const reply = await call(`/api/presets/${encodeURIComponent(id)}`)
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      const key = apiError(reply.body)?.key
      switch (key) {
        case 'client-not-authorized':
        case 'client-unavailable':
          return { error: key }
        case 'no-preset-named':
          return { error: 'not-found' }
        default:
          return { error: 'failed' }
      }
    }
    return isPresetDoc(reply.body) ? { doc: reply.body } : { error: 'failed' }
  } catch {
    return { error: 'failed' }
  }
}

export default function PresetPage({
  id,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  /** The id from the address, while the read has not answered. */
  id: string
  onSignedOut: (byManager: boolean) => void
  /** A sign-in in the bar's dialog: true when the edits belong to it and stay. */
  onRenewed: (session: Session) => boolean
  /** Told whether the form holds unsaved changes; false when it goes. */
  onDirty: (dirty: boolean) => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // Raised by the bar's Reload (145): `reads` starts the read again, `asked`
  // holds the preset the read was asked for.
  const [reads, setReads] = useState(0)
  const asked = useRef<string | null>(null)
  // The sign-ins in the bar's dialog (152): `rereads` remounts the form and
  // reads the preset again for the new session, `dropped` says the edits went
  // with it, `kept` counts the sign-ins that kept the page (ends the alert).
  const [signIns, setSignIns] = useState({
    rereads: 0,
    dropped: false,
    kept: 0,
  })
  // Whether the form on screen has edits to lose (152).
  const dirty = useRef(false)

  useEffect(() => {
    // `.` and `..` are sent to no read at all (59a): the page says there is no
    // such preset, as a 404 would.
    if (presetIdUnreadable(id)) return
    // The read a Reload asked for is the one asked for THIS preset: the mark is
    // spent here, so it cannot outlive the preset that asked for it (145).
    const reload = asked.current === id
    asked.current = null
    let live = true
    readPreset(id).then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked') {
        onSignedOut(a === 'revoked')
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

  /** Reads the preset again, the edits discarded: the settings give place to
   * the page's loading status until the read lands (145). */
  function reload() {
    asked.current = id
    setAnswer(undefined)
    setReads((n) => n + 1)
  }

  /** A sign-in in the bar's dialog (152): the form stays for the same user;
   * for another one its edits are dropped and the preset is read again, and
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
      <p className="preset-back">
        <a href="#/models">{t('preset-back')}</a>
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

/** The page's content: the loaded preset, or the message. */
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
  if (presetIdUnreadable(id)) {
    return <p>{t('preset-not-found', id)}</p>
  }
  if (answer === undefined || answer === 'signed-out' || answer === 'revoked') {
    return <p role="status">{t('preset-loading')}</p>
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
    return <p>{t('preset-not-found', id)}</p>
  }
  return <p>{t('preset-load-failed')}</p>
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

/** The check keys a preset save answers (2h's 150): the text of each, and the
 * field each names when the reply does not. One literal t() call per key. */
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
      case 'preset-no-model':
        return t('api-preset-no-model', ...args)
      case 'override-no-pattern':
        return t('api-override-no-pattern', ...args)
      case 'override-no-device':
        return t('api-override-no-device', ...args)
      case 'invalid-preset-id':
        return t('api-invalid-preset-id', ...args)
      case 'preset-name-taken':
        return t('api-preset-name-taken', ...args)
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
      case 'preset-no-model':
        return 'model'
      case 'override-no-pattern':
      case 'override-no-device':
        return 'override-tensor'
      default:
        return undefined
    }
  },
}

/** The preset save's keys, beside the shared ones `saveAnswer` reads (145, 146).
 * The 404 text takes the id of the page: the bff sends empty `args` for an id no
 * preset can carry, and the id it echoes is not the one the user came for (146). */
function presetKey(key: string, id: string): SaveAnswer | undefined {
  switch (key) {
    case 'preset-changed-elsewhere':
      return { kind: 'changed-elsewhere' }
    case 'no-preset-named':
      return { kind: 'failed', text: t('api-no-preset-named', id) }
    default:
      return undefined
  }
}

/** Writes the changed keys (133) and reads the answer by key. */
async function sendPresetSave(
  id: string,
  revision: string,
  values: Record<string, string | null>,
): Promise<SaveAnswer> {
  let reply
  try {
    reply = await call(`/api/presets/${encodeURIComponent(id)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ revision, values }),
    })
  } catch (e) {
    // The fe's own 120 s limit of a PUT (148, D7).
    return { kind: 'failed', text: sendFailure(e) }
  }
  return saveAnswer(reply, (key) => presetKey(key, id), REFUSALS)
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

/** The loaded preset as a form: the settings of 2e, the two Embeddings
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
  doc: PresetDoc
  /** The edits were dropped by a sign-in as another session (152). */
  dropped: boolean
  /** Raised by every sign-in that kept the page: a session-ended alert goes. */
  renewals: number
  onDirty: (dirty: boolean) => void
  /** Reads the preset again, the edits discarded (145). */
  onReload: () => void
  /** A sign-in in the bar's dialog: true when the edits belong to it and stay. */
  onRenewed: (session: Session) => boolean
}) {
  const fid = useId()
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
  const alerts = useSaveAlert(renewals)
  // The field a save refusal marked (142).
  const fields = useFieldErrors<string>()
  // A second Save while one runs sends nothing (137).
  const saving = useRef(false)
  const bar = useSaveBarPadding()
  // The bar's Sign in, and the dialog it opens (151).
  const saveId = 'preset-save'
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

  // The bar's Reload asks before dropping the edits, and the focus goes back
  // to the button when the answer is kept (145).
  const reloadId = `${fid}-reload`
  const askReload = useAsk(dirty, onReload, () =>
    document.getElementById(reloadId)?.focus(),
  )

  const status = phase === 'saving' ? 'saving' : dirty ? 'unsaved' : phase

  /** One control picked: the value it now holds, in the draft. The pick ends
   * the message the last save left in the bar: what is on screen is what the
   * bar speaks of now, and a field picked back to the saved state is clean
   * with nothing said about it (138). */
  function edit(key: string, value: string) {
    setDraft((d) => ({ ...d, [key]: value }))
    // 142: the edit answers the refusal that named this field, its own error
    // only: the bar's alert stays for the rest to read.
    fields.clear(key)
    setPhase('idle')
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
    setPhase('idle')
  }

  /** What the user typed in a number field. The box is unticked, that being the
   * only state in which its input is writable. */
  function textBoxed(key: string, text: string) {
    setBoxed((p) => ({ ...p, [key]: { ticked: false, text } }))
    fields.clear(key)
    setPhase('idle')
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
      const state = checkboxState(row, draft)
      if (state === checkboxState(row, loaded.values)) continue
      values[row.key] = checkboxSend(row, state)
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
    // 157, 166: every enabled number field whose box is unticked, and every
    // text field with a text that is not empty, is checked, changed or not
    // (the manager checks the whole section anyway). A failure marks its
    // field, tells the bar, and puts the focus on the first failing control
    // in the order the fields are on screen: nothing is sent, and the bar
    // keeps saying what is not saved.
    const failures = checkRows()
      .map(
        (row) =>
          [
            row.key,
            row.kind === 'number'
              ? numberError(row, boxed[row.key])
              : textError(row, draft[row.key] ?? ''),
          ] as const,
      )
      .filter((one): one is [string, string] => one[1] !== undefined)
    if (failures.length > 0) {
      // Rendered first, so the focused control is announced with its error.
      flushSync(() => {
        alerts.setAlert({ text: t('save-invalid') })
        fields.set(Object.fromEntries(failures))
      })
      focusField(failures[0]![0])
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
    const answer = await sendPresetSave(doc.id, loaded.revision, values)
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
          text: t('api-preset-changed-elsewhere'),
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
        // Rendered first, so the focused control is announced with its error.
        flushSync(() => {
          alerts.setAlert({ text: t('save-refused', answer.text) })
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
   * the preset again and this form goes with it. */
  function signedIn(session: Session) {
    if (!onRenewed(session)) return
    alerts.setAlert(null)
    askSignIn.close(saveId)
  }

  /** Discard: the controls back to the loaded state, the alert and the status
   * cleared, focus untouched (136). */
  function discard() {
    if (saving.current || !dirty) return
    setDraft(loaded.values)
    setBoxed(numberStates(loaded.values))
    alerts.setAlert(null)
    fields.set({})
    setPhase('idle')
  }

  return (
    <>
      {!doc.enabled && <p>{t('preset-off')}</p>}
      <section>
        <h2>{t('preset-settings-title')}</h2>
        {dropped && (
          <p role="alert" className="alert">
            {t('server-edits-dropped')}
          </p>
        )}
        <p className="notice">{t('preset-partly-editable')}</p>
        {!doc.engineAvailable && (
          <p className="notice">{t('preset-engine-unavailable', doc.engine)}</p>
        )}
        <SelectField
          id="preset-engine"
          label={t('preset-engine')}
          hint={t('preset-engine-hint')}
          options={[{ value: doc.engine, text: engineOption(doc) }]}
          value={doc.engine}
          onChange={() => undefined}
        />
        <Card title={t('preset-assets')}>
          <dl className="assets">
            {assetRows(doc).map((row) => (
              <Fragment key={row.label}>
                <dt>{row.label}</dt>
                <dd className={row.value === '' ? 'muted' : 'mono'}>
                  {row.value === '' ? t('preset-value-none') : row.value}
                </dd>
              </Fragment>
            ))}
          </dl>
        </Card>
        <PresetCards
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
