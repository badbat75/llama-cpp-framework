// The manager's instances and the state of each, read from the bff's
// /api/instances.
import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  apiError,
  call,
  isRecord,
  sessionEnded,
  sessionRevoked,
} from './api.ts'
import { Chip, ChipList } from './Chip.tsx'
import { Icon } from './Icon.tsx'
import {
  ACTIONS,
  failureText,
  inactiveIn,
  runIcon,
  runLabel,
  runningText,
  sendAction,
  settledText,
  type Action,
} from './run.ts'
import { instanceHref } from './route.ts'
import type { SignedOut } from './session.ts'
import { Loading, Lines } from './Skeleton.tsx'
import { StateShape, stateLabel } from './StateBadge.tsx'
import { t } from './strings.ts'

/**
 * An instance (fe/briefs/12-instance-tags.md): the id, the engine and the
 * state, plus the port of its server profile (0 when it has none), the two
 * lists of tags, the union and the instance's own, whether its service
 * starts at boot (fe/briefs/13-node-about.md, 13 N.1), and the name of its
 * server profile (fe/briefs/20-nodes-menu.md, 20 N.2).
 */
export type Instance = {
  id: string
  engine: string
  state: string
  port: number
  tags: string[]
  instanceTags: string[]
  /** The list's answer for that one instance, null when it has none. */
  boot: boolean | null
  /** The name of the server profile its record carries, null when it names
   * none (fe/briefs/20-nodes-menu.md, 20 N.2): the nav's link shows it, the
   * Overview's tile does not. */
  serverProfile: string | null
  /** The free label the user gave the instance (fe/briefs/16-server-profiles-and-instances.md,
   * 16 17f.1), null when it has none: `instanceName` is what every place shows. */
  label: string | null
}

/**
 * The name of an instance (16 17f.1): its label as the bff gives it when it is
 * a non-empty string, else `instance-name-auto` with the engine and the port
 * when the port is above 0, else the id. Every place that shows an instance
 * uses this, and shows the id beside it except when the name IS the id.
 */
