// The manager's instances and the state of each, read from the bff's
// /api/instances.
import { useEffect, useState } from 'react'
import {
  apiError,
  call,
  isRecord,
  sessionEnded,
  sessionRevoked,
} from './api.ts'
import { instanceHref } from './route.ts'
import type { SignedOut } from './session.ts'
import { StateShape, stateLabel } from './StateBadge.tsx'
import { t } from './strings.ts'

/**
 * An instance (fe/briefs/12-instance-tags.md): the id, the engine and the
 * state, plus the port of its server profile (0 when it has none) and the two
 * lists of tags, the union and the instance's own.
 */
export type Instance = {
  id: string
  engine: string
  state: string
  port: number
  tags: string[]
  instanceTags: string[]
}

/** What the bff answered: the instances, or the key of the message saying why
 * not, `session-required` for a read that answered 401 (09 S1b.1). */
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

/** One item of the list reply, before its port and tags are read out. */
type Item = Record<string, unknown> & {
  id: string
  engine: string
  state: string
}

function isItem(value: unknown): value is Item {
  if (typeof value !== 'object' || value === null) return false
  const { id, engine, state } = value as Record<string, unknown>
  return (
    typeof id === 'string' &&
    id !== '' &&
    typeof engine === 'string' &&
    typeof state === 'string'
  )
}

/** `key` of an item as a list of tags: the list, `[]` when it is not one: the
 * tags are extra facts, so a strange item loses them and not the list. */
function tagsIn(item: Item, key: 'tags' | 'instanceTags'): string[] {
  const tags = item[key]
  return Array.isArray(tags) && tags.every((tag) => typeof tag === 'string')
    ? (tags as string[])
    : []
}

/** `port` of an item: a whole number of the range, 0 for anything else (the
 * bff answers 0 when the server profile has no port). */
function portIn(item: Item): number {
  const { port } = item
  return typeof port === 'number' &&
    Number.isInteger(port) &&
    port > 0 &&
    port <= 65535
    ? port
    : 0
}

/** The instance an item of the reply describes. */
function instanceOf(item: Item): Instance {
  return {
    id: item.id,
    engine: item.engine,
    state: item.state,
    port: portIn(item),
    tags: tagsIn(item, 'tags'),
    instanceTags: tagsIn(item, 'instanceTags'),
  }
}

/** The tags the manager refused, with the values for its text and the status
 * that answered it (a 422 with a key the fe does not know is shown with the
 * key, any other status is a request that did not go through). */
export type TagsRefusal = { key: string; args: string[]; status: number }

/** Writes an instance's own tags (the tags route of
 * fe/briefs/12-instance-tags.md): 'saved' on 2xx, 'signed-out' on 401,
 * 'revoked' on a 403 `session-not-administrator`, a refusal as its key and
 * args, else the answer's key or 'failed'. */
export async function writeTags(
  id: string,
  tags: string[],
): Promise<'saved' | 'signed-out' | 'revoked' | TagsRefusal | string> {
  try {
    const reply = await call(`/api/instances/${encodeURIComponent(id)}/tags`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tags }),
    })
    if (reply.ok) return 'saved'
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    const refusal = apiError(reply.body)
    return refusal === null ? 'failed' : { ...refusal, status: reply.status }
  } catch {
    return 'failed'
  }
}

/** The tags of the box: split on commas and white space, empty pieces dropped,
 * order, case and repeats kept as typed (12 T1.6): the manager is the judge of
 * the rule, and its refusal says what to fix. */
export function tagsFrom(text: string): string[] {
  return text.split(/[\s,]+/).filter((tag) => tag !== '')
}

/** Two lists of tags, the same when one is a re-typing of the other, so
 * `v2,gpu` is `v2, gpu` (12 T1.13). */
export function sameTags(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((tag, i) => tag === b[i])
}

/** The instances from the reply, or the error key; never the reply's text. */
async function readInstances(): Promise<Answer> {
  try {
    const reply = await call('/api/instances')
    if (sessionEnded(reply)) return { error: 'session-required' }
    if (!reply.ok) {
      const key = apiError(reply.body)?.key
      const known =
        key === 'client-unavailable' || key === 'client-not-authorized'
      return { error: known ? key : 'failed' }
    }
    if (!Array.isArray(reply.body) || !reply.body.every(isItem))
      return { error: 'failed' }
    return reply.body.map(instanceOf)
  } catch {
    return { error: 'failed' }
  }
}

/**
 * Whether the instance starts at boot, for a tile: the value, `session-required`
 * when the read answered 401 (09 S1b.1), or null when the read has no answer
 * for the tile. The instance page reads the same route and says why a read
 * failed (its `readBoot`); the Overview is a glance, so a tile shows nothing at
 * all when there is no answer.
 */
