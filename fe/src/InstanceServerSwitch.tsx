// The instance page's Server profile switch (fe/briefs/16-server-profiles-and-instances.md
// 17d, as its "17d check (2026-11-26)" amends it: 17d.6a, 17d.7a and 17d.9a
// replace 17d.6, 17d.7 and 17d.9): a select of the server profiles no instance
// runs, read from `GET /api/profiles/server` when the page opens and after each
// switch, and a button that writes the choice through
// `PUT /api/instances/<id>/profiles/server`. The page carries its own Restart
// in its head, so the switch shows no restart offer (17d.6a): a 200 with
// `running: true` says the answer in the section's one status line. A refusal
// with the field `serverProfile` marks the select and puts the focus on it;
// every other one goes to the alert, with a Sign in when the session ended. The
// text is in strings/en_US.json.
import { useCallback, useEffect, useState } from 'react'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import { FieldError, Hint } from './Field.tsx'
import { readPool, type Pool, type PoolRow } from './Instances.tsx'
import { t } from './strings.ts'

const JSON_HEADERS = { 'Content-Type': 'application/json' }

/** A write's answer: the file written, with the instance running on the new
 *  profile, or the text of the refusal, where it goes (the select's field, or
 *  the alert), whether a Sign in belongs to it, and whether the pool is read
 *  again. */
type Written =
  | { ok: true; running: boolean }
  | {
      ok: false
      text: string
      signIn: boolean
      field: boolean
      rereadPool: boolean
    }

/** The switch's refusal, by the key (16 17d.8): the checks the manager ran on
 *  the profile, the refusals of the record, and a 422 key the fe has no text
 *  for. */
function refusalText(
  id: string,
  status: number,
  key: string,
  args: string[],
): string {
  switch (key) {
    case 'model-profile-over-memory':
      return t('api-model-profile-over-memory', ...args)
    case 'load-on-startup-over-max':
      return t('api-load-on-startup-over-max', ...args)
    case 'client-address-in-use':
      return t('api-client-address-in-use', ...args)
    case 'client-unresolvable-host':
      return t('api-client-unresolvable-host', ...args)
    case 'client-server-profile-in-use':
      return t('api-client-server-profile-in-use', ...args)
    case 'client-no-server-profile':
      return t('api-client-no-server-profile', ...args)
    case 'client-no-instance':
      return t('instance-not-found', args[0] ?? id)
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-unavailable':
      return t('api-client-unavailable')
    case 'client-service-failed':
      return t('api-client-service-failed')
    // A 504 reached the manager: the write may have happened (16 17f.10).
    case 'client-timeout':
      return t('write-timeout')
    default:
      return status === 422 ? t('api-other', key) : t('request-failed')
  }
}

/** Writes the switch: `profile` is the id chosen. */
async function writeSwitch(id: string, profile: string): Promise<Written> {
  try {
    const reply = await call(
      `/api/instances/${encodeURIComponent(id)}/profiles/server`,
      {
        method: 'PUT',
        headers: JSON_HEADERS,
        body: JSON.stringify({ serverProfile: profile }),
      },
    )
    if (reply.ok)
      return {
        ok: true,
        running: isRecord(reply.body) && reply.body.running === true,
      }
    if (reply.status === 401)
      return {
        ok: false,
        text: t('session-ended-retry'),
        signIn: true,
        field: false,
        rereadPool: false,
      }
    if (sessionRevoked(reply))
      return {
        ok: false,
        text: t('session-revoked'),
        signIn: false,
        field: false,
        rereadPool: false,
      }
    const refusal = apiError(reply.body)
    const key = refusal?.key ?? ''
    const args = refusal?.args ?? []
    // The pool the select lists is out of date for these two: the profile is
    // gone, or someone took it (16 17d.8).
    const gone =
      key === 'client-no-server-profile' ||
      key === 'client-server-profile-in-use'
    return {
      ok: false,
      text: refusalText(id, reply.status, key, args),
      signIn: false,
      field: isRecord(reply.body) && reply.body.field === 'serverProfile',
      rereadPool: gone,
    }
  } catch {
    return {
      ok: false,
      text: t('request-failed'),
      signIn: false,
      field: false,
      rereadPool: false,
    }
  }
}

/** The pool read's two state values: the rows it gave, or the words of a read
 *  that failed, with the Sign in when it says the session is gone. */
function poolState(answer: Pool): {
  pool: PoolRow[] | null
  failed: { text: string; signIn: boolean } | null
} {
  return 'failed' in answer
    ? { pool: null, failed: { text: answer.failed, signIn: answer.signIn } }
    : { pool: answer.rows, failed: null }
}

