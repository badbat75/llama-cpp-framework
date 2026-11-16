// A preset's page (#/models/<id>, signed in), read-only: the way back to the
// list, the id as the heading, and the preset read once each time the page
// opens from GET /api/presets/<id>, or the message saying why it is not there
// (fe/briefs/02-models.md, sections 2d.1 to 2d.4). Below the heading, the
// Engine field, the Assets card as text and the plain cards of the settings
// (fe/briefs/02e-preset-cards.md, section 1): every control of a card is
// disabled, the page shows the file and cannot change it yet, so there is
// nothing to save.
import { Fragment, useEffect, useState } from 'react'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import Card from './Card.tsx'
import { SelectField } from './Field.tsx'
import PageHeading from './PageHeading.tsx'
import PresetCards from './PresetCards.tsx'
import { presetIdUnreadable } from './route.ts'
import { t } from './strings.ts'

/** One preset as the read gives it, the shape of criterion 81. */
type PresetDoc = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
  values: Record<string, string>
  projectUrl: string
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
}: {
  /** The id from the address, while the read has not answered. */
  id: string
  onSignedOut: (byManager: boolean) => void
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
      <Body id={id} answer={answer} />
    </>
  )
}

/** The page's content: the loaded preset, or the message. */
function Body({ id, answer }: { id: string; answer: Answer | undefined }) {
  if (presetIdUnreadable(id)) {
    return <p>{t('preset-not-found', id)}</p>
  }
  if (answer === undefined || answer === 'signed-out' || answer === 'revoked') {
    return <p role="status">{t('preset-loading')}</p>
  }
  if ('doc' in answer) return <Content doc={answer.doc} />
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

/** The loaded preset, read-only: the line for a switched-off preset, the
 * Engine field, and the Assets card (fe/briefs/02-models.md, 2d.5 to 2d.7).
 * Picking the engine changes nothing in this slice, so there is no save. */
function Content({ doc }: { doc: PresetDoc }) {
  return (
    <>
      {!doc.enabled && <p>{t('preset-off')}</p>}
      <section>
        <h2>{t('preset-settings-title')}</h2>
        <p className="notice">{t('preset-read-only')}</p>
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
        <PresetCards values={doc.values} />
      </section>
    </>
  )
}
