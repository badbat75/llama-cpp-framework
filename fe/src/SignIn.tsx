// The sign-in form (fe/briefs/01-server.md, section 2.2): at #/sign-in, in
// place of a protected page while signed out, and in the dialog a page's bar
// opens over the page (01's 36, 2h's 151).
import { useId, useRef, useState, type FormEvent } from 'react'
import { flushSync } from 'react-dom'
import type { ApiError } from './api.ts'
import Dialog from './Dialog.tsx'
import { FieldError } from './Field.tsx'
import PageHeading from './PageHeading.tsx'
import { signIn, type Session } from './session.ts'
import { t } from './strings.ts'

/** The message of a refused sign-in, picked by the key; anything else failed. */
function failureText(error: ApiError | null): string {
  switch (error?.key) {
    case 'session-bad-credentials':
      return t('api-session-bad-credentials')
    case 'session-too-many-attempts':
      return t('api-session-too-many-attempts', ...error.args)
    case 'session-unavailable':
      return t('api-session-unavailable')
    case 'session-not-administrator':
      return t('api-session-not-administrator')
    default:
      return t('signin-failed')
  }
}

export default function SignIn({
  required,
  revoked = false,
  inDialog = false,
  onSignedIn,
  onCancel,
}: {
  /** True in place of a protected page. */
  required: boolean
  /** True in place of a protected page after the manager ended the session. */
  revoked?: boolean
  /** True inside a Dialog, which holds the title: no h1, no focus taken. */
  inDialog?: boolean
  onSignedIn: (session: Session) => void
  /** In a Dialog: its Cancel button, which does what Escape does. */
  onCancel?: () => void
}) {
  const id = useId()
  // The plain-HTTP notice (fe/briefs/03-insecure-sign-in.md): read once when
  // the sign-in renders, it cannot change without a new page load. A browser
  // without the property (undefined) shows nothing.
  const insecure = window.isSecureContext === false
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [empty, setEmpty] = useState({ username: false, password: false })
  const [failure, setFailure] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  // A second submit while one runs sends nothing.
  const running = useRef(false)

  function focusField(name: 'username' | 'password') {
    document.getElementById(`${id}-${name}`)?.focus()
  }

  async function submit(event: FormEvent) {
    event.preventDefault()
    if (running.current) return
    const missing = { username: username === '', password: password === '' }
    setEmpty(missing)
    if (missing.username || missing.password) {
      setFailure(null)
      focusField(missing.username ? 'username' : 'password')
      return
    }
    running.current = true
    setBusy(true)
    setFailure(null)
    const result = await signIn(username, password)
    running.current = false
    setBusy(false)
    if ('user' in result) {
      onSignedIn(result)
      return
    }
    setFailure(failureText(result.error))
    const key = result.error?.key
    if (
      key === 'session-bad-credentials' ||
      key === 'session-unavailable' ||
      key === 'session-not-administrator'
    ) {
      setPassword('')
      // A non-administrator has to change the account, not the password.
      focusField(key === 'session-not-administrator' ? 'username' : 'password')
    }
  }

  const content = (
    <>
      {!inDialog && <PageHeading text={t('signin-title')} />}
      <p>{t('signin-intro')}</p>
      {revoked ? (
        <p role="alert" className="notice">
          {t('session-revoked')}
        </p>
      ) : (
        required && <p className="notice">{t('signin-required')}</p>
      )}
      {insecure && (
        <p id={`${id}-insecure`} className="notice">
          {t('signin-insecure')}
        </p>
      )}
      <form className="signin" onSubmit={submit} noValidate>
        {failure !== null && (
          <p role="alert" className="form-error">
            {failure}
          </p>
        )}
        <div className="stacked">
          <label htmlFor={`${id}-username`}>{t('signin-username')}</label>
          <input
            id={`${id}-username`}
            type="text"
            autoComplete="username"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            aria-invalid={empty.username || undefined}
            aria-describedby={
              empty.username ? `${id}-username-error` : undefined
            }
          />
          {empty.username && (
            <FieldError
              id={`${id}-username-error`}
              text={t('signin-empty-username')}
            />
          )}
        </div>
        <div className="stacked">
          <label htmlFor={`${id}-password`}>{t('signin-password')}</label>
          <input
            id={`${id}-password`}
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            aria-invalid={empty.password || undefined}
            aria-describedby={
              [
                empty.password ? `${id}-password-error` : null,
                insecure ? `${id}-insecure` : null,
              ]
                .filter((s) => s !== null)
                .join(' ') || undefined
            }
          />
          {empty.password && (
            <FieldError
              id={`${id}-password-error`}
              text={t('signin-empty-password')}
            />
          )}
        </div>
        <div className="signin-actions">
          <button type="submit" className="primary">
            {busy ? t('signin-busy') : t('signin-submit')}
          </button>
          {inDialog && onCancel !== undefined && (
            <button type="button" className="secondary" onClick={onCancel}>
              {t('signin-cancel')}
            </button>
          )}
        </div>
      </form>
    </>
  )
  // The page (fe/briefs/08-hearth-h7-signin.md H7.1) is the drawing and the
  // panel that holds all of this; the dialog holds exactly the same nodes it
  // held before, with no box and no drawing.
  if (inDialog) return content
  return (
    <div className="signin-page">
      <SignInArt />
      <div className="signin-panel">{content}</div>
    </div>
  )
}

