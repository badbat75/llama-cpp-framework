// The app shell (fe/briefs/01-server.md, section 2.1): skip link, banner,
// nav and main; the session read once on start; the page of the hash route.
import { useCallback, useEffect, useState, type KeyboardEvent } from 'react'
import InstancePage from './InstancePage.tsx'
import { readInstances, type Instance } from './Instances.tsx'
import NotFound from './NotFound.tsx'
import Overview from './Overview.tsx'
import { FocusRequest } from './PageHeading.tsx'
import { instanceHref, parseRoute, type Route } from './route.ts'
import { readSession, signOut, type Session } from './session.ts'
import SignIn from './SignIn.tsx'
import { t } from './strings.ts'

export default function App() {
  const [hash, setHash] = useState(window.location.hash)
  // undefined while the session is read, null when signed out.
  const [session, setSession] = useState<Session | null | undefined>(undefined)
  // Raised on every page change: the new page's h1 takes the focus.
  const [focus, setFocus] = useState(0)
  const [menuOpen, setMenuOpen] = useState(false)
  const route = parseRoute(hash)

  useEffect(() => {
    let live = true
    readSession().then((s) => {
      if (live) setSession(s)
    })
    return () => {
      live = false
    }
  }, [])

  useEffect(() => {
    const onHash = () => {
      setHash(window.location.hash)
      setFocus((n) => n + 1)
      setMenuOpen(false)
    }
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])

  const signedIn = (s: Session) => {
    setSession(s)
    setFocus((n) => n + 1)
    if (route.page === 'sign-in') window.location.hash = '#/'
  }

  // Stable: the server section reads again when it changes.
  const signedOut = useCallback(() => {
    setSession(null)
    setFocus((n) => n + 1)
  }, [])

  // Escape on the Menu button or in the open nav closes it, focus on the button.
  const closeOnEscape = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && menuOpen) {
      setMenuOpen(false)
      document.getElementById('menu-button')?.focus()
    }
  }

  const leave = async () => {
    await signOut()
    signedOut()
    window.location.hash = '#/'
  }

  return (
    <FocusRequest.Provider value={focus}>
      <a
        className="skip-link"
        href="#main"
        onClick={(e) => {
          e.preventDefault()
          document.getElementById('main')?.focus()
        }}
      >
        {t('skip-to-content')}
      </a>
      <header className="banner">
        <a className="app-name" href="#/">
          {t('app-name')}
        </a>
        <button
          id="menu-button"
          type="button"
          className="menu-button secondary"
          aria-expanded={menuOpen}
          aria-controls="nav"
          onClick={() => setMenuOpen((o) => !o)}
          onKeyDown={closeOnEscape}
        >
          {t('nav-menu')}
        </button>
        <SessionBar session={session} onSignOut={leave} />
      </header>
      <div className="layout">
        <nav
          id="nav"
          className={menuOpen ? 'nav open' : 'nav'}
          aria-label={t('nav-label')}
          onClick={(e) => {
            if ((e.target as Element).closest('a')) setMenuOpen(false)
          }}
          onKeyDown={closeOnEscape}
        >
          <ul>
            <li>
              <a
                href="#/"
                aria-current={route.page === 'overview' ? 'page' : undefined}
              >
                {t('nav-overview')}
              </a>
            </li>
          </ul>
          {session && <NavInstances key={session.user} route={route} />}
        </nav>
        <main id="main" tabIndex={-1}>
          <Page
            key={hash}
            route={route}
            session={session}
            onSignedIn={signedIn}
            onSignedOut={signedOut}
          />
        </main>
      </div>
    </FocusRequest.Provider>
  )
}

/** The banner's end: Sign in, or the user, the role and Sign out. */
function SessionBar({
  session,
  onSignOut,
}: {
  session: Session | null | undefined
  onSignOut: () => void
}) {
  if (session === undefined) {
    return null
  }
  if (session === null) {
    return (
      <a className="session" href="#/sign-in">
        {t('session-sign-in')}
      </a>
    )
  }
  return (
    <div className="session">
      <span>{t('session-signed-in-as', session.user)}</span>
      <span className="role">
        {session.canConfigure
          ? t('session-role-admin')
          : t('session-role-user')}
      </span>
      <button type="button" className="secondary" onClick={onSignOut}>
        {t('session-sign-out')}
      </button>
    </div>
  )
}

/** One nav link per instance, under the Instances label; none when the read fails. */
function NavInstances({ route }: { route: Route }) {
  const [instances, setInstances] = useState<Instance[]>([])

  useEffect(() => {
    let live = true
    readInstances().then((a) => {
      if (live && Array.isArray(a)) setInstances(a)
    })
    return () => {
      live = false
    }
  }, [])

  if (instances.length === 0) return null
  return (
    <>
      <p id="nav-instances" className="nav-label">
        {t('nav-instances')}
      </p>
      <ul aria-labelledby="nav-instances">
        {instances.map((i) => (
          <li key={i.id}>
            <a
              href={instanceHref(i.id)}
              aria-current={
                route.page === 'instance' && route.id === i.id
                  ? 'page'
                  : undefined
              }
            >
              {i.id}
            </a>
          </li>
        ))}
      </ul>
    </>
  )
}

/** The page of the route, the sign-in form in place of a protected one. */
function Page({
  route,
  session,
  onSignedIn,
  onSignedOut,
}: {
  route: Route
  session: Session | null | undefined
  onSignedIn: (session: Session) => void
  onSignedOut: () => void
}) {
  if (session === undefined) {
    return <p role="status">{t('loading')}</p>
  }
  if (route.page === 'overview') {
    return <Overview />
  }
  if (route.page === 'sign-in') {
    return <SignIn required={false} onSignedIn={onSignedIn} />
  }
  if (route.page === 'not-found') {
    return <NotFound />
  }
  if (session === null) {
    return <SignIn required onSignedIn={onSignedIn} />
  }
  return (
    <InstancePage
      id={route.id}
      canConfigure={session.canConfigure}
      onSignedOut={onSignedOut}
    />
  )
}