async function readBoot(
  id: string,
): Promise<boolean | 'session-required' | null> {
  try {
    const reply = await call(`/api/instances/${encodeURIComponent(id)}/boot`)
    if (sessionEnded(reply)) return 'session-required'
    if (!reply.ok) return null
    return isRecord(reply.body) && typeof reply.body.enabled === 'boolean'
      ? reply.body.enabled
      : null
  } catch {
    return null
  }
}

/**
 * The app's one read of /api/instances (fe/briefs/01-server.md, 41): at most
 * one request in flight, a read asked meanwhile gets its answer, and every
 * answer reaches the watchers (the nav).
 */
export type InstancesStore = {
  read: () => Promise<Answer>
  /**
   * A read whose request is sent from now on: it waits for the one in
   * flight, if any, then reads (an action is settled only by such a read),
   * unless `wanted` says false by then (its answer is then the waited one's).
   */
  fresh: (wanted?: () => boolean) => Promise<Answer>
  /** Calls `listener` with every answer from now on; returns the unwatch. */
  watch: (listener: (answer: Answer) => void) => () => void
}

export function instancesStore(): InstancesStore {
  let inFlight: Promise<Answer> | null = null
  const listeners = new Set<(answer: Answer) => void>()
  const read = () => {
    inFlight ??= readInstances().then((a) => {
      inFlight = null
      for (const listener of listeners) listener(a)
      return a
    })
    return inFlight
  }
  return {
    read,
    fresh(wanted = () => true) {
      if (inFlight === null) return read()
      return inFlight.then((a) => (wanted() ? read() : a))
    },
    watch(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
  }
}

export default function Instances({
  store,
  onSignedOut,
}: {
  store: InstancesStore
  /** Raised when the read answers 401, the session being gone (09 S1b.1). */
  onSignedOut: (reason: SignedOut) => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)

  useEffect(() => {
    let live = true
    store.read().then((a) => {
      if (!live) return
      if (!Array.isArray(a) && a.error === 'session-required') {
        onSignedOut('ended')
        return
      }
      setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [store, onSignedOut])

  return (
    <section>
      <h2>{t('instances-title')}</h2>
      <Body answer={answer} onSignedOut={onSignedOut} />
    </section>
  )
}

/** The engine is shown only when it tells the instances apart. */
function manyEngines(instances: Instance[]): boolean {
  return new Set(instances.map((i) => i.engine)).size > 1
}

/** The instances as a list of tiles, or the message in their place. */
function Body({
  answer,
  onSignedOut,
}: {
  answer: Answer | undefined
  onSignedOut: (reason: SignedOut) => void
}) {
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
    // role="list": WebKit drops the list semantics of a ul whose list-style is
    // none, on purpose, and advises the explicit role (the follow-up of H6,
    // 08-hearth-h6-overview.md section 4). The count of instances is the fact.
    <ul className="tiles" role="list">
      {answer.map((i) => (
        <Tile
          key={i.id}
          instance={i}
          engines={engines}
          onSignedOut={onSignedOut}
        />
      ))}
    </ul>
  )
}

/**
 * One tile: the id, the state, the engine when it tells the instances apart,
 * and the facts: the port, the tags and the boot state. The boot read belongs
 * to the tile: it goes when the tile mounts, once, and the row shows only an
 * answer that came (no row while it is on its way, none after a failure). The
 * port and the tags come with the list, so their rows are there with the tile
 * (12 T1.2). A fact with nothing to say has no row, and a tile with no fact at
 * all has no `dl`.
 */
function Tile({
  instance,
  engines,
  onSignedOut,
}: {
  instance: Instance
  /** The engine line is there: the engines tell the instances apart. */
  engines: boolean
  onSignedOut: (reason: SignedOut) => void
}) {
  const { id, engine, state, port, tags } = instance
  // null: no row, because there is no answer to show.
  const [boot, setBoot] = useState<boolean | null>(null)

  useEffect(() => {
    let live = true
    readBoot(id).then((v) => {
      if (!live) return
      if (v === 'session-required') {
        onSignedOut('ended')
        return
      }
      if (v !== null) setBoot(v)
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut])

  return (
    <li className="tile">
      <div className="tile-head">
        <a className="tile-id" href={instanceHref(id)}>
          {id}
        </a>
        <span className="badge">
          <StateShape state={state} />
          {stateLabel(state)}
        </span>
      </div>
      {engines && <p className="tile-engine">{t('instance-engine', engine)}</p>}
      {(port > 0 || tags.length > 0 || boot !== null) && (
        <dl className="tile-facts">
          {port > 0 && (
            <div>
              <dt>{t('server-port')}</dt>
              <dd>{port}</dd>
            </div>
          )}
          {tags.length > 0 && (
            <div>
              <dt>{t('instance-tags')}</dt>
              <dd>{tags.join(', ')}</dd>
            </div>
          )}
          {boot !== null && (
            <div>
              <dt>{t('boot-label')}</dt>
              <dd>{boot ? t('instances-boot-on') : t('instances-boot-off')}</dd>
            </div>
          )}
        </dl>
      )}
    </li>
  )
}
