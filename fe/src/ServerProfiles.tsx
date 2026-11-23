// The Server profiles page (#/profiles/server, signed in): the pool read once
// each time the page opens from GET /api/profiles/server, one row per profile
// in the reply's order, the id a link to its page and the instance it runs a
// link to that page. The columns do not sort: the pool is a handful of files,
// and the row's own link is where the detail is
// (fe/briefs/16-server-profiles-and-instances.md, 17a.1 to 17a.4). The list's
// read is its own: `readServerProfiles`, the tags pill's silent read, stays as
// it is (17a.3).
import { useEffect, useState } from 'react'
import { apiError, call, sessionRevoked } from './api.ts'
import PageHeading from './PageHeading.tsx'
import { instanceHref, serverProfileHref } from './route.ts'
import type { SignedOut } from './session.ts'
import { Lines, Loading, LoadingText } from './Skeleton.tsx'
import { t } from './strings.ts'

/** One row of the pool, as the page reads it. */
type ServerProfile = {
  id: string
  /** The port, null when the reply's is not a number (17a.4 reads it as 0,
   *  which is not the same as a file naming no port). */
  port: number | null
  tags: string[]
  /** The instance that runs it, null when none does. */
  instance: string | null
}

/** The row of a reply body, null when the item names nothing to open: an id
 * that is not a non-empty string has no page (17a.4). */
function rowOf(value: unknown): ServerProfile | null {
  if (typeof value !== 'object' || value === null) return null
  const item = value as Record<string, unknown>
  if (typeof item.id !== 'string' || item.id === '') return null
  return {
    id: item.id,
    port: typeof item.port === 'number' ? item.port : null,
    tags: Array.isArray(item.tags)
      ? item.tags.filter((tag): tag is string => typeof tag === 'string')
      : [],
    instance:
      typeof item.instance === 'string' && item.instance !== ''
        ? item.instance
        : null,
  }
}

/** What the read gave: the rows, the error of its message, or the session. */
type Answer =
  | ServerProfile[]
  | { error: 'client-unavailable' | 'failed' }
  | 'signed-out'
  | 'revoked'

async function readServerPool(): Promise<Answer> {
  try {
    const reply = await call('/api/profiles/server')
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      return apiError(reply.body)?.key === 'client-unavailable'
        ? { error: 'client-unavailable' }
        : { error: 'failed' }
    }
    if (!Array.isArray(reply.body)) return { error: 'failed' }
    return reply.body
      .map(rowOf)
      .filter((row): row is ServerProfile => row !== null)
  } catch {
    return { error: 'failed' }
  }
}

export default function ServerProfiles({
  onSignedOut,
}: {
  /** The session a read says is gone. */
  onSignedOut: (reason: SignedOut) => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)

  // The arrival focus (fe/DESIGN.md 2.3, 16 17a.1): the shell raises its focus
  // request on a navigation, and the session read that brings this page is an
  // arrival too, so the page gives its h1 the focus when it opens.
  useEffect(() => {
    document.getElementById('page-heading')?.focus()
  }, [])

  useEffect(() => {
    let live = true
    readServerPool().then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      else setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [onSignedOut])

  return (
    <>
      <PageHeading text={t('server-profiles-title')} />
      <section>
        <p className="intro">{t('server-profiles-intro')}</p>
        <Body answer={answer} />
      </section>
    </>
  )
}

/** The table, or the message in its place. */
function Body({ answer }: { answer: Answer | undefined }) {
  if (answer === undefined) {
    // The rows' skeleton (17a.3): three lines in the table's place, the read's
    // words in the DOM and out of sight from the first frame.
    return (
      <Loading text={t('server-profiles-loading')} shape={<Lines n={3} />} />
    )
  }
  if (answer === 'signed-out' || answer === 'revoked') {
    return <LoadingText text={t('server-profiles-loading')} />
  }
  if (!Array.isArray(answer)) {
    return (
      <p>
        {answer.error === 'client-unavailable'
          ? t('api-client-unavailable')
          : t('server-profiles-failed')}
      </p>
    )
  }
  if (answer.length === 0) {
    return <p>{t('server-profiles-empty')}</p>
  }
  return (
    <div className="table-scroll">
      <table className="server-profiles" aria-labelledby="page-heading">
        <thead>
          <tr>
            <th scope="col">{t('server-profiles-column-name')}</th>
            <th scope="col">{t('server-profiles-column-port')}</th>
            <th scope="col">{t('server-profiles-column-tags')}</th>
            <th scope="col">{t('server-profiles-column-used-by')}</th>
          </tr>
        </thead>
        <tbody>
          {answer.map((p) => (
            <tr key={p.id}>
              <td>
                <a href={serverProfileHref(p.id)}>{p.id}</a>
              </td>
              {/* The port as text: 0 is the file that names none, and an empty
                  cell says that better than a zero; a port that is not a
                  number shows 0 (17a.4). */}
              <td>
                {p.port === null ? '0' : p.port === 0 ? '' : String(p.port)}
              </td>
              <td>{p.tags.join(', ')}</td>
              {p.instance === null ? (
                <td className="muted">{t('server-profiles-unused')}</td>
              ) : (
                <td>
                  <a href={instanceHref(p.instance)}>{p.instance}</a>
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
