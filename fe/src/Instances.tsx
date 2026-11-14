// The manager's instances and the state of each, read from the bff's
// /api/instances.
import { useEffect, useState } from 'react'
import { apiError, call } from './api.ts'
import { instanceHref } from './route.ts'
import { StateShape, stateLabel } from './StateBadge.tsx'
import { t } from './strings.ts'

export type Instance = { id: string; engine: string; state: string }

/** What the bff answered: the instances, or the key of the message saying why not. */
export type Answer = Instance[] | { error: string }

/** The message of an error key the bff answers; anything else is a failure. */
export function errorText(key: string): string {
  switch (key) {
    case 'client-unavailable':
      return t('instances-manager-unavailable')
    case 'client-not-authorized':
      return t('instances-not-authorized')
    default:
      return t('instances-failed')
  }
}

function isInstance(value: unknown): value is Instance {
  if (typeof value !== 'object' || value === null) return false
  const { id, engine, state } = value as {
    id?: unknown
    engine?: unknown
    state?: unknown
  }
  return (
    typeof id === 'string' &&
    id !== '' &&
    typeof engine === 'string' &&
    typeof state === 'string'
  )
}

/** The instances from the reply, or the error key; never the reply's text. */
export async function readInstances(): Promise<Answer> {
  try {
    const reply = await call('/api/instances')
    if (!reply.ok) {
      const key = apiError(reply.body)?.key
      const known =
        key === 'client-unavailable' || key === 'client-not-authorized'
      return { error: known ? key : 'failed' }
    }
    if (!Array.isArray(reply.body) || !reply.body.every(isInstance))
      return { error: 'failed' }
    return reply.body
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

/** The engine is shown only when it tells the instances apart. */
function manyEngines(instances: Instance[]): boolean {
  return new Set(instances.map((i) => i.engine)).size > 1
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
  const engines = manyEngines(answer)
  return (
    <table>
      <thead>
        <tr>
          <th scope="col">{t('instances-column-id')}</th>
          {engines && <th scope="col">{t('instances-column-engine')}</th>}
          <th scope="col">{t('instances-column-state')}</th>
        </tr>
      </thead>
      <tbody>
        {answer.map((i) => (
          <tr key={i.id}>
            <td>
              <a href={instanceHref(i.id)}>{i.id}</a>
            </td>
            {engines && <td>{i.engine}</td>}
            <td>
              <StateShape state={i.state} />
              {stateLabel(i.state)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