export function instanceName(item: Instance): string {
  return item.label !== null
    ? item.label
    : item.port > 0
      ? t('instance-name-auto', item.engine, String(item.port))
      : item.id
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

/** This instance in a successful answer, undefined when the answer is a failure
 * or the list has no such instance (the page's `instance-not-found`). */
export function findInstance(answer: Answer, id: string): Instance | undefined {
  return Array.isArray(answer) ? answer.find((i) => i.id === id) : undefined
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

/** `boot` of an item: the boolean, null for anything else, as a list that
 * carries no answer for that instance (fe/briefs/13-node-about.md, 13 N.1). */
function bootIn(item: Item): boolean | null {
  return typeof item.boot === 'boolean' ? item.boot : null
}

/** `serverProfile` of an item: the name as the manager spells it, null when
 * the item names none (fe/briefs/20-nodes-menu.md, 20 N.2): missing, null,
 * empty or not a string is no profile. */
function serverProfileIn(item: Item): string | null {
  const { serverProfile } = item
  return typeof serverProfile === 'string' && serverProfile !== ''
    ? serverProfile
    : null
}

/** `label` of an item: the label as the manager gives it, null when the
 * instance has none (16 17f.1): missing, null, the empty string or not a
 * string is no label. A label of spaces is a label, kept as it is. */
function labelIn(item: Item): string | null {
  return typeof item.label === 'string' && item.label !== '' ? item.label : null
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
    boot: bootIn(item),
    serverProfile: serverProfileIn(item),
    label: labelIn(item),
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

/** Writes an instance's label (`PUT /api/instances/<id>/label`, 16 17f.6):
 * 'saved' on 204, 'signed-out' on 401, 'revoked' on a 403
 * `session-not-administrator`, a refusal as its key and args, else the answer's
 * key or 'failed'. The body is the text trimmed, `""` for the clear. */
export async function writeLabel(
  id: string,
  label: string,
): Promise<'saved' | 'signed-out' | 'revoked' | TagsRefusal | string> {
  try {
    const reply = await call(`/api/instances/${encodeURIComponent(id)}/label`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label }),
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

/** Reads the node's server profiles (`GET /api/profiles/server`, the route of
 * fe/briefs/api-contract.md, the profiles the new-tag pill suggests from,
 * 15 C2): its body on 2xx, null for anything else, the session's ending
 * included. The suggestion is a help, so a read that failed says nothing. */
export async function readServerProfiles(): Promise<unknown> {
  try {
    const reply = await call('/api/profiles/server')
    return reply.ok ? reply.body : null
  } catch {
    return null
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

/**
 * The name of every instance of the app's list, by id (16 17f.4): a page that
 * names an instance it did not read, like a server profile's "Used by", says it
 * by the word the menu uses. The id for an instance the list does not have
 * (before the read, or one the manager removed).
 */
export function useInstanceNames(
  store: InstancesStore,
): (id: string) => string {
  const [names, setNames] = useState<Record<string, string>>({})
  useEffect(() => {
    // The list is polled, so the names are compared: an answer that changes
    // none of them asks for no re-render.
    let last = ''
    const unwatch = store.watch((answer) => {
      if (!Array.isArray(answer)) return
      const map = Object.fromEntries(answer.map((i) => [i.id, instanceName(i)]))
      const key = JSON.stringify(map)
      if (key === last) return
      last = key
      setNames(map)
    })
    return unwatch
  }, [store])
  return useCallback((id: string) => names[id] ?? id, [names])
}

/**
 * An instance as a link to its page, named as the menu names it, with the id
 * right after the link in a `span.instance-id` (16 17f.4, as brief 22 22 D4.34
 * amends it): the id is the key the reader can act on, and it is plain text,
 * so what a reader hears as the link is the name alone (2.4.4). The id is left
 * out when the name IS the id, which would say the same twice (17f.1).
 */
export function InstanceLink({
  id,
  nameOf,
}: {
  /** The instance's id: the key, and the route. */
  id: string
  /** The name of an instance, as the app's list has it. */
  nameOf: (id: string) => string
}) {
  const name = nameOf(id)
  return (
    <>
      <a href={instanceHref(id)}>{name}</a>
      {name !== id && (
        <>
          {' '}
          <span className="instance-id">{t('instance-id-shown', id)}</span>
        </>
      )}
    </>
  )
}

export default function Instances({
  store,
  host,
  onSignedOut,
}: {
  store: InstancesStore
  /** The host the shell read (fe/briefs/13-node-about.md, 13 N.5): it names
   * the section; null before the read answers and after a failure, when the
   * words of today stand. */
  host: string | null
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

  // A tile's write settles on a fresh read of the shared list (23 A1.2, and
  // 1c.41 leaves the Overview unpolluted): its answer reaches the watcher, so
  // every tile redraws with it. A later read that fails leaves the tiles the
  // first read drew: the Overview's own message is the first read's (S1b.2).
  useEffect(
    () =>
      store.watch((a) => {
        if (Array.isArray(a)) setAnswer(a)
      }),
    [store],
  )

  return (
    <section>
      <h2>{host ?? t('instances-title')}</h2>
      <Body answer={answer} store={store} />
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
  store,
}: {
  answer: Answer | undefined
  /** The app's list, so a tile's write settles on its fresh read (23 A1.2). */
  store: InstancesStore
}) {
  if (answer === undefined) {
    // The tiles' skeleton (17a.3): the shape of the list to come, two tiles of
    // three lines, so the section is no longer empty while the read runs.
    return (
      <Loading
        text={t('instances-loading')}
        shape={
          // The `.tiles` grid of the answer (17a.3, the U17a review): the two
          // skeleton tiles sit in the same grid, so the answer is not a jump in
          // width and count.
          <div className="tiles">
            <div className="tile">
              <Lines n={3} />
            </div>
            <div className="tile">
              <Lines n={3} />
            </div>
          </div>
        }
      />
    )
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
        <Tile key={i.id} instance={i} engines={engines} store={store} />
      ))}
    </ul>
  )
}

/**
 * One line of a write's answer: its words, and the link to the instance page
 * when the answer sends the user there (23 A1.4, 24 A1.2 D2).
 */
type WriteLine = { text: string; link: boolean }

/**
 * One line of a write's answer, in the row under the verbs (23 A1.2): its
 * words, and the link to the instance page when the answer asks the user to
 * go there, as the 17c refusal and the accepted write that has not settled
 * do (23 A1.4, 24 A1.2 D2).
 */
function WriteAnswer({
  role,
  className,
  id,
  line,
}: {
  /** `status` for the line that is always there, `alert` for a failure. */
  role: 'status' | 'alert'
  /** The line's class, when the sheet styles it. */
  className?: string
  /** The instance the link leads to. */
  id: string
  /** The line to show, null for an empty one. */
  line: WriteLine | null
}) {
  return (
    <p role={role} className={className}>
      {line?.text ?? ''}
      {line?.link && (
        <>
          {' '}
          <a href={instanceHref(id)}>{t('run-open-page')}</a>
        </>
      )}
    </p>
  )
}

/**
 * One tile: the id, the state, the engine when it tells the instances apart,
 * and the facts: the port, the tags and the boot state. Every fact comes with
 * the list, so its row is there with the tile (12 T1.2, and 13 N.1 for boot:
 * the Overview sends no boot read of its own). A fact with nothing to say has
 * no row, and a tile with no fact at all has no `dl`. The tags are the fixed
 * chips of brief 17 U17b, in the order of `tags`, the instance's own and the
 * profile's alike: the tile does not tell them apart, as it did not before.
 * The tile ends with the instance page's run verbs (23 A1.1), and the verbs
 * carry the write's answer in their own row (23 A1.2).
 */
function Tile({
  instance,
  engines,
  store,
}: {
  instance: Instance
  /** The engine line is there: the engines tell the instances apart. */
  engines: boolean
  /** The app's list: the write settles on one fresh read of it (23 A1.2). */
  store: InstancesStore
}) {
  const { id, engine, state, port, tags, boot } = instance
  // The name, and the id beside it except when the name IS the id (16 17f.3):
  // the eye reads `ID default` under the head, the reader says it inside the
  // link, so two tiles labelled alike are told apart.
  const name = instanceName(instance)
  const idShown = name !== id
  // This tile's write (23 A1.2): one at a time, its answer under the verbs.
  const [busy, setBusy] = useState<Action | null>(null)
  const [status, setStatus] = useState<WriteLine | null>(null)
  const [alert, setAlert] = useState<WriteLine | null>(null)

  async function run(action: Action) {
    if (busy !== null || inactiveIn(state, action)) return
    setBusy(action)
    setAlert(null)
    setStatus({ text: runningText(action), link: false })
    const result = await sendAction(id, action)
    if (result === 'ok') {
      // The Overview is not polled (1c.41): the write settles on one fresh
      // read of the shared list, whose answer redraws every tile.
      const answer = await store.fresh()
      const found = findInstance(answer, id)
      setBusy(null)
      if (found === undefined) {
        setStatus(null)
      } else {
        // The one fresh read usually finds the instance still moving, so the
        // line says the write went and links to the page that follows the
        // state (24 A1.2 D2).
        const settled = settledText(action, found.state)
        setStatus(
          settled === null
            ? { text: t('run-requested'), link: true }
            : { text: settled, link: false },
        )
      }
      return
    }
    setBusy(null)
    setStatus(null)
    setAlert({
      text: failureText(name, result),
      // The 17c start refusal is fixed on the instance page (23 A1.4).
      link: result === 'instance-no-model-profiles',
    })
    // A 504 is not confirmed: the state shown must follow the manager, so the
    // list is read now (16 17f.10): the watcher redraws every tile.
    if (result === 'client-timeout') void store.fresh()
  }

  return (
    <li className="tile">
      <div className="tile-head">
        <a className="tile-id" href={instanceHref(id)}>
          {name}
          {idShown && (
            <span className="visually-hidden">{t('instance-id-said', id)}</span>
          )}
        </a>
        <span className="badge">
          <StateShape state={state} />
          {stateLabel(state)}
        </span>
      </div>
      {idShown && <p className="instance-id">{t('instance-id-shown', id)}</p>}
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
              <dd>
                <ChipList>
                  {tags.map((tag) => (
                    <Chip key={tag} tag={tag} />
                  ))}
                </ChipList>
              </dd>
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
      <RunButtons
        state={state}
        busy={busy}
        nameOf={name}
        onRun={(action) => void run(action)}
      >
        <WriteAnswer role="status" id={id} line={status} />
        {alert !== null && (
          <WriteAnswer
            role="alert"
            className="run-alert"
            id={id}
            line={alert}
          />
        )}
      </RunButtons>
    </li>
  )
}

/**
 * The three run verbs of one instance, in the order every page shows them
 * (23 A1.1): the instance page's header and the Overview's tile alike, with
 * the words, icons and state rules of `run.ts`. A state leaves its verbs
 * inactive, and a write in flight leaves all three so (23 A1.2): every one is
 * `aria-disabled`, never `disabled`, so the reader reaches it and hears why.
 * `children` are the write's answer, in the row under the verbs.
 */
export function RunButtons({
  state,
  busy,
  onRun,
  ids,
  nameOf,
  children,
}: {
  /** The instance's state, as the list gives it. */
  state: string
  /** The verb whose write is in flight, null while none is. */
  busy: Action | null
  /** Asks for one verb. */
  onRun: (action: Action) => void
  /** The id of each verb, as the instance page gives them for the focus a
   * sign-in in its dialog returns (45e); a tile has none. */
  ids?: (action: Action) => string
  /** The instance the accessible name carries after the visible word
   * (23 A1.3, "Start default"); a page with one instance needs none. */
  nameOf?: string
  /** The write's answer, under the verbs. */
  children?: ReactNode
}) {
  return (
    <div className="run-actions">
      {ACTIONS.map((action) => (
        <button
          key={action}
          {...(ids === undefined ? {} : { id: ids(action) })}
          type="button"
          className="secondary"
          aria-disabled={
            busy !== null || inactiveIn(state, action) ? true : undefined
          }
          onClick={() => onRun(action)}
        >
          <Icon name={runIcon(action)} />
          {runLabel(action)}
          {nameOf !== undefined && (
            <>
              {' '}
              <span className="visually-hidden">{` ${nameOf}`}</span>
            </>
          )}
        </button>
      ))}
      {children}
    </div>
  )
}

/**
 * The tags already used, in the order of the lists they are read from
 * (brief 15 section 5, slice C2, decision 10): the two tag lists of every
 * instance of the list read `list` (the union and the instance's own, section
 * 0: the list answers the union alone when the instance has no tags of its
 * own, so both are read), then the `tags` of every server profile of the read
 * `profiles`. The tags `shown` are left out, as this instance already shows
 * them, and a tag of two rows is kept once. A reply that is not the list it
 * promises, and a read that failed, answer nothing.
 */
export function readInstanceTagSuggestions(
  list: Answer,
  profiles: unknown,
  shown: string[],
): string[] {
  const used: string[] = []
  const add = (tag: unknown) => {
    if (
      typeof tag === 'string' &&
      tag !== '' &&
      !shown.includes(tag) &&
      !used.includes(tag)
    )
      used.push(tag)
  }
  if (Array.isArray(list)) {
    for (const instance of list)
      for (const tags of [instance.tags, instance.instanceTags])
        if (Array.isArray(tags)) tags.forEach(add)
  }
  if (Array.isArray(profiles)) {
    for (const profile of profiles)
      if (isRecord(profile) && Array.isArray(profile.tags))
        profile.tags.forEach(add)
  }
  return used
}
