// A model profile's page (#/profiles/model/<id>, signed in): the way back to
// the list, the id as the heading, and the model profile read once each time
// the page opens from GET /api/profiles/model/<id>, or the message saying why
// it is not there (fe/briefs/02-models.md, sections 2d.1 to 2d.4). Below the
// heading, the summary line naming the engine and the model file (H11a.1) and
// the head's Used by row, the instance profiles that list the model (brief 22,
// IP5.4), then the Assets card as text behind a disclosure in its own heading
// (H11a.3), its first row the Engine (H11b.1), and the cards of the settings
// (fe/briefs/02e-model-profile-cards.md, section 1). The page is hardware-free
// (brief 22, IP5.5): each card keeps only the fields of
// checks::MODEL_PROFILE_STAY_KEYS, and the four cards of the placement and the
// memory are the instance profile entry's (brief 26). The Embeddings card's two
// controls, the KV cache card's control, the Chat / reasoning and Sampling
// overrides cards' fields, the Vision card's controls and the Speculative
// decoding card's controls are editable, and
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
// A model profile no instance profile lists is deleted from its own page, the
// delete asked in a confirm in place (fe/briefs/22-model-sources.md, slice 22D,
// criteria 22 D.1 to 22 D.7), the way 16 17b has it on a server profile; the
// bar's Delete is switched off while an instance profile lists the model
// (brief 22, IP5.6).
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
  checkboxSend,
  checkboxState,
  deviceSend,
  deviceState,
  fieldId,
  fieldPlace,
  numberError,
  numberSend,
  numberStates,
  pageCheckboxRows,
  pageDeviceRows,
  pageEditableRows,
  pageNumberRows,
  pageTextRows,
  ruleLocked,
  selectSend,
  selectState,
  specTypeEntries,
  textError,
  textSend,
  unlockControl,
} from './ModelProfileCards.tsx'
import { factsRead } from './GpuSplit.tsx'
import {
  modelProfileInUse,
  modelProfileInUseReason,
} from './modelProfileDelete.ts'
import { useFieldErrors, type RefusalTable } from './refusal.ts'
import { deleteRow } from './RowDelete.tsx'
import { RestartOffer } from './RestartOffer.tsx'
import { sendFailure, saveAnswer, type SaveAnswer } from './save.ts'
import {
  SaveBar,
  useAsk,
  useDirtyReport,
  useSaveAlert,
  useSaveBarPadding,
  useWarnOnLeave,
} from './SaveBar.tsx'
import {
  instanceProfileHref,
  instanceProfileModelHref,
  modelProfileIdUnreadable,
} from './route.ts'
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
  /** The instance profiles that list this model, in the order the read listed
   *  them (brief 22, IP5.0): who uses the profile, and the reason its Delete
   *  cannot act. A read that names none, a file of an earlier release, is a
   *  profile nobody lists (IP5.6, the list's rule too). */
  usedBy?: string[]
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
    (value.usedBy === undefined || Array.isArray(value.usedBy)) &&
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
  nameOf,
  onInstances,
}: {
  /** The id from the address, while the read has not answered. */
  id: string
  onSignedOut: (reason: SignedOut) => void
  /** A sign-in in the bar's dialog: true when the edits belong to it and stay. */
  onRenewed: (session: Session) => boolean
  /** Told whether the form holds unsaved changes; false when it goes. */
  onDirty: (dirty: boolean) => void
  /** The name of an instance by its id, for the restart offer (16 17d.1a). */
  nameOf: (id: string) => string
  /** The app's instance list read again (16 17d.2a). */
  onInstances: () => void
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
  // Brief 22, slice 22D: the model profile this page deleted (22 D.3), and the
  // read again of a delete that found it gone, which lands on the paragraph
  // saying there is no such model profile (22 D.6, 17b review D2).
  const [deleted, setDeleted] = useState(false)
  const [focusMissing, setFocusMissing] = useState(false)

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

  // The paragraph of a delete that went through takes the focus (22 D.3).
  useEffect(() => {
    if (deleted) document.getElementById('model-profile-deleted')?.focus()
  }, [deleted])

  // The paragraph the delete's read again lands on takes the focus, so the
  // focus does not end on the body once the form and its confirm are gone
  // (22 D.6, 17b review D2).
  useEffect(() => {
    if (focusMissing)
      document.getElementById('model-profile-not-found')?.focus()
  }, [focusMissing, answer])

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

  /** Reads the model profile again beside what is on screen, the form and the
   * delete's confirm staying as they are: the read answers the refusal the
   * delete got (22 D.4, 22 D.6, 22 D.7, as 16 17b.9). */
  function reread() {
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
  if (deleted) {
    // The model profile is gone: the page is the one paragraph that says so,
    // focused, and the link back to the list (22 D.3, as 16 17b.8). The form
    // went with it, so the page asks nothing about unsaved changes any more.
    return (
      <>
        <PageHeading text={shown} />
        <p id="model-profile-deleted" tabIndex={-1}>
          {t('model-profile-deleted', shown)}
        </p>
        <p>
          <a href="#/profiles/model">{t('model-profile-back')}</a>
        </p>
      </>
    )
  }
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
        onDeleted={() => setDeleted(true)}
        onMissing={() => setFocusMissing(true)}
        onReread={reread}
        nameOf={nameOf}
        onInstances={onInstances}
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
  onDeleted,
  onMissing,
  onReread,
  nameOf,
  onInstances,
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
  /** The delete went through (22 D.3): the page becomes the paragraph saying so. */
  onDeleted: () => void
  /** The delete found the model profile gone (22 D.6): the read again lands on
   * its paragraph, which takes the focus. */
  onMissing: () => void
  /** The model profile is read again beside the form (22 D.4, 22 D.6, 22 D.7). */
  onReread: () => void
  /** The name of an instance by its id, for the restart offer (16 17d.1a). */
  nameOf: (id: string) => string
  /** The app's instance list read again (16 17d.2a). */
  onInstances: () => void
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
        dropped={dropped}
        renewals={renewals}
        onDirty={onDirty}
        onReload={onReload}
        onRenewed={onRenewed}
        onDeleted={onDeleted}
        onMissing={onMissing}
        onReread={onReread}
        nameOf={nameOf}
        onInstances={onInstances}
      />
    )
  if (answer.error === 'client-not-authorized') {
    return <p>{t('api-client-not-authorized')}</p>
  }
  if (answer.error === 'client-unavailable') {
    return <p>{t('api-client-unavailable')}</p>
  }
  if (answer.error === 'not-found') {
    // The paragraph is a place the focus can take: the read again after a
    // delete lands here, and the form and its confirm are gone with the
    // profile (22 D.6, 17b review D2).
    return (
      <p id="model-profile-not-found" tabIndex={-1}>
        {t('model-profile-not-found', id)}
      </p>
    )
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
      // Brief 21 TP2, 21 T.12: the two checks of `override-tensor` the manager
      // answers beside the pattern and device ones.
      case 'override-tensor-space':
        return t('api-override-tensor-space', ...args)
      case 'override-bad-device':
        return t('api-override-bad-device', ...args)
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
      // Brief 22, IP5.7: the setting is the instance profile entry's, so the
      // bff refuses it by name. The name is the 422's `field`, which
      // `refusalOf` reads beside the args.
      case 'model-profile-placement-key':
        return t('api-model-profile-placement-key', args[0] ?? '')
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
      // Brief 22, IP5.7: the key names the setting, and the reply names it too.
      case 'model-profile-placement-key':
        return args[0]
      // Brief 21 TP2, 21 T.12: the card owns the key, so the fallback names it
      // and the page answers on the rule the args point at.
      case 'override-no-pattern':
      case 'override-no-device':
      case 'override-tensor-space':
      case 'override-bad-device':
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

/** Puts the focus on the first control the page marked, in the order the
 *  controls are on screen (21 T.11, 21 T.12: the first failing one). The marks
 *  are keyed as the card keys its Field Errors, and `otControlElementId` gives
 *  the id of the control each one marks. A mark names a control the page
 *  renders, so one of them answers. */

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
  onDeleted,
  onMissing,
  onReread,
  nameOf,
  onInstances,
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
  /** The delete went through (22 D.3): the page becomes the paragraph saying so. */
  onDeleted: () => void
  /** The delete found the model profile gone (22 D.6): the read again lands on
   * its paragraph, which takes the focus. */
  onMissing: () => void
  /** The model profile is read again beside the form (22 D.4, 22 D.6, 22 D.7). */
  onReread: () => void
  /** The name of an instance by its id, for the restart offer (16 17d.1a). */
  nameOf: (id: string) => string
  /** The app's instance list read again (16 17d.2a). */
  onInstances: () => void
}) {
  const fid = useId()
  // H11a.1: the model file the summary line names, as the read gave it.
  const fileName = modelFileName(doc.values)
  // The instance profiles that list the model (IP5.0): the head's Used by row,
  // the placement note's link, and the reason the Delete cannot act (IP5.6). A
  // read that names none is a profile nobody lists, the list's rule (IP5.3).
  const usedBy = doc.usedBy ?? []
  const [loaded, setLoaded] = useState<Loaded>({
    revision: doc.revision,
    values: doc.values,
  })
  // The document read again beside the form (22 D.4, and 23 A2.3 after a
  // switch): the revision the next save sends is the one that read gave, so a
  // page the switch brought saves with the revision the switch's write left
  // behind, and is never refused as stale. The values stay as the page holds
  // them: a read again answers a refusal, not an edit.
  const [read, setRead] = useState({ id: doc.id, revision: doc.revision })
  if (read.id !== doc.id || read.revision !== doc.revision) {
    setRead({ id: doc.id, revision: doc.revision })
    setLoaded((l) => ({ ...l, revision: doc.revision }))
  }
  const [draft, setDraft] = useState<Record<string, string>>(doc.values)
  // The box state of the number fields the form edits (156): the text a field
  // holds is kept while its box is ticked, so unticking gives it back.
  const [boxed, setBoxed] = useState<Record<string, Boxed>>(() =>
    numberStates(doc.values),
  )
  const [phase, setPhase] = useState<
    'idle' | 'saving' | 'saved' | 'saved-restart' | 'saved-restart-ids'
  >('idle')
  // The instances the last save left needing a restart (16 17d.4a): the offer
  // of the Save bar names them, and the next edit or save takes it away.
  const [restart, setRestart] = useState<string[]>([])
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
  // The restart offer's button that asked for the sign-in (17d review D1): the
  // dialog gives the focus back to it, not to Save changes.
  const offerControl = useRef<string | null>(null)

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

  // Brief 22, slice 22D: the delete, asked in the page (fe/DESIGN.md 5.2), the
  // way 16 17b has it on a server profile.
  const [confirming, setConfirming] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [deleteStatus, setDeleteStatus] = useState('')
  const [deleteAlert, setDeleteAlert] = useState<{
    text: string
    signIn: boolean
  } | null>(null)
  // The refusal of a delete whose read again takes the delete and its group
  // with it: its words go on living in a paragraph of their own, in the place
  // the delete had (22 D.4, 17b review D2).
  const [refusal, setRefusal] = useState<string | null>(null)
  // The sign-in the delete's alert offers, in a dialog of its own: the page and
  // its group stay behind it (22 D.7).
  const [askingDeleteSignIn, setAskingDeleteSignIn] = useState(false)
  const deleteOpener = useRef<HTMLButtonElement>(null)
  const sendingDelete = useRef(false)
  // 22 D4.10 keeps the bar's Delete always on the bar, switched off with its
  // reason as the description (22 D4.2), never hidden. IP5.6: what switches it
  // off is an instance profile listing the model, the same rule and the same
  // reason the list's row minus carries (IP5.3, `modelProfileDelete.ts`).
  const canDelete = !modelProfileInUse(doc.usedBy)

  /** The bar's Delete pressed (22 D4.10): it opens the confirm, and is the
   *  place Cancel and Escape bring the focus back to. A pressed Delete that
   *  cannot act opens nothing, and 23 A2.4: a Delete pressed while the switch
   *  writes opens nothing either. */
  function openDelete() {
    if (!canDelete) return
    deleteOpener.current = document.getElementById(
      'model-profile-delete',
    ) as HTMLButtonElement | null
    setConfirming(true)
  }

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
   * 166). The rows are those of the cards the page draws, so a key that moved
   * to the instance profile has no row and is never part of a save (brief 22,
   * IP5 review A1). */
  function changed(): Record<string, string | null> {
    const values: Record<string, string | null> = {}
    for (const row of pageEditableRows()) {
      const state = selectState(row, draft)
      if (state === selectState(row, loaded.values)) continue
      values[row.key] = selectSend(row, state)
    }
    for (const row of pageCheckboxRows()) {
      // 2h.8's 185: an invalid `fit` is a state of its own, so a click off it
      // is a change even when the box ends at the state an `off` value shows.
      const state = checkboxDirty(row, draft)
      if (state === checkboxDirty(row, loaded.values)) continue
      values[row.key] = checkboxSend(row, checkboxState(row, draft))
    }
    // 2h.7's 179: a device select at the empty option is the key's removal.
    for (const row of pageDeviceRows()) {
      const state = deviceState(row, draft)
      if (state === deviceState(row, loaded.values)) continue
      values[row.key] = deviceSend(state)
    }
    const was = numberStates(loaded.values)
    for (const row of pageNumberRows()) {
      const b = boxed[row.key]
      if (b === undefined) continue
      if (b.ticked) {
        if (loaded.values[row.key] !== undefined) values[row.key] = null
      } else if (b.text !== was[row.key]?.text) {
        values[row.key] = numberSend(row, b)
      }
    }
    for (const row of pageTextRows()) {
      const send = textSend(draft[row.key] ?? '')
      if (send === (loaded.values[row.key] ?? null)) continue
      values[row.key] = send
    }
    return values
  }

  /** Save changes: the bar of 135 to 149. */
  async function save() {
    // A second Save while one runs sends nothing (137).
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
          !ruleLocked(row, draft, loaded.values, boxed, factsRead(undefined)) ||
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
    if (failures.length > 0) {
      // Rendered first, so the focused control is announced with its error.
      flushSync(() => {
        alerts.setAlert({ text: t('save-invalid') })
        fields.set(
          Object.fromEntries(failures.map(([row, error]) => [row.key, error])),
        )
      })
      const first = failures[0]!
      focusFailure(
        first[0],
        ruleLocked(first[0], draft, loaded.values, boxed, factsRead(undefined)),
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
      setRestart(answer.restart)
      setPhase(
        answer.restart.length > 0
          ? 'saved-restart-ids'
          : answer.running
            ? 'saved-restart'
            : 'saved',
      )
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
        // Brief 22, IP5.7: a setting that moved has no field on this page to
        // mark, and the sentence carries its name, so the bar says the sentence
        // as it is, with no wrapper.
        const placement =
          answer.key === 'model-profile-placement-key'
            ? t('api-model-profile-placement-key', answer.field ?? '')
            : null
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
          placement !== null ||
          place === null ||
          field === undefined ||
          fieldTakesFocus(field)
            ? null
            : t('save-refused-at', answer.text, place.label, place.card)
        // Rendered first, so the focused control is announced with its error.
        const text = placement ?? answer.text
        flushSync(() => {
          alerts.setAlert({ text: at ?? placement ?? t('save-refused', text) })
          if (field !== undefined) fields.set({ [field]: text })
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
   * the model profile again and this form goes with it. The restart offer's
   * Sign in is the exception: the focus goes back to the button whose restart
   * was refused (17d review D1). */
  function signedIn(session: Session) {
    if (!onRenewed(session)) return
    alerts.setAlert(null)
    const offer = offerControl.current
    offerControl.current = null
    askSignIn.close(offer ?? saveId)
  }

  /** Cancel, or Escape: the group goes and the focus returns to the opener.
   * While the delete runs nothing closes: the group, its question and its
   * status line stay as they are and the focus does not move (22 D.3, 17b
   * review R1). */
  function closeConfirm() {
    if (sendingDelete.current) return
    setConfirming(false)
    setDeleteAlert(null)
    setDeleteStatus('')
    setRefusal(null)
    deleteOpener.current?.focus()
  }

  /** The delete (22 D.3): `DELETE` of the model profile, its answers 22 D.4 to
   * 22 D.7. The words are the list delete's (`deleteRow`, 22 D4.7): the page
   * only says where each answer goes on this page, the group, the bar's own
   * paragraph, and the page that says the profile is gone. */
  async function doDelete() {
    if (sendingDelete.current) return
    sendingDelete.current = true
    setDeleting(true)
    setDeleteAlert(null)
    setRefusal(null)
    setDeleteStatus(t('confirm-deleting'))
    const answer = await deleteRow(
      `/api/profiles/model/${encodeURIComponent(doc.id)}`,
    )
    sendingDelete.current = false
    setDeleting(false)
    setDeleteStatus('')
    if (answer.kind === 'done') {
      // The model profile is gone: the page is the paragraph saying so (22 D.3).
      onDeleted()
      return
    }
    if (answer.kind === 'group') {
      // The group keeps the words and the verb stays active (22 D.5).
      setDeleteAlert({ text: answer.text, signIn: answer.signIn })
      return
    }
    if (answer.focus === 'row') {
      // The manager refused and the opener stays: the words go on living in the
      // bar's own paragraph, and the read again shows what it refuses now (22
      // D.4, 22 D4.7, 22 D4.11). The Delete stays, switched off, and takes the
      // focus (22 D4.11).
      setRefusal(answer.text)
      setConfirming(false)
      setDeleteAlert(null)
      onReread()
      deleteOpener.current?.focus()
      return
    }
    // The model profile is gone, or the manager did not answer in time: the
    // read again shows the page that says so, and that page's paragraph takes
    // the focus (22 D.6, 22 D.7, 17b review D2).
    setDeleteAlert({ text: answer.text, signIn: false })
    onMissing()
    onReread()
  }

  /** A sign-in in the delete's dialog (22 D.7): the page keeps what it holds,
   * the group stays open and the focus returns to the verb; nothing is sent
   * again. */
  function signedInDelete(session: Session) {
    const kept = onRenewed(session)
    flushSync(() => {
      setAskingDeleteSignIn(false)
      if (kept) setDeleteAlert(null)
    })
    if (!kept) return
    document.getElementById('confirm-delete')?.focus()
  }

  function closeDeleteSignIn() {
    flushSync(() => setAskingDeleteSignIn(false))
    document.getElementById('confirm-delete')?.focus()
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
      {/* Brief 22 IP5.4: the profile has no on/off state, so the state line of
          24 A2.1 is gone with its switch. The head says who lists the profile
          instead: the row the list's Used by column says (IP5.2), each instance
          profile reaching its entry page, and the words of a profile none
          lists. It is the shape of 16 17a.5's head list, the one `dl.assets`
          above the settings. */}
      <dl className="assets">
        <dt>{t('model-profiles-column-used-by')}</dt>
        {usedBy.length === 0 ? (
          <dd className="muted">{t('model-profiles-unused')}</dd>
        ) : (
          <dd>
            {usedBy.map((one, n) => (
              <Fragment key={one}>
                {n > 0 && ', '}
                <a href={instanceProfileHref(one)}>{one}</a>
              </Fragment>
            ))}
          </dd>
        )}
      </dl>
      {/* IP5.5: one note under the head says where the placement lives now.
          The first instance profile that lists the model is the link to its
          placement entry; a model none lists leaves it out. */}
      <p className="intro">
        {t('model-profile-placement-elsewhere')}
        {usedBy.length > 0 && (
          <>
            {' '}
            <a href={instanceProfileModelHref(usedBy[0]!, doc.id)}>
              {t('model-profile-placement-link', usedBy[0]!)}
            </a>
          </>
        )}
      </p>
      <section>
        {/* 22 D4.32: the heading is for the outline, so the page keeps one
            title above the form; the eye sees the cards' own headings. */}
        <h2 className="visually-hidden">{t('model-profile-settings-title')}</h2>
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
        {/* 22 L2.B1 (the user's ruling of 2026-11-26, reversing 22 D2.5 and
            D2.6): the Save bar is the settings section's last child, after
            every card, and the sheet sticks it to the viewport's bottom edge,
            so Tab walks the fields before the bar and the bar floats over the
            end of the form. 22 D4.30 with L2.A2 orders its group Discard, Save
            changes, Delete; 22 D4.11 keeps the confirm and a refusal in the
            bar, so both outlive the cards and the bar grows for them. */}
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
          deleteId="model-profile-delete"
          deleteLabel={t('model-profile-delete')}
          onDelete={openDelete}
          deleteDisabled={!canDelete}
          deleteExpanded={confirming}
          deleteHint={modelProfileInUseReason(doc.usedBy) ?? undefined}
          restartNames={restart.map((id) => nameOf(id)).join(', ')}
        >
          {/* 16 17d.4a: the restart the save left to make, a row of the bar
              under its buttons, the bar being sticky so the offer stays in
              reach. An edit answers it, as it answers the status line. */}
          {phase === 'saved-restart-ids' && !dirty && (
            <RestartOffer
              ids={restart}
              nameOf={nameOf}
              renewals={renewals}
              onChanged={onInstances}
              onSignIn={(control) => {
                offerControl.current = control
                askSignIn.open()
              }}
            />
          )}
          {canDelete && confirming && (
            <ConfirmDelete
              id={doc.id}
              status={deleteStatus}
              alert={deleteAlert}
              running={deleting}
              onConfirm={() => void doDelete()}
              onCancel={closeConfirm}
              onSignIn={() => setAskingDeleteSignIn(true)}
            />
          )}
          {/* 22 D2.4: the refusal of a delete whose read again took the Delete
              and its group with it: its words go on living in the bar's own
              paragraph (22 D.4, 22 D4.11, 17b review D2). It is no live region
              of the form: it is the answer outliving the control that asked. */}
          {refusal !== null && (
            <p
              id="model-profile-delete-refused"
              role="alert"
              tabIndex={-1}
              className="save-alert"
            >
              {refusal}
            </p>
          )}
        </SaveBar>
      </section>
      {askReload.asking && (
        <DiscardDialog onKeep={askReload.keep} onDiscard={askReload.confirm} />
      )}
      {askSignIn.asking && (
        <SignInDialog onSignedIn={signedIn} onClose={() => askSignIn.close()} />
      )}
      {/* The sign-in the delete's alert offers (22 D.7): the page and its group
          stay behind it. */}
      {askingDeleteSignIn && (
        <SignInDialog onSignedIn={signedInDelete} onClose={closeDeleteSignIn} />
      )}
    </>
  )
}

/** The confirm in place of the delete (fe/DESIGN.md 5.2, 22 D.2 to 22 D.7, as
 *  16 17b has it on a server profile): the question that names the model
 *  profile, the verb and Cancel, the status line and the alert place. It is
 *  mounted while the delete is asked, so its words and its alert are its own:
 *  closing it clears all of them. */
function ConfirmDelete({
  id,
  status,
  alert,
  running,
  onConfirm,
  onCancel,
  onSignIn,
}: {
  /** The model profile's id, named in the question. */
  id: string
  status: string
  alert: { text: string; signIn: boolean } | null
  /** True while the delete runs: both buttons go inactive, Escape is inert. */
  running: boolean
  onConfirm: () => void
  onCancel: () => void
  onSignIn: () => void
}) {
  const uid = useId()
  const questionId = `${uid}-question`

  // The focus moves to Cancel, the verb is one Tab away (22 D.2).
  useEffect(() => {
    document.getElementById('confirm-cancel')?.focus()
  }, [])

  return (
    <div
      role="group"
      className="confirm-group"
      aria-labelledby={questionId}
      onKeyDown={(e) => {
        if (e.key === 'Escape' && !running) onCancel()
      }}
    >
      <p id={questionId}>{t('model-profile-delete-question', id)}</p>
      <div className="save-actions">
        <button
          type="button"
          id="confirm-delete"
          className="primary danger"
          aria-disabled={running || undefined}
          onClick={onConfirm}
        >
          {t('confirm-delete')}
        </button>
        <button
          type="button"
          id="confirm-cancel"
          className="secondary"
          aria-disabled={running || undefined}
          onClick={onCancel}
        >
          {t('confirm-cancel')}
        </button>
      </div>
      <p role="status" className="save-status" aria-live="polite">
        {status}
      </p>
      {alert !== null && <p role="alert">{alert.text}</p>}
      {alert?.signIn && (
        <button
          type="button"
          id="session-sign-in"
          className="secondary"
          onClick={onSignIn}
        >
          {t('session-sign-in')}
        </button>
      )}
    </div>
  )
}