/** The drawing beside the form (08-hearth-h7-signin.md H7.2 and H7.3): pure
 * decoration, so it is hidden from assistive technology, carries no text and
 * no name, and takes every colour from its classes (H7.4): the CSS gives the
 * fill and the stroke, the drawing only the geometry. */
function SignInArt() {
  return (
    <div className="signin-art" aria-hidden="true">
      <svg viewBox="0 0 420 260" focusable="false" aria-hidden="true">
        <circle className="art-sun" cx="330" cy="70" r="38" />
        <path className="art-ground" d="M0 214h420v46H0z" />
        <path
          className="art-door"
          d="M120 214V112a90 90 0 0 1 180 0v102z"
          strokeWidth="3"
        />
        <rect
          className="art-panel"
          x="160"
          y="120"
          width="100"
          height="80"
          rx="12"
          strokeWidth="3"
        />
        <rect
          className="art-bar-on"
          x="174"
          y="136"
          width="72"
          height="12"
          rx="6"
        />
        <rect
          className="art-bar"
          x="174"
          y="156"
          width="72"
          height="12"
          rx="6"
          strokeWidth="2.5"
        />
        <rect
          className="art-bar"
          x="174"
          y="176"
          width="72"
          height="12"
          rx="6"
          strokeWidth="2.5"
        />
        <circle className="art-knob" cx="236" cy="142" r="3" />
        <path
          className="art-cable"
          d="M260 196c40 0 40 18 80 18"
          strokeWidth="3"
          strokeLinecap="round"
          strokeDasharray="1 8"
        />
        <rect
          className="art-panel"
          x="320"
          y="180"
          width="64"
          height="34"
          rx="6"
          strokeWidth="3"
        />
        <path
          className="art-line"
          d="M312 214h80"
          strokeWidth="3"
          strokeLinecap="round"
        />
        <path
          className="art-stem"
          d="M60 214c0-30 10-46 26-58M86 214c0-22-6-38-20-50"
          strokeWidth="3"
          strokeLinecap="round"
        />
        <ellipse
          className="art-leaf"
          cx="86"
          cy="152"
          rx="12"
          ry="6"
          transform="rotate(-35 86 152)"
        />
        <ellipse
          className="art-leaf"
          cx="64"
          cy="162"
          rx="11"
          ry="5"
          transform="rotate(30 64 162)"
        />
      </svg>
    </div>
  )
}

/** The Sign in whose dialog is open (01's 43a, 2h's 151): `open` shows it,
 * `close` hides it and gives the focus to `focusId`, the control that answers
 * the alert. Escape and Cancel both close it with the focus back on `fromId`,
 * the Sign in that opened it. */
export function useSignInDialog(fromId: string): {
  asking: boolean
  open: () => void
  close: (focusId?: string) => void
} {
  const [asking, setAsking] = useState(false)
  return {
    asking,
    open: () => setAsking(true),
    close: (focusId = fromId) => {
      flushSync(() => setAsking(false))
      document.getElementById(focusId)?.focus()
    },
  }
}

/** The sign-in of `useSignInDialog`, in the Dialog of DESIGN.md 5.2: the page
 * stays behind it, with its own h1. */
export function SignInDialog({
  onSignedIn,
  onClose,
}: {
  onSignedIn: (session: Session) => void
  /** Escape, Cancel, and the close that follows a sign-in. */
  onClose: () => void
}) {
  return (
    <Dialog title={t('signin-title')} onClose={onClose}>
      <SignIn
        required={false}
        inDialog
        onSignedIn={onSignedIn}
        onCancel={onClose}
      />
    </Dialog>
  )
}
