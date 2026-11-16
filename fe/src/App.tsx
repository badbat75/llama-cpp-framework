// The app shell (fe/briefs/01-server.md, section 2.1): skip link, banner,
// nav and main; the session read once on start; the page of the hash route;
// leaving a page with unsaved changes asks first (38), the browser's Back,
// Forward and a typed address too (G1).
import {
  useCallback,
  useEffect,
  useRef,
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
import PresetPage from './PresetPage.tsx'
import { instanceHref, parseRoute, type Route } from './route.ts'
import { readSession, signOut, type Session } from './session.ts'
import SignIn from './SignIn.tsx'
import { t } from './strings.ts'

export default function App() {
  const [hash, setHash] = useState(window.location.hash)
  // undefined while the session is read, null when signed out.
  const [session, setSession] = useState<Session | null | undefined>(undefined)
  // The Models list's filter text, kept for the tab's life by the owner who
  // typed it, so it outlives the page (02-models.md, 62 and 62a).
  const [filter, setFilter] = useState<{ owner: string; text: string } | null>(
    null,
  )
  // Raised on every page change: the new page's h1 takes the focus.
  const [focus, setFocus] = useState(0)
  const [menuOpen, setMenuOpen] = useState(false)
  // The page holds unsaved changes.
  const [dirty, setDirty] = useState(false)
  // The session ended because the manager refused its user (a read answered
  // 403 session-not-administrator): the sign-in form says that, not the plain
  // "sign in to see this page".
  const [revoked, setRevoked] = useState(false)
  // Leaving asked about: where to (none: Sign out) and what to give the
  // focus back to (the link or control that asked, or what had it when the
  // browser moved the address).
  const [leaving, setLeaving] = useState<{
    href?: string
    opener: HTMLElement
  } | null>(null)
  // The hash the app asked for: its hashchange passes the guard of 38b.
  const going = useRef<string | null>(null)
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

  /**
   * A navigation the app asked for: it is not one to ask about (38d). The
   * token goes only when the address really moved: a hash equal to the one
   * shown fires no hashchange, and a token left there would pass a later Back
   * to it through the guard. It holds what the browser reads back, an encoded
   * hash coming back normalised.
   */
  const go = (href: string) => {
    const before = window.location.hash
    window.location.hash = href
    const after = window.location.hash
    if (after !== before) going.current = after
  }

  useEffect(() => {
    const onHash = () => {
      const next = window.location.hash
      if (going.current === next) going.current = null
      else if (dirty && next !== hash) {
        // The browser moved the address (Back, Forward, a hash typed in the
        // address bar) while the form has edits, and a hashchange cannot be
        // held back: the address goes back (replaceState fires none of its
        // own), the page stays with its edits, and 38's dialog asks. The
        // focus the page had is kept for Keep editing (38c); a second change
        // retargets the dialog that is already open (38g).
        history.replaceState(
          null,
          '',
          `${window.location.pathname}${window.location.search}${hash}`,
        )
        setLeaving((l) => ({
          href: next,
          // Nothing focused, the body or a null activeElement (the spec
          // allows it): keep() gives the focus back to the page's h1.
          opener:
            l?.opener ??
            (document.activeElement as HTMLElement | null) ??
            document.body,
        }))
        return
      }
      setHash(next)
      setRevoked(false)
      setFocus((n) => n + 1)
      setMenuOpen(false)
    }
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [dirty, hash])

  const signedIn = (s: Session) => {
    setSession(s)
    setRevoked(false)
    // The kept filter belongs to the one who typed it: another user starts
    // empty (62a).
    if (filter !== null && filter.owner !== s.user) setFilter(null)
    setFocus((n) => n + 1)
    if (route.page === 'sign-in') go('#/')
  }

  /**
   * A sign-in in a page's dialog: the page stays only for the same user;
   * otherwise the page's h1 takes the focus.
   */
  const renewed = (s: Session) => {
    const kept = session?.user === s.user
    setSession(s)
    if (!kept) {
      setFilter(null)
      setFocus((n) => n + 1)
    }
    return kept
  }

  /**
   * Stable: the pages sign the app out when a read is refused. `byManager` tells
   * a read the manager refused from any other sign-out.
   */
  const signedOut = useCallback((byManager: boolean) => {
    setSession(null)
    setRevoked(byManager)
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
    // Sign out empties the kept filter: the next sign-in may be someone else.
    setFilter(null)
    signedOut(false)
    go('#/')
  }

  /** The list's filter field typed: the text belongs to the signed-in user. */
  const typedFilter = (text: string) =>
    setFilter({ owner: session?.user ?? '', text })

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

  /** Keep editing: the dialog goes, the focus back on what opened it. With
   * nothing of the page focused (the browser's address bar), its h1 (38c). */
  const keep = () => {
    const opener = leaving?.opener
    flushSync(() => setLeaving(null))
    const gone =
      opener === undefined || !opener.isConnected || opener === document.body
    if (gone) {
      document.getElementById('page-heading')?.focus()
    } else {
      opener.focus()
    }
  }

  /** Discard changes: on where the click led. */
  const discard = () => {
    const href = leaving?.href
    setLeaving(null)
    if (href === undefined) void leave()
    else go(href)
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
                {/* The preset page belongs to the list without being it, so the
                    list is its current item, not its current page
                    (02-models.md, 2d.64). */}
                <a
                  href="#/models"
                  aria-current={
                    route.page === 'models'
                      ? 'page'
                      : route.page === 'preset'
                        ? 'true'
                        : undefined
                  }
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
            revoked={revoked}
            filterText={filter?.text ?? ''}
            onFilterText={typedFilter}
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

/** The banner's end: Sign in, or the user and Sign out. */
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
  revoked,
  filterText,
  onFilterText,
  onSignedIn,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  route: Route
  store: InstancesStore
  session: Session | null | undefined
  /** The last read was refused by the manager: the sign-in form says so. */
  revoked: boolean
  filterText: string
  onFilterText: (text: string) => void
  onSignedIn: (session: Session) => void
  onSignedOut: (byManager: boolean) => void
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
    return <SignIn required revoked={revoked} onSignedIn={onSignedIn} />
  }
  if (route.page === 'models') {
    return (
      <Models
        filterText={filterText}
        onFilterText={onFilterText}
        onSignedOut={onSignedOut}
        onRenewed={onRenewed}
      />
    )
  }
  if (route.page === 'preset') {
    return <PresetPage id={route.id} onSignedOut={onSignedOut} />
  }
  return (
    <InstancePage
      id={route.id}
      store={store}
      onSignedOut={onSignedOut}
      onRenewed={onRenewed}
      onDirty={onDirty}
    />
  )
}
