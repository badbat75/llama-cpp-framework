// The manager's instances and the state of each, read from the bff's
// /api/instances.
import { useEffect, useState } from 'react'
import { t } from './strings.ts'

type Instance = { id: string; state: string }

/** What the page shows: the instances, or the key of the message saying why not. */
type Answer = Instance[] | { error: string }

/** The label of a state, the manager's name shown as is when the page does not know it. */
function stateLabel(state: string): string {
  switch (state) {
    case 'active':
      return t('instance-state-active')
    case 'inactive':
      return t('instance-state-inactive')
    case 'activating':
      return t('instance-state-activating')
    case 'deactivating':
      return t('instance-state-deactivating')
    case 'failed':
      return t('instance-state-failed')
    default:
      return t('instance-state-other', state)
  }
}

/** The message of an error key the bff answers; anything else is a failure. */
function errorText(key: string): string {
  switch (key) {
    case 'unavailable':
      return t('instances-manager-unavailable')
    case 'not-authorized':
      return t('instances-not-authorized')
    default:
      return t('instances-failed')
  }
}

function isInstance(value: unknown): value is Instance {
  if (typeof value !== 'object' || value === null) return false
  const { id, state } = value as { id?: unknown; state?: unknown }
  return typeof id === 'string' && id !== '' && typeof state === 'string'
}

/** The instances from the reply, or the error key; never the reply's text. */
async function readInstances(): Promise<Answer> {
  try {
    const reply = await fetch('/api/instances')
    const body: unknown = await reply.json()
    if (!reply.ok) {
      const key = (body as { error?: unknown } | null)?.error
      const known =
        (reply.status === 503 && key === 'unavailable') ||
        (reply.status === 502 && key === 'not-authorized')
      return { error: known ? (key as string) : 'failed' }
    }
    if (!Array.isArray(body) || !body.every(isInstance)) return { error: 'failed' }
    return body
  } catch {
    return { error: 'failed' }
  }
}

export default function Instances() {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)

  useEffect(() => {
    let live = true
    readInstances().then((a) => {
      if (live) setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [])

  return (
    <section>
      <h2>{t('instances-title')}</h2>
      <Body answer={answer} />
    </section>
  )
}

/** The instances as a table, or the message in their place. */
function Body({ answer }: { answer: Answer | undefined }) {
  if (answer === undefined) {
    return null
  }
  if (!Array.isArray(answer)) {
    return <p>{errorText(answer.error)}</p>
  }
  if (answer.length === 0) {
    return <p>{t('instances-empty')}</p>
  }
  return (
    <table>
      <thead>
        <tr>
          <th>{t('instances-column-id')}</th>
          <th>{t('instances-column-state')}</th>
        </tr>
      </thead>
      <tbody>
        {answer.map((i) => (
          <tr key={i.id}>
            <td>{i.id}</td>
            <td>{stateLabel(i.state)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
