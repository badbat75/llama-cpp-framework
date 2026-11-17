// A preset's page (#/models/<id>, signed in): the way back to the list, the id
// as the heading, and the preset read once each time the page opens from
// GET /api/presets/<id>, or the message saying why it is not there
// (fe/briefs/02-models.md, sections 2d.1 to 2d.4). Below the heading, the
// Engine field, the Assets card as text and the cards of the settings
// (fe/briefs/02e-preset-cards.md, section 1). The Embeddings card's two
// controls are editable and the save bar at the end of the settings writes
// what changed (fe/briefs/02h-preset-save.md, sub-slice 2h.1, criteria 130 to
// 141); every other control stays disabled, and its settings are changed in
// pai-config.
import { Fragment, useEffect, useRef, useState } from 'react'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import Card from './Card.tsx'
import { SelectField } from './Field.tsx'
import PageHeading from './PageHeading.tsx'
import PresetCards, {
  editableRows,
  selectSend,
  selectState,
} from './PresetCards.tsx'
import {
  SaveBar,
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

  useEffect(() => {
    // `.` and `..` are sent to no read at all (59a): the page says there is no
    // such preset, as a 404 would.
    if (presetIdUnreadable(id)) return
    let live = true
    readPreset(id).then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked') onSignedOut(a === 'revoked')
      else setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut])

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
      <Body id={id} answer={answer} onDirty={onDirty} />
    </>
  )
}

/** The page's content: the loaded preset, or the message. */
function Body({
  id,
  answer,
  onDirty,
}: {
  id: string
  answer: Answer | undefined
  onDirty: (dirty: boolean) => void
}) {
  if (presetIdUnreadable(id)) {
    return <p>{t('preset-not-found', id)}</p>
  }
  if (answer === undefined || answer === 'signed-out' || answer === 'revoked') {
    return <p role="status">{t('preset-loading')}</p>
  }
  if ('doc' in answer) return <Content doc={answer.doc} onDirty={onDirty} />
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

/** The loaded preset as a form: the settings of 2e, the two Embeddings
 * controls of 2h.1 editable, and the save bar of the Server page writing the
 * changed keys (criteria 130 to 141). */
function Content({
  doc,
  onDirty,
}: {
  doc: PresetDoc
  onDirty: (dirty: boolean) => void
}) {
  const [loaded, setLoaded] = useState<Loaded>({
    revision: doc.revision,
    values: doc.values,
  })
  const [draft, setDraft] = useState<Record<string, string>>(doc.values)
  const [phase, setPhase] = useState<
    'idle' | 'saving' | 'saved' | 'saved-restart'
  >('idle')
  const [alert, setAlert] = useState<SaveAlert | null>(null)
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

  const status = phase === 'saving' ? 'saving' : dirty ? 'unsaved' : phase

  /** One control picked: the value it now holds, in the draft. The pick ends
   * the message the last save left in the bar: what is on screen is what the
   * bar speaks of now, and a field picked back to the saved state is clean
   * with nothing said about it (138). */
  function edit(key: string, value: string) {
    setDraft((d) => ({ ...d, [key]: value }))
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

  /** Save changes: the bar of 135 to 139. */
  async function save() {
    if (saving.current || !dirty) return
    const sent = draft
    const values = changed()
    saving.current = true
    setPhase('saving')
    setAlert(null)
    try {
      const reply = await call(`/api/presets/${encodeURIComponent(doc.id)}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ revision: loaded.revision, values }),
      })
      const body = reply.body
      // 138's 200, and nothing else: 2h.2 and 2h.3 give the keyed answers
      // their own text, and everything they do not name is 139.
      if (
        reply.status === 200 &&
        isRecord(body) &&
        typeof body.revision === 'string' &&
        typeof body.running === 'boolean'
      ) {
        const next = {
          revision: body.revision,
          values: applied(loaded.values, values),
        }
        setLoaded(next)
        // What was sent is the clean state now. An edit made during the PUT
        // stays on screen, dirty against the new loaded values (01's 33b).
        setDraft((d) => (d === sent ? next.values : d))
        setPhase(body.running ? 'saved-restart' : 'saved')
      } else {
        setAlert({ text: t('save-failed') })
        setPhase('idle')
      }
    } catch {
      // Not reachable, not JSON, the time limit: 139.
      setAlert({ text: t('save-failed') })
      setPhase('idle')
    } finally {
      saving.current = false
    }
  }

  /** Discard: the controls back to the loaded state, the alert and the status
   * cleared, focus untouched (136). */
  function discard() {
    if (saving.current || !dirty) return
    setDraft(loaded.values)
    setAlert(null)
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
        <PresetCards values={draft} loaded={loaded.values} onEdit={edit} />
        <SaveBar
          bar={bar}
          status={status}
          alert={alert}
          dirty={dirty}
          saving={phase === 'saving'}
          saveId="preset-save"
          onDiscard={discard}
          onSave={save}
        />
      </section>
    </>
  )
}
