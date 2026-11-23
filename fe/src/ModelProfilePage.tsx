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
import {
  apiError,
  call,
  isRecord,
  readModelInfo,
  sessionRevoked,
  type ModelInfoRead,
} from './api.ts'
import { boxText, tickBox, type Boxed } from './boxed.ts'
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
import {
  factsRead,
  gpuComposeDevice,
  gpuComposeSplit,
  gpuFileIds,
  gpuFileWeights,
  gpuHas,
  gpuIndexOf,
  gpuLayout,
  gpuNewWeight,
  gpuSplitLoaded,
  gpuWeightFailures,
  readGpuProbe,
  type GpuCardProps,
  type GpuDirection,
  type GpuProbe,
} from './GpuSplit.tsx'
import { setBlocks, type Layout } from './gpuBlocks.ts'
import {
  ADD_ID,
  otNewDevice,
  otNewRule,
  otSetLayersText,
  otTensorId,
  rowRule,
  rowsValue,
  tensorRows,
  type TensorCardProps,
  type TensorRow,
} from './TensorPlacement.tsx'
import { CUSTOM as OT_CUSTOM } from './tensorRules.ts'
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
import { CardBox, Loading, LoadingText } from './Skeleton.tsx'
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
  // Brief 19 (fe/briefs/19-layer-sliders.md), 19 G.1: the model file's layer
  // facts, read on the same arrival as the model profile and the GPUs, and
  // never again: a save, a Discard, an edit and a sign-in send nothing, and
  // nothing polls. The read carries the id it was asked for, so the card shows
  // it only while that model profile is the one on screen: no answer is the read
  // running and its pending line, a read that answered nothing (no session, no
  // manager, a body of no shape, no answer within the GET limit) is the failed
  // line. The read ends no session and writes no alert (decision 1).
  const [facts, setFacts] = useState<{
    id: string
    read: ModelInfoRead
  } | null>(null)

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
    // The model file's layer facts, of the same id, sent beside the profile
    // read and after it (brief 19, 19 G.1): the arrival's reads are the profile,
    // the facts and the GPUs, in this order.
    readModelInfo(id).then((read) => {
      if (live) setFacts({ id, read })
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
        modelInfo={facts?.id === id ? facts.read : undefined}
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
  modelInfo,
  dropped,
  renewals,
  onDirty,
  onReload,
  onRenewed,
}: {
  id: string
  answer: Answer | undefined
  /** The model file's layer facts, read on this arrival (brief 19, 19 G.1). */
  modelInfo: ModelInfoRead | undefined
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
  if (answer === undefined) {
    // The cards' skeleton (17a.4): two cards, the shape of the page's form.
    return (
      <Loading
        text={t('model-profile-loading')}
        shape={
          <>
            <CardBox />
            <CardBox />
          </>
        }
      />
    )
  }
  if (answer === 'signed-out' || answer === 'revoked') {
    return <LoadingText text={t('model-profile-loading')} />
  }
  if ('doc' in answer)
    return (
      <Content
        doc={answer.doc}
        modelInfo={modelInfo}
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
      case 'device-bad-name':
        return t('api-device-bad-name', ...args)
      // Brief 19, 19 G.18: the device the profile names is outside the
      // instance's server profile. The args are [profile, instance, key,
      // device].
      case 'model-profile-device-outside':
        return t('api-model-profile-device-outside', ...args)
      case 'tensor-split-format':
        return t('api-tensor-split-format', ...args)
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
      // Brief 14 GD1.11: the reply names the key, `device`, `device-draft` or
      // `mmproj-device`; the GPUs fieldset speaks for `device`.
      case 'device-bad-name':
        return args[0]
      // Brief 19, 19 G.18: the L4 route names the device select in `args[2]`
      // (its args are [profile, instance, key, device]); the reply's `field`
      // answers for it when it carries one.
      case 'model-profile-device-outside':
        return args[2]
      case 'tensor-split-format':
        return 'tensor-split'
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

/** The GPUs fieldset's checkboxes, in the order they are on screen: the
 * checked rows first, in the split order (brief 14 GD1.5). */
function gpuBoxes(onlyChecked: boolean): HTMLInputElement[] {
  return [
    ...document.querySelectorAll<HTMLInputElement>('input[data-gpu-box]'),
  ].filter((box) => !onlyChecked || box.checked)
}

/** The weight fields that show, in the order they are on screen. */
function gpuWeightFields(): HTMLInputElement[] {
  return [
    ...document.querySelectorAll<HTMLInputElement>('input[data-gpu-weight]'),
  ]
}

/** The controls the field of a GPU key answers to, in the order the focus is
 * put on them (GD1.11, GD2.6, GD2.7): a checked checkbox for `device`, a marked
 * weight field, then any one, then a checked checkbox, then the split mode
 * select, for `tensor-split`. The fieldset has no control of the cards' kind,
 * so the keys name it, not a field of the table. */
function gpuFieldControls(
  key: string,
): (HTMLInputElement | HTMLSelectElement)[] {
  if (key === 'device')
    return [...gpuBoxes(true), ...gpuBoxes(false)].slice(0, 1)
  if (key !== 'tensor-split') return []
  const marked = gpuWeightFields().filter(
    (input) => input.getAttribute('aria-invalid') === 'true',
  )
  const mode = controlWithId(`${fieldId('split-mode')}-control`)
  return [
    ...marked,
    ...gpuWeightFields(),
    ...gpuBoxes(true),
    ...(mode === null ? [] : [mode]),
  ]
}

/** The controls of the field a refusal names, in the order the focus is put on
 * them: the control, the box that ticks it on, and for the GPUs fieldset's two
 * keys, its checkboxes and weight fields. */
function fieldControls(key: string): (HTMLInputElement | HTMLSelectElement)[] {
  const id = fieldId(key)
  const found = [
    controlWithId(`${id}-control`),
    controlWithId(`${id}-box`),
  ].filter((control) => control !== null)
  return found.length === 0 ? gpuFieldControls(key) : found
}

/** The control of the field a refusal names, when this page renders it: a key
 * of the Assets card, or one 2h shows nowhere, has no control here (143). */
function fieldControl(
  key: string,
): HTMLInputElement | HTMLSelectElement | null {
  return fieldControls(key)[0] ?? null
}

/** Puts the focus on the marked field's control (142). A disabled control
 * takes no focus, so the field's next one is tried: the default box, first in
 * the field (144). With no enabled control the focus stays where it is. */
function focusField(key: string) {
  for (const control of fieldControls(key)) {
    if (!control.disabled) {
      control.focus()
      return
    }
  }
}

/** Whether the field of a key can be worked on: its control is enabled, or the
 *  box that ticks it on is. A field the page cannot edit has no control and no
 *  box at all, so nothing to mark and nowhere to go. */
function fieldTakesFocus(key: string): boolean {
  return fieldControls(key).some((control) => !control.disabled)
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

/** A block count as the number a blocks control applies (brief 19, 19 G.6 and
 * 19 G.7): a whole number, trimmed; a slider's value is always one, and a typed
 * text that is not (``, `x`, `2.5`, `-1`) applies nothing. The clamp to the
 * places is `setBlocks`'s. */
function gpuBlockNumber(raw: string): number | null {
  const text = raw.trim()
  return /^\d+$/.test(text) ? Number(text) : null
}

/** The loaded model profile as a form: the settings of 2e, the two Embeddings
 * controls of 2h.1, the number fields of 2h.4 and the KV cache card of 2h.5
 * editable, and the save bar of the Server page writing the changed keys
 * (criteria 130 to 141, 155 to 157, 164). */
function Content({
  doc,
  modelInfo,
  dropped,
  renewals,
  onDirty,
  onReload,
  onRenewed,
}: {
  doc: ModelProfileDoc
  /** The model file's layer facts of this arrival, read by the page (19 G.1). */
  modelInfo: ModelInfoRead | undefined
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
  // The GPU distribution card (brief 14): the host's GPUs, read once per
  // arrival (GD1.3) and never again, the rows' weights (the rows and their
  // checks are the draft's `device`), the weights a check or a refusal marked
  // (`null` is every one that shows, what a refusal of the server does), and
  // what the last move announced (GD1.9).
  const [probe, setProbe] = useState<GpuProbe | null | undefined>(undefined)
  const [weights, setWeights] = useState<Record<string, string>>(() =>
    gpuFileWeights(
      doc.values['tensor-split'] ?? null,
      gpuFileIds(doc.values['device'] ?? null),
    ),
  )
  const [marks, setMarks] = useState<string[] | null>(null)
  const [moved, setMoved] = useState('')
  // The one read of the GPUs, kept out of the double mount of the development
  // StrictMode the way the node read is (App.tsx): the read is not cancelled,
  // so its answer lands.
  const devicesRead = useRef(false)
  useEffect(() => {
    if (devicesRead.current) return
    devicesRead.current = true
    readGpuProbe().then(setProbe)
  }, [])
  // The Tensor placement card (brief 21): the rules of `override-tensor`, one row
  // each, the page's state as the GPU list's rows are, because the card composes
  // the one key out of them.
  const [otRows, setOtRows] = useState<TensorRow[]>(() =>
    tensorRows(doc.values['override-tensor'] ?? null),
  )
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

  /** The rows the GPUs fieldset shows checked, in the split order: the ids the
   * draft's `device` names (GD1.10). */
  function gpuChecked(): string[] {
    return gpuFileIds(draft['device'] ?? null)
  }

  /** Checking or unchecking a row (GD1.8): a checked row joins at the end of
   * the checked rows at the weight GD2.3 gives a new row, an unchecked row
   * takes its weight with it. The row's checkbox keeps the focus: the rows are
   * keyed by id, so nothing is re-created under it. The device error goes, the
   * bar's alert stays (GD1.11). */
  function gpuToggle(id: string) {
    const checked = gpuChecked()
    const on = gpuHas(checked, id)
    edit(
      'device',
      (on
        ? checked.filter((one) => !gpuHas([one], id))
        : [...checked, id]
      ).join(','),
    )
    setWeights((w) => {
      const next = { ...w }
      if (on) delete next[id]
      else next[id] = gpuNewWeight(checked, w)
      return next
    })
    fields.clear('tensor-split')
    setMarks(null)
  }

  /** Move up or Move down (GD1.9): the row swaps with its neighbour among the
   * checked rows, its weight going with it, and the card's visually hidden
   * status says where it landed. The pressed button keeps the focus. A Move
   * changes the checked rows, so the weights' error and the marks go with it
   * (GD2.6), as under a checkbox. */
  function gpuMove(id: string, dir: GpuDirection) {
    const checked = gpuChecked()
    const at = checked.findIndex(
      (one) => one.toLowerCase() === id.toLowerCase(),
    )
    const to = dir === 'up' ? at - 1 : at + 1
    if (at < 0 || to < 0 || to >= checked.length) return
    const next = [...checked]
    next[at] = next[to]!
    next[to] = checked[at]!
    edit('device', next.join(','))
    setMoved(
      t('model-profile-gpu-moved', id, String(to + 1), String(checked.length)),
    )
    fields.clear('tensor-split')
    setMarks(null)
  }

  /** What the user typed in a row's weight (GD2.1): the text is kept as typed,
   * and the weights' error goes with it (GD2.6). */
  function gpuWeight(id: string, value: string) {
    setWeights((w) => ({ ...w, [id]: value }))
    fields.clear('tensor-split')
    setMarks(null)
    setPhase((p) => (p === 'saving' ? p : 'idle'))
  }

  /** Auto and Even (GD2.4): every checked row's weight emptied, or set to 1.
   * The focus stays on the pressed button, and the weights' error goes. */
  function gpuWeightsAll(value: string) {
    setWeights(Object.fromEntries(gpuChecked().map((id) => [id, value])))
    fields.clear('tensor-split')
    setMarks(null)
    setPhase((p) => (p === 'saving' ? p : 'idle'))
  }

  /** The GPU layers the blocks are cut over (brief 19, 19 G.11): what the
   * field holds, and nothing while its box is ticked, which is the default and
   * no value of its own. A change here re-projects the rows and writes nothing:
   * the counts are the layout's, not a key's. */
  function gpuLayers(): string {
    const b = boxed['n-gpu-layers']
    return b === undefined ? '' : boxText(b)
  }

  /** The layout the blocks are cut over (brief 19, 19 G.4), read the way the
   * rows read it: the same GPU layers and the same checked rows, so a count a
   * row shows and a count a save writes are the same cut. */
  function gpuBlocksLayout(): Layout | null {
    return gpuLayout(
      modelInfo,
      gpuLayers(),
      draft['split-mode'] ?? null,
      gpuChecked().length,
    )
  }

  /** The weights a slider's drag started from (19 G.6): the pointer going down
   * takes them, every move of that drag is measured from them, and the pointer
   * going up or cancelled lets them go, so a drag back to where it began
   * leaves the rows as they were, and the arrow keys work from the counts. */
  const blockBase = useRef<Record<string, string> | null>(null)

  /** A row's block count applied (19 G.6 and 19 G.7): the counts of `base` with
   * that row asking `blocks`, written back as the weights of GD2.5, so the
   * counts always sum to the places and `tensor-split` is the counts joined.
   * `null` is a count that is no whole number: the row's own text puts its
   * count back, and nothing is written. */
  function gpuBlocksSet(
    id: string,
    blocks: number | null,
    base: Record<string, string>,
  ) {
    const lay = gpuBlocksLayout()
    const checked = gpuChecked()
    const at = gpuIndexOf(checked, id)
    if (lay === null || at < 0 || blocks === null) return
    setWeights(
      Object.fromEntries(
        checked.map((one, i) => [
          one,
          `${
            setBlocks(
              checked.map((each) => (base[each] ?? '').trim()),
              at,
              blocks,
              lay,
            )[i]
          }`,
        ]),
      ),
    )
    fields.clear('tensor-split')
    setMarks(null)
    setPhase((p) => (p === 'saving' ? p : 'idle'))
  }

  function gpuBlock(id: string, raw: string) {
    gpuBlocksSet(id, gpuBlockNumber(raw), blockBase.current ?? weights)
  }

  function gpuBlockCommit(id: string, raw: string) {
    gpuBlocksSet(id, gpuBlockNumber(raw), weights)
  }

  /** The two keys the GPUs fieldset composes (GD1.10, GD2.5): the checked rows
   * and their weights say both, so neither is ever typed. `tensor-split`
   * counts as changed when the rows moved or a weight differs from what it
   * loaded with, and the vector composed then differs from the one read. */
  function gpuChanges(): Record<string, string | null> {
    const checked = gpuChecked()
    const loadedIds = gpuFileIds(loaded.values['device'] ?? null)
    const device = gpuComposeDevice(checked)
    const deviceWas = gpuComposeDevice(loadedIds)
    const values: Record<string, string | null> = {}
    if (device !== deviceWas) values['device'] = device
    const split = gpuComposeSplit(checked, weights)
    const splitWas = gpuSplitLoaded(loaded.values['tensor-split'] ?? null)
    const loadedWeights = gpuFileWeights(
      loaded.values['tensor-split'] ?? null,
      loadedIds,
    )
    const typed =
      checked.map((id) => (weights[id] ?? '').trim()).join(',') !==
      loadedIds.map((id) => (loadedWeights[id] ?? '').trim()).join(',')
    if ((device !== deviceWas || typed) && split !== splitWas)
      values['tensor-split'] = split
    return values
  }

  /** The Tensor placement card's controls (brief 21, 21 T.6): each one rewrites
   *  its own row and leaves the others, and moves no focus. The key is sent when
   *  the rows no longer render the loaded rules (21 T.8), so an edit typed and
   *  typed back is unchanged, and a Layers text `parseLayers` cannot read is a
   *  change: the pattern stays as the last readable text composed it. */
  function otEdit(id: number, change: (row: TensorRow) => TensorRow) {
    setOtRows((rows) => rows.map((row) => (row.id === id ? change(row) : row)))
  }

  /** Tensor: the row's kind. A row that comes to Custom shows the Custom text it
   *  remembers for the visit, else the pattern it stands for now; a row that
   *  leaves Custom keeps its text to remember (21 T.6). */
  function otKind(id: number, kind: string) {
    otEdit(id, (row) => ({
      ...row,
      kind,
      ...(kind === OT_CUSTOM
        ? { customText: row.customText ?? rowRule(row).pattern }
        : {}),
    }))
  }

  /** Layers: the text as typed. The rule's pattern is the kind composed with the
   *  layers the row carries (21 T.6), so a half-typed text leaves the pattern as
   *  the last readable text composed it. */
  function otLayersText(id: number, text: string) {
    otEdit(id, (row) => otSetLayersText(row, text))
  }

  /** Pattern: the text as typed. It is the rule's pattern at once (21 T.6), so
   *  the key follows the text as typed; TP2 checks the text itself at Save. */
  function otPatternText(id: number, text: string) {
    otEdit(id, (row) => ({ ...row, customText: text }))
  }

  /** Goes on: the option's value is the rule's device. */
  function otDevice(id: number, device: string) {
    otEdit(id, (row) => ({ ...row, device }))
  }

  /** Add rule (21 T.7): the embedding table on the first GPU the read lists, on
   *  CPU when it lists none or has not answered, appended after the rules, the
   *  focus on its Tensor. The rows are committed (flushSync) before the focus,
   *  the way a jump opens a card: the Tensor of a row that mounts in this commit
   *  exists only after it. */
  function otAdd() {
    const id = otRows.reduce((max, row) => Math.max(max, row.id), 0) + 1
    flushSync(() => {
      setOtRows([...otRows, otNewRule(id, otNewDevice(probe))])
    })
    document.getElementById(otTensorId(id))?.focus()
  }

  /** Remove (21 T.7): the rule goes, and the focus goes to the rule that moved
   *  up, else to the one before it, else to Add rule. */
  function otRemove(id: number) {
    const at = otRows.findIndex((row) => row.id === id)
    const rows = otRows.filter((row) => row.id !== id)
    flushSync(() => {
      setOtRows(rows)
    })
    const next = rows[at] ?? rows[at - 1]
    document
      .getElementById(next === undefined ? ADD_ID : otTensorId(next.id))
      ?.focus()
  }

  /** Whether the rules on screen are an edit of the loaded ones (21 T.8): the key
   *  is changed when `render` of the rows differs from `render` of the loaded
   *  rules, so a value the file wrote with an alias, a space or a hand-written
   *  order reads unchanged, and an edit typed and typed back is unchanged. */
  function otChanged() {
    return (
      rowsValue(otRows) !==
      rowsValue(tensorRows(loaded.values['override-tensor'] ?? null))
    )
  }

  const tensor: TensorCardProps = {
    rows: otRows,
    probe,
    onKind: otKind,
    onLayersText: otLayersText,
    onPatternText: otPatternText,
    onDevice: otDevice,
    onAdd: otAdd,
    onRemove: otRemove,
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
    // Brief 14: the GPUs fieldset's two composed keys.
    Object.assign(values, gpuChanges())
    // Brief 21 21 T.8: the rules of `override-tensor`, sent when the rows no
    // longer render the loaded ones; no rule left is the key's removal.
    if (otChanged()) values['override-tensor'] = rowsValue(otRows)
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
          !ruleLocked(row, draft, loaded.values, boxed, factsRead(modelInfo)) ||
          row.key in changes,
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
    // Brief 14 GD2.6: when `tensor-split` is changed and composes a vector,
    // every checked row's weight must be a number: one that is not stops the
    // save, and the fieldset says so after the list, after the device error.
    const incomplete = gpuWeightFailures(gpuChecked(), weights)
    const weightsBad =
      'tensor-split' in changes &&
      gpuComposeSplit(gpuChecked(), weights) !== null &&
      incomplete.length > 0
    if (failures.length > 0 || weightsBad) {
      const marked: Record<string, string> = Object.fromEntries(
        failures.map(([row, error]) => [row.key, error]),
      )
      if (weightsBad) {
        marked['tensor-split'] = t('model-profile-gpu-weights-incomplete')
        // The device error a refusal left stands beside it.
        const device = fields.errors['device']
        if (device !== undefined) marked['device'] = device
      }
      // Rendered first, so the focused control is announced with its error.
      flushSync(() => {
        alerts.setAlert({ text: t('save-invalid') })
        fields.set(marked)
        setMarks(weightsBad ? incomplete : null)
      })
      const first = failures[0]
      if (first === undefined) focusField('tensor-split')
      else
        focusFailure(
          first[0],
          ruleLocked(
            first[0],
            draft,
            loaded.values,
            boxed,
            factsRead(modelInfo),
          ),
        )
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
    setMarks(null)
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
    // Brief 14 GD1.10: the rows, their checks, their order and their weights
    // go back with the draft.
    setWeights(
      gpuFileWeights(
        loaded.values['tensor-split'] ?? null,
        gpuFileIds(loaded.values['device'] ?? null),
      ),
    )
    setMarks(null)
    setMoved('')
    // Brief 21 21 T.8: the rules go back to the loaded ones, and the rows'
    // remembered texts and half-typed fields go with them.
    setOtRows(tensorRows(loaded.values['override-tensor'] ?? null))
    fields.set({})
    setPhase('idle')
  }

  /** The GPU distribution card's state, as the cards' table passes it to the
   * GPUs fieldset (brief 14). */
  const gpu: GpuCardProps = {
    probe,
    modelInfo,
    weights,
    weightIds: marks,
    loadedDevice: loaded.values['device'] ?? null,
    onToggle: gpuToggle,
    onMove: gpuMove,
    layers: gpuLayers(),
    onWeight: gpuWeight,
    onBlock: gpuBlock,
    onBlockCommit: gpuBlockCommit,
    onBlockBase: () => {
      blockBase.current = weights
    },
    onBlockRelease: () => {
      blockBase.current = null
    },
    onAuto: () => gpuWeightsAll(''),
    onEven: () => gpuWeightsAll('1'),
    moved,
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
          gpu={gpu}
          tensor={tensor}
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
