// The sign-in form (fe/briefs/01-server.md, section 2.2): at #/sign-in, and
// in place of a protected page while signed out.
import { useId, useRef, useState, type FormEvent } from 'react'
import type { ApiError } from './api.ts'
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
  inDialog = false,
  onSignedIn,
  onCancel,
}: {
  /** True in place of a protected page. */
  required: boolean
  /** True inside a Dialog, which holds the title: no h1, no focus taken. */
  inDialog?: boolean
  onSignedIn: (session: Session) => void
  /** In a Dialog: its Cancel button, which does what Escape does. */
  onCancel?: () => void
}) {
  const id = useId()
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

  return (
    <>
      {!inDialog && <PageHeading text={t('signin-title')} />}
      <p>{t('signin-intro')}</p>
      {required && <p className="notice">{t('signin-required')}</p>}
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
              empty.password ? `${id}-password-error` : undefined
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
}
