// A preset's page (#/models/<id>, signed in): the way back to the list, the id
// as the heading, and the preset read once each time the page opens from
// GET /api/presets/<id>, or the message saying why it is not there
// (fe/briefs/02-models.md, sections 2d.1 to 2d.4). Below the heading, the
// Engine field, the Assets card as text and the cards of the settings
// (fe/briefs/02e-preset-cards.md, section 1). The Embeddings card's two
// controls are editable and the save bar at the end of the settings writes
// what changed (fe/briefs/02h-preset-save.md, sub-slice 2h.1, criteria 130 to
// 149, read by key in save.ts as the Server page reads its own); every other
// control stays disabled, and its settings are changed in pai-config.
import { Fragment, useEffect, useId, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import Card from './Card.tsx'
import { DiscardDialog } from './Dialog.tsx'
import { SelectField } from './Field.tsx'
import PageHeading from './PageHeading.tsx'
import PresetCards, {
  editableRows,
  fieldId,
  selectSend,
  selectState,
} from './PresetCards.tsx'
import { useFieldErrors, type RefusalTable } from './refusal.ts'
import { sendFailure, saveAnswer, type SaveAnswer } from './save.ts'
import {
  SaveBar,
  useAsk,
  useDirtyReport,
  useSaveBarPadding,
  useWarnOnLeave,
  type SaveAlert,
} from './SaveBar.tsx'
import { presetIdUnreadable } from './route.ts'
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
  onDirty,
}: {
  /** The id from the address, while the read has not answered. */
  id: string
  onSignedOut: (byManager: boolean) => void
  /** Told whether the form holds unsaved changes; false when it goes. */
  onDirty: (dirty: boolean) => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // Raised by the bar's Reload (145): `reads` starts the read again, `asked`
  // holds the preset the read was asked for.
  const [reads, setReads] = useState(0)
  const asked = useRef<string | null>(null)

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

  /** Reads the preset again, the edits discarded: the settings give place to
   * the page's loading status until the read lands (145). */
  function reload() {
    asked.current = id
    setAnswer(undefined)
    setReads((n) => n + 1)
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
      <Body id={id} answer={answer} onDirty={onDirty} onReload={reload} />
    </>
  )
}

/** The page's content: the loaded preset, or the message. */
function Body({
  id,
  answer,
  onDirty,
  onReload,
}: {
  id: string
  answer: Answer | undefined
  onDirty: (dirty: boolean) => void
  onReload: () => void
}) {
  if (presetIdUnreadable(id)) {
    return <p>{t('preset-not-found', id)}</p>
  }
  if (answer === undefined || answer === 'signed-out' || answer === 'revoked') {
    return <p role="status">{t('preset-loading')}</p>
  }
  if ('doc' in answer)
    return <Content doc={answer.doc} onDirty={onDirty} onReload={onReload} />
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

/** The control of the field a refusal names, when this page renders it: a key
 * of the Assets card, or one 2h shows nowhere, has no control here (143). */
function fieldControl(
  key: string,
): HTMLInputElement | HTMLSelectElement | null {
  const control = document.getElementById(`${fieldId(key)}-control`)
  return control instanceof HTMLInputElement ||
    control instanceof HTMLSelectElement
    ? control
    : null
}

/** Puts the focus on the marked field's control (142). A disabled control
 * takes no focus, so the focus stays where it was (144). */
function focusField(key: string) {
  const control = fieldControl(key)
  if (control !== null && !control.disabled) control.focus()
}

/** The loaded preset as a form: the settings of 2e, the two Embeddings
 * controls of 2h.1 editable, and the save bar of the Server page writing the
 * changed keys (criteria 130 to 141). */
function Content({
  doc,
  onDirty,
  onReload,
}: {
  doc: PresetDoc
  onDirty: (dirty: boolean) => void
  /** Reads the preset again, the edits discarded (145). */
  onReload: () => void
}) {
  const fid = useId()
  const [loaded, setLoaded] = useState<Loaded>({
    revision: doc.revision,
    values: doc.values,
  })
  const [draft, setDraft] = useState<Record<string, string>>(doc.values)
  const [phase, setPhase] = useState<
    'idle' | 'saving' | 'saved' | 'saved-restart'
  >('idle')
  const [alert, setAlert] = useState<SaveAlert | null>(null)
  // The field a save refusal marked (142).
  const fields = useFieldErrors<string>()
  // A second Save while one runs sends nothing (137).
  const saving = useRef(false)
  const bar = useSaveBarPadding()

  // 132: a field is changed when its control's state is not the state the read
  // gave it, so picking the same option, or the extra option of 87 that a read
  // value became, is no change.
  const dirty = editableRows().some(
    (row) => selectState(row, draft) !== selectState(row, loaded.values),
  )
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

  /** The changed keys as the body of 133 spells them. */
  function changed(): Record<string, string | null> {
    const values: Record<string, string | null> = {}
    for (const row of editableRows()) {
      const state = selectState(row, draft)
      if (state === selectState(row, loaded.values)) continue
      values[row.key] = selectSend(row, state)
    }
    return values
  }

  /** Save changes: the bar of 135 to 149. */
  async function save() {
    if (saving.current || !dirty) return
    const sent = draft
    const values = changed()
    saving.current = true
    setPhase('saving')
    setAlert(null)
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
      // stays on screen, dirty against the new loaded values (01's 33b).
      setDraft((d) => (d === sent ? next.values : d))
      setPhase(answer.running ? 'saved-restart' : 'saved')
      return
    }
    setPhase('idle')
    switch (answer.kind) {
      // The session's own answers come with 2h.3; today they read as 139.
      case 'signed-out':
      case 'revoked':
        setAlert({ text: t('save-failed') })
        return
      case 'changed-elsewhere':
        setAlert({
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
          setAlert({ text: t('save-refused', answer.text) })
          if (field !== undefined) fields.set({ [field]: answer.text })
        })
        if (field !== undefined) focusField(field)
        return
      }
      case 'failed':
        setAlert({ text: answer.text })
    }
  }

  /** Discard: the controls back to the loaded state, the alert and the status
   * cleared, focus untouched (136). */
  function discard() {
    if (saving.current || !dirty) return
    setDraft(loaded.values)
    setAlert(null)
    fields.set({})
    setPhase('idle')
  }

  return (
    <>
      {!doc.enabled && <p>{t('preset-off')}</p>}
      <section>
        <h2>{t('preset-settings-title')}</h2>
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
          errors={fields.errors}
        />
        <SaveBar
          bar={bar}
          status={status}
          alert={alert}
          dirty={dirty}
          saving={phase === 'saving'}
          reloadId={reloadId}
          onReload={alert?.action === 'reload' ? askReload.press : undefined}
          saveId="preset-save"
          onDiscard={discard}
          onSave={save}
        />
      </section>
      {askReload.asking && (
        <DiscardDialog onKeep={askReload.keep} onDiscard={askReload.confirm} />
      )}
    </>
  )
}