export function InstanceServerSwitch({
  id,
  fieldId,
  renewals,
  onSignIn,
  onSwitched,
}: {
  id: string
  /** The ids of the section's controls, as the page gives them. */
  fieldId: string
  /** The page's sign-ins: each one reads the pool again. */
  renewals: number
  /** The alert's Sign in, opening the page's dialog for `controlId`. */
  onSignIn: (controlId: string) => void
  /** A switch wrote: the page reads its instances list again. */
  onSwitched: () => void
}) {
  // The pool, null while the read is awaited, and the words of a read that
  // failed.
  const [pool, setPool] = useState<PoolRow[] | null>(null)
  const [failed, setFailed] = useState<{
    text: string
    signIn: boolean
  } | null>(null)
  const [chosen, setChosen] = useState('')
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('')
  const [error, setError] = useState<string | undefined>(undefined)
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )

  /** The pool put over the section: the rows, or the words of a read that
   *  failed, with the Sign in when it says the session is gone. */
  const applyPool = useCallback((answer: Pool) => {
    const read = poolState(answer)
    setPool(read.pool)
    setFailed(read.failed)
  }, [])

  // The pool of the opening, and the pool of every sign-in the page answers
  // (`renewals`): the free profiles are the manager's, never the page's.
  useEffect(() => {
    void readPool().then(applyPool)
  }, [applyPool, renewals])

  const free = (pool ?? []).filter((row) => row.instance === null)
  const canSwitch = !busy && chosen !== ''

  const send = async () => {
    if (!canSwitch) return
    const profile = chosen
    setBusy(true)
    setAlert(null)
    setError(undefined)
    setStatus(t('instance-server-switching'))
    const result = await writeSwitch(id, profile)
    if (result.ok) {
      setStatus(
        result.running
          ? t('instance-server-switched-restart', profile)
          : t('instance-server-switched', profile),
      )
    } else {
      setStatus('')
      if (result.field) {
        // The refusal names the select: its text goes under it, and the focus
        // goes there (16 17d.8).
        setError(result.text)
        document.getElementById(`${fieldId}-control`)?.focus()
      } else {
        setAlert({ text: result.text, signIn: result.signIn })
      }
    }
    // The record and the pool follow the write; a refusal that says the pool is
    // out of date reads it too (16 17d.7a, 17d.8).
    if (result.ok) {
      setChosen('')
      onSwitched()
    }
    if (result.ok || result.rereadPool) await readPool().then(applyPool)
    setBusy(false)
  }

  return (
    <div className="instance-server-switch">
      {failed !== null ? (
        <>
          <p>{failed.text}</p>
          {failed.signIn && (
            <button
              id={`${fieldId}-sign-in`}
              type="button"
              className="secondary"
              onClick={() => onSignIn(`${fieldId}-sign-in`)}
            >
              {t('session-sign-in')}
            </button>
          )}
        </>
      ) : pool === null ? null : free.length === 0 ? (
        <>
          <p>{t('instance-server-no-free')}</p>
          <p>
            <a href="#/profiles/server">{t('instance-new-profiles-link')}</a>
          </p>
        </>
      ) : (
        <div className="field">
          <div className="field-label">
            <label htmlFor={`${fieldId}-control`}>
              {t('instance-server-switch-label')}
            </label>
          </div>
          <div className="field-control">
            <select
              id={`${fieldId}-control`}
              value={chosen}
              aria-disabled={busy || undefined}
              aria-describedby={
                error === undefined
                  ? `${fieldId}-hint`
                  : `${fieldId}-hint ${fieldId}-error`
              }
              aria-invalid={error === undefined ? undefined : true}
              onChange={(e) => {
                // The choice alone sends nothing (16 17d.7).
                setChosen(e.target.value)
                setError(undefined)
              }}
            >
              <option value="">{t('instance-new-server-choose')}</option>
              {free.map((row) => (
                <option key={row.id} value={row.id}>
                  {row.port !== null && row.port > 0
                    ? t('instance-server-option', row.id, String(row.port))
                    : row.id}
                </option>
              ))}
            </select>
            {/* The button that writes the choice follows its select in the
                control cell, at its own width (17d review's re-check D3): the
                pair reads as one field and its action, and the row wraps at
                320 px, where `.field-control` wraps. The hint sits under both,
                after the button. */}
            <button
              id={`${fieldId}-switch`}
              type="button"
              className="secondary"
              aria-disabled={canSwitch ? undefined : 'true'}
              onClick={() => void send()}
            >
              {t('instance-server-switch')}
            </button>
          </div>
          <Hint
            id={`${fieldId}-hint`}
            text={t('instance-server-switch-hint')}
            open
          />
          {error !== undefined && (
            <FieldError id={`${fieldId}-error`} text={error} />
          )}
        </div>
      )}
      <p role="status" className="instance-server-switch-status">
        {status}
      </p>
      {alert !== null && (
        <p role="alert" className="instance-server-switch-alert">
          {alert.text}
        </p>
      )}
      {alert?.signIn && (
        <button
          id={`${fieldId}-sign-in`}
          type="button"
          className="secondary"
          onClick={() => onSignIn(`${fieldId}-switch`)}
        >
          {t('session-sign-in')}
        </button>
      )}
    </div>
  )
}
