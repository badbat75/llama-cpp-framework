// The app shell (fe/briefs/01-server.md, section 2.1): skip link, banner,
// nav and main; the session read once on start; the page of the hash route;
// leaving a page with unsaved changes asks first (38).
import {
  useCallback,
  useEffect,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from 'react'
import { flushSync } from 'react-dom'
import { DiscardDialog } from './Dialog.tsx'
import InstancePage from './InstancePage.tsx'
import {
  instancesStore,
  type Instance,
  type InstancesStore,
} from './Instances.tsx'
import Models from './Models.tsx'
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
  // The page holds unsaved changes.
  const [dirty, setDirty] = useState(false)
  // Leaving asked about: where to (none: Sign out) and the control that asked.
  const [leaving, setLeaving] = useState<{
    href?: string
    opener: HTMLElement
  } | null>(null)
  // The one read of /api/instances the nav and the pages share.
  const [store] = useState(instancesStore)
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

  /**
   * A sign-in in a page's dialog: the page stays only for the same user with
   * the same role; otherwise the page's h1 takes the focus.
   */
  const renewed = (s: Session) => {
    const kept =
      session?.user === s.user && session.canConfigure === s.canConfigure
    setSession(s)
    if (!kept) setFocus((n) => n + 1)
    return kept
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

  /** A click on `link` while dirty asks first; true when it was held back. */
  const held = (e: MouseEvent, link: HTMLAnchorElement) => {
    const href = link.getAttribute('href') ?? ''
    if (!dirty || href === window.location.hash) return false
    e.preventDefault()
    setLeaving({ href, opener: link })
    return true
  }

  const signOutClicked = (e: MouseEvent<HTMLButtonElement>) => {
    if (dirty) setLeaving({ opener: e.currentTarget })
    else void leave()
  }

  /** Keep editing: the dialog goes, the focus back on what opened it. */
  const keep = () => {
    const opener = leaving?.opener
    flushSync(() => setLeaving(null))
    opener?.focus()
  }

  /** Discard changes: on where the click led. */
  const discard = () => {
    const href = leaving?.href
    setLeaving(null)
    if (href === undefined) void leave()
    else window.location.hash = href
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
        <a
          className="app-name"
          href="#/"
          onClick={(e) => held(e, e.currentTarget)}
        >
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
        <SessionBar
          session={session}
          current={route.page === 'sign-in'}
          onSignOut={signOutClicked}
        />
      </header>
      <div className="layout">
        <nav
          id="nav"
          className={menuOpen ? 'nav open' : 'nav'}
          aria-label={t('nav-label')}
          onClick={(e) => {
            const link = (e.target as Element).closest('a')
            if (link && !held(e, link)) setMenuOpen(false)
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
          {session && (
            <NavInstances key={session.user} route={route} store={store} />
          )}
          {session && (
            <ul>
              <li>
                <a
                  href="#/models"
                  aria-current={route.page === 'models' ? 'page' : undefined}
                >
                  {t('nav-models')}
                </a>
              </li>
            </ul>
          )}
        </nav>
        <main id="main" tabIndex={-1}>
          <Page
            key={hash}
            route={route}
            store={store}
            session={session}
            onSignedIn={signedIn}
            onSignedOut={signedOut}
            onRenewed={renewed}
            onDirty={setDirty}
          />
        </main>
      </div>
      {leaving !== null && <DiscardDialog onKeep={keep} onDiscard={discard} />}
    </FocusRequest.Provider>
  )
}

/** The banner's end: Sign in, or the user, the role and Sign out. */
function SessionBar({
  session,
  current,
  onSignOut,
}: {
  session: Session | null | undefined
  /** On #/sign-in: the Sign in link is the current page. */
  current: boolean
  onSignOut: (e: MouseEvent<HTMLButtonElement>) => void
}) {
  if (session === undefined) {
    return null
  }
  if (session === null) {
    return (
      <a
        className="session"
        href="#/sign-in"
        aria-current={current ? 'page' : undefined}
      >
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

/**
 * One nav link per instance, under the Instances label: the latest list a
 * read gave since the session started; none until one succeeds.
 */
function NavInstances({
  route,
  store,
}: {
  route: Route
  store: InstancesStore
}) {
  const [instances, setInstances] = useState<Instance[]>([])

  useEffect(() => {
    const unwatch = store.watch((a) => {
      if (Array.isArray(a)) setInstances(a)
    })
    void store.read()
    return unwatch
  }, [store])

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
  store,
  session,
  onSignedIn,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  route: Route
  store: InstancesStore
  session: Session | null | undefined
  onSignedIn: (session: Session) => void
  onSignedOut: () => void
  onRenewed: (session: Session) => boolean
  onDirty: (dirty: boolean) => void
}) {
  if (session === undefined) {
    return <p role="status">{t('loading')}</p>
  }
  if (route.page === 'overview') {
    return <Overview store={store} />
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
  if (route.page === 'models') {
    return <Models onSignedOut={onSignedOut} />
  }
  return (
    <InstancePage
      id={route.id}
      store={store}
      canConfigure={session.canConfigure}
      onSignedOut={onSignedOut}
      onRenewed={onRenewed}
      onDirty={onDirty}
    />
  )
}
