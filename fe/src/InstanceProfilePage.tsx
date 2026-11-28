// One instance profile's page (#/profiles/instance/<id>, signed in). Brief 26
// 26.5 is its head: the link back to the list, the id as the h1, and the row
// of the instances that run it. The sections that read and write the profile
// (26.6 on) are the next slice, so today the page is where the list's create
// lands, and what a reader sees of a profile is the head.
import { useEffect, useState } from 'react'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import { Icon } from './Icon.tsx'
import { InstanceLink, type NameOf } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { LoadingText } from './Skeleton.tsx'
import type { SignedOut } from './session.ts'
import { t } from './strings.ts'

/** What one profile's read gave: the head's facts, a message, or the session.
 *  The entries with their placement settings, and the `revision` the sections
 *  of 26.6 on send, are that slice's read. */
type Answer =
  | { usedBy: string[] }
  /** 404 `no-instance-profile-named`: the page says so (26.5). */
  | 'missing'
  | 'failed'
  | 'signed-out'
  | 'revoked'

async function readProfile(id: string): Promise<Answer> {
  try {
    const reply = await call(`/api/profiles/instance/${encodeURIComponent(id)}`)
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      return apiError(reply.body)?.key === 'no-instance-profile-named'
        ? 'missing'
        : 'failed'
    }
    const body = reply.body
    if (!isRecord(body)) return 'failed'
    const usedBy = Array.isArray(body.usedBy)
      ? body.usedBy.filter((id): id is string => typeof id === 'string')
      : []
    return { usedBy }
  } catch {
    return 'failed'
  }
}

export default function InstanceProfilePage({
  id,
  nameOf,
  onSignedOut,
}: {
  /** The id in the route: the h1, and the path of the read. */
  id: string
  /** The name of an instance by its id: the Used by row says the word the
   *  menu uses, with the shared-name rule of 22 D4.37. */
  nameOf: NameOf
  /** A read answered 401, or a session read revoked. */
  onSignedOut: (reason: SignedOut) => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)

  useEffect(() => {
    let live = true
    readProfile(id).then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      else setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut])

  return (
    <>
      {/* The link back to the list, before the h1 (26.5, as the server
          profile's page does, 16 17a.5). */}
      <p className="model-profile-back">
        <a href="#/profiles/instance">
          <Icon name="chevron-left" />
          {t('instance-profile-back')}
        </a>
      </p>
      {/* The id alone as the h1 (26.5): the id is the profile's name, and the
          create of 26.4 lands here with the focus on it. */}
      <PageHeading text={id} />
      {answer === undefined ? (
        <LoadingText text={t('instance-profiles-loading')} />
      ) : typeof answer === 'object' ? (
        <dl className="assets">
          <dt>{t('instance-profiles-column-used-by')}</dt>
          {answer.usedBy.length === 0 ? (
            <dd className="muted">{t('instance-profiles-unused')}</dd>
          ) : (
            <dd>
              {answer.usedBy.map((used, n) => (
                <span key={used}>
                  {n > 0 && ', '}
                  <InstanceLink id={used} nameOf={nameOf} />
                </span>
              ))}
            </dd>
          )}
        </dl>
      ) : answer === 'missing' ? (
        // The read says the profile is not there: the page keeps the back link
        // and the id, and says so (26.5).
        <p>{t('instance-profile-missing', id)}</p>
      ) : answer === 'failed' ? (
        <p>{t('instance-profiles-failed')}</p>
      ) : (
        <LoadingText text={t('instance-profiles-loading')} />
      )}
    </>
  )
}
