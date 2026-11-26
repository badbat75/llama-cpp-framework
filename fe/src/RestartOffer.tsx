// The restart offer a save or a switch leaves where its answer says a restart
// is needed (fe/briefs/16-server-profiles-and-instances.md 17d, as its "17d
// check (2026-11-26)" amends it: 17d.1a, 17d.2a, 17d.3a, 17d.4a, 17d.9a): one
// secondary button per instance id, each sending the instance's restart, the
// answer of the click read in the offer's own status line and its failures
// told in an alert of its own. The instance is named by its display name
// (22 D4.37), read from the app's instance list; an id the list lacks shows as
// the id. The offer belongs to the write that made it: the page clears it at
// the next one. A restart the session refuses says so with a Sign in: the
// sign-in the page keeps takes that alert away, and the page gives the focus
// back to the button whose restart was refused (17d review D1). The text is in
// strings/en_US.json.
import { useEffect, useId, useRef, useState } from 'react'
import { Icon } from './Icon.tsx'
import { failureText, sendAction } from './run.ts'
import { t } from './strings.ts'

export function RestartOffer({
  ids,
  nameOf,
  renewals,
  onChanged,
  onSignIn,
}: {
  ids: readonly string[]
  nameOf: (id: string) => string
  /** The page's sign-ins: the one its dialog kept drops the offer's alert. */
  renewals: number
  onChanged: () => void
  /** The alert's Sign in, opening the page's dialog for `controlId`: the id of
   *  the button whose restart failed, so the page gives it the focus back. */
  onSignIn: (controlId: string) => void
}) {
  const uid = useId()
  // The instance whose restart is in flight: every button of the offer is
  // switched off while it runs (17d.2).
  const [running, setRunning] = useState<string | null>(null)
  const [status, setStatus] = useState('')
  // The alert and the sign-in count it was raised at: a sign-in the page kept
  // raises that count, and the alert goes with it (17d review D1).
  const [alert, setAlert] = useState<{
    text: string
    signIn: boolean
    at: number
  } | null>(null)
  // The button a restart was made with, for the page's dialog (17d review D1).
  const used = useRef('')
  const mounted = useRef(true)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  if (ids.length === 0) return null
  const shown = alert === null || alert.at === renewals ? alert : null

  const restart = async (id: string) => {
    used.current = `${uid}-restart-${id}`
    setRunning(id)
    setStatus(t('restart-offer-running', nameOf(id)))
    setAlert(null)
    const name = nameOf(id)
    const result = await sendAction(id, 'restart')
    if (!mounted.current) return
    setRunning(null)
    if (result === 'ok') {
      setStatus(t('restart-offer-sent', name))
      onChanged()
      return
    }
    // A restart that timed out leaves the instance's state unknown: the list is
    // read again (16 17d.2a, 17f.10).
    if (result === 'client-timeout') onChanged()
    setStatus('')
    setAlert({
      text: failureText(name, result),
      signIn: result === 'signed-out' || result === 'revoked',
      at: renewals,
    })
  }

  return (
    <div className="restart-offer">
      {/* The row carries the run row's class beside its own (17d review D1):
          the run row's rules lay each verb's icon beside the words it says, and
          the offer's verbs are the same verbs. */}
      <div className="restart-offer-buttons run-actions">
        {ids.map((id) => (
          <button
            key={id}
            id={`${uid}-restart-${id}`}
            type="button"
            className="secondary"
            aria-disabled={running !== null ? 'true' : undefined}
            onClick={() => {
              // A switched-off button sends nothing (16 17c.6a).
              if (running !== null) return
              void restart(id)
            }}
          >
            <Icon name="rotate-cw" />
            {t('restart-offer-button', nameOf(id))}
          </button>
        ))}
      </div>
      <p role="status" className="restart-offer-status">
        {status}
      </p>
      {shown !== null && (
        <p role="alert" className="restart-offer-alert">
          {shown.text}
        </p>
      )}
      {shown?.signIn && (
        <button
          id={`${uid}-sign-in`}
          type="button"
          className="secondary"
          onClick={() => onSignIn(used.current)}
        >
          {t('session-sign-in')}
        </button>
      )}
    </div>
  )
}
