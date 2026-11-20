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
import About from './About.tsx'
import { DiscardDialog } from './Dialog.tsx'
import { Icon } from './Icon.tsx'
import InstancePage from './InstancePage.tsx'
import {
  instancesStore,
  type Instance,
  type InstancesStore,
} from './Instances.tsx'
import { Mark } from './Mark.tsx'
import ModelProfiles from './ModelProfiles.tsx'
import NotFound from './NotFound.tsx'
import Overview from './Overview.tsx'
import { FocusRequest } from './PageHeading.tsx'
import ModelProfilePage from './ModelProfilePage.tsx'
import { readNode } from './node.ts'
import { instanceHref, legacyHref, parseRoute, type Route } from './route.ts'
import {
  readSession,
  signOut,
  type Session,
  type SignedOut,
} from './session.ts'
import SignIn from './SignIn.tsx'
import { ThemeSwitch } from './ThemeSwitch.tsx'
import { t } from './strings.ts'

/**
 * The old `#/models` bookmark of 05b (fe/briefs/05b-profiles-nav.md, 5b.8 and
 * 5b.9): the address in the bar becomes what it stands for, `legacyHref`, by
 * replaceState, which adds no history entry and fires no hashchange of its
 * own, and keeps the path and the query. The hash to route on: the rewritten
 * one, and `hash` for every address that is not an old one.
 */
function rewriteLegacy(hash: string): string {
  const target = legacyHref(hash)
  if (target === null) return hash
  replaceTo(target)
  return target
}

/**
 * The address in the bar becomes `href`, with no history entry and no
 * hashchange of its own: the same rewrite as an old bookmark (5b.8), used for
 * the `#/sign-in` alias of 09 S1a.9 and S1a.10. The path and the query are
 * kept.
 */
function replaceTo(href: string) {
  history.replaceState(
    null,
    '',
    `${window.location.pathname}${window.location.search}${href}`,
  )
}

/**
 * What the sign-in page says about a session that ended: `ended` when a read
 * answered 401, `revoked` when the manager refused its user, `none` when the
 * visitor signed out themselves or never signed in (09 S1b.3, S1b.5).
 */
type Notice = 'none' | 'ended' | 'revoked'

export default function App() {
  // The route reads the rewritten address, so an old bookmark opens its page
  // with no flash of the not-found page; the effect below puts the same
  // address in the bar.
  const [hash, setHash] = useState(
    () => legacyHref(window.location.hash) ?? window.location.hash,
  )
  // undefined while the session is read, null when signed out.
  const [session, setSession] = useState<Session | null | undefined>(undefined)
  // The Model profiles list's filter text, kept for the tab's life by the
  // owner who typed it, so it outlives the page (02-models.md, 62 and 62a).
  const [filter, setFilter] = useState<{ owner: string; text: string } | null>(
    null,
  )
  // Raised on every page change: the new page's h1 takes the focus.
  const [focus, setFocus] = useState(0)
  const [menuOpen, setMenuOpen] = useState(false)
  // The page holds unsaved changes, from whichever section of it has them:
  // the Server form, the model profile, the instance's tags (12 T1.13).
  const [dirty, setDirty] = useState(false)
  // The session ended and the sign-in page says why: see `Notice`. The ref is
  // the word as it stands right now, so several reads answering 401 at once
  // say it once and move the focus once (S1b.1).
  const [notice, setNotice] = useState<Notice>('none')
  const told = useRef<Notice>('none')
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
  // The host's name, from the one read of /api/node a session brings
  // (fe/briefs/13-node-about.md, 13 N.3): the nav's instance group and the
  // Overview's heading. null while the read is open and after any failure,
  // when both keep the words of today. That read never signs anybody out: a
  // sign-out from the shell could drop the page's unsaved edits.
  // Whose session the read was made for: a sign-in in a page's dialog as the
  // same user keeps it, another user's is a new one (13 N.3).
  const hostSeen = useRef<string | null>(null)
  // The user of the session on screen right now: the read compares this when it
  // lands, so a re-run of the effect for the same user (StrictMode, a sign-in in
  // a page's dialog) drops nothing of the read already on its way, and a session
  // that moved to another user does drop the older read (13 N.3).
  const hostNow = useRef<string | null>(null)
  // The address the app is at. For a signed-in visitor `#/sign-in` IS `#/`
  // (S1a.10), so the route, the page and the nav all read the alias as the
  // Overview, and the bar follows in the effect below. The page's identity is
  // this address, so the Overview the visitor is reading stays mounted while
  // the bar moves.
  const at = session && hash === '#/sign-in' ? '#/' : hash
  const route = parseRoute(at)

  // The address in the bar when the app opens: an old bookmark is rewritten
  // before any page reads (5b.8). The route already reads the rewritten one.
  useEffect(() => {
    rewriteLegacy(window.location.hash)
  }, [])

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
   * The host of the session that just started, read once for it (13 N.3): no
   * read while the session is being read and none signed out, none for a page
   * change, and none again for the user a page's dialog signed in as. The name
   * carries the user it was read for, so a session that is not that one reads
   * the fallback words; Sign out takes the name with the session, so the next
   * one, of the same user or not, reads the words of today until it answers.
   */
  const [hostFor, setHostFor] = useState<{ user: string; name: string } | null>(
    null,
  )
  useEffect(() => {
    const user = session?.user ?? null
    hostNow.current = user
    if (user === null) {
      // No session to read for: the guard goes, so the next sign-in reads
      // again, even as the same user (13 N.3: a sign-out and a sign-in is a
      // new session). The name itself goes with `signedOut`.
      hostSeen.current = null
      return
    }
    if (hostSeen.current === user) return
    hostSeen.current = user
    readNode().then(({ node }) => {
      if (hostNow.current !== user) return
      setHostFor(node === null ? null : { user, name: node.name })
    })
  }, [session])
  const host = hostFor && session?.user === hostFor.user ? hostFor.name : null

  /**
   * Stable: the sign-in page stops saying anything about a session. A new
   * address and a sign-in both start a page with nothing to explain (S1b.3:
   * the notice goes at the next address change and at a successful sign-in).
   */
  const forgetNotice = useCallback(() => {
    told.current = 'none'
    setNotice('none')
  }, [])

  /**
   * `#/sign-in` is an alias of `#/` for a signed-in visitor (09 S1a.9, S1a.10):
   * the bar becomes `#/` by replaceState, which adds no history entry and
   * fires no hashchange of its own, so Back leaves the place the visitor came
   * from and never meets a sign-in page. This is the direct load: the route
   * already reads the alias as the Overview (the `at` above), and this moves
   * the bar and gives the focus to the Overview reached (S1a.10). A typed
   * address, Back and Forward pass in the hashchange handler below, a sign-in
   * answered at the alias in `signedIn`.
   */
  useEffect(() => {
    if (!session || hash !== '#/sign-in') return
    replaceTo('#/')
    document.getElementById('page-heading')?.focus()
  }, [hash, session])

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
      // An old bookmark arrived (typed, or Back to one): the bar is rewritten
      // first, and everything below reads the rewritten address, so the leave
      // guard asks about the page the address stands for (5b.9).
      const asked = rewriteLegacy(window.location.hash)
      // Signed in, `#/sign-in` is an alias of `#/` (09 S1a.10): the guard
      // asks about the Overview, the bar is rewritten to it, and it is the
      // page that shows.
      const next = session && asked === '#/sign-in' ? '#/' : asked
      const aliased = next !== asked
      if (next === hash) {
        // The address already shown is no navigation (05b's 5b.13): the
        // rewrite of the old bookmark is all that happens, clean or dirty
        // alike. No focus move, no title, no read, the menu left as it is,
        // the guard not asked, a "session ended" sign-in text kept: the user
        // did not leave, and a focus move would take someone typing away
        // from their place (2.4.3, 3.2.1, 3.2.2). A token for this address is
        // spent here, so no later navigation to it passes the guard. The
        // alias is the one arrival that moves the focus: it asks for the page
        // and the bar has to say so (S1a.10).
        if (going.current === next) going.current = null
        if (aliased) {
          replaceTo(next)
          setFocus((n) => n + 1)
        }
        return
      }
      if (going.current === next) going.current = null
      else if (dirty) {
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
      // The alias asks for the Overview and the bar has to say so (S1a.10).
      if (aliased) replaceTo(next)
      forgetNotice()
      setFocus((n) => n + 1)
      setMenuOpen(false)
    }
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [dirty, forgetNotice, hash, session])

  const signedIn = (s: Session) => {
    setSession(s)
    forgetNotice()
    // The kept filter belongs to the one who typed it: another user starts
    // empty (62a).
    if (filter !== null && filter.owner !== s.user) setFilter(null)
    setFocus((n) => n + 1)
    // Signed in at `#/sign-in`: the address and the route move in this one
    // update (S1a.9), so the Overview mounts once at `#/` and its h1 takes the
    // focus; the alias effect sees the moved hash and does nothing.
    if (hash === '#/sign-in') {
      replaceTo('#/')
      setHash('#/')
    }
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
   * Stable: the pages sign the app out when a read is refused, and Sign out
   * ends the visitor's own session. `reason` says what the sign-in page tells:
   * a read that answered 401, a read the manager refused, or nothing when the
   * visitor signed out themselves (09 S1b.3). The word of a kind is told once,
   * so the reads of one page that all answer 401 move the focus once, and the
   * revoked word wins over the ended one (S1b.5).
   */
  const signedOut = useCallback((reason: SignedOut) => {
    if (reason === 'sign-out') {
      told.current = 'none'
      setNotice('none')
    } else if (
      reason === 'revoked'
        ? told.current !== 'revoked'
        : told.current === 'none'
    ) {
      told.current = reason
      setNotice(reason)
    } else {
      // Nothing new to say: the sign-in page is already there, with the focus
      // on its h1.
      return
    }
    setSession(null)
    // The host name belongs to the session that ended: the sign-in page has no
    // nav, and the next session reads again (13 N.3).
    setHostFor(null)
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
    signedOut('sign-out')
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
          <Mark />
          {t('app-name')}
        </a>
        <div className="banner-tools">
          {session && (
            <button
              id="menu-button"
              type="button"
              className="menu-button secondary"
              aria-expanded={menuOpen}
              aria-controls="nav"
              onClick={() => setMenuOpen((o) => !o)}
              onKeyDown={closeOnEscape}
            >
              <Icon name="menu" />
              {t('nav-menu')}
            </button>
          )}
          <ThemeSwitch />
          <SessionBar session={session} onSignOut={signOutClicked} />
        </div>
      </header>
      <div className="layout">
        {session && (
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
                  <Icon name="house" />
                  {t('nav-overview')}
                </a>
              </li>
            </ul>
            <NavInstances
              key={session.user}
              route={route}
              store={store}
              host={host}
            />
            <p id="nav-profiles" className="nav-label">
              {t('nav-profiles')}
            </p>
            <ul aria-labelledby="nav-profiles">
              <li>
                {/* The model profile page belongs to the list without being it,
                    so the list is its current item, not its current page
                    (02-models.md, 2d.64). */}
                <a
                  href="#/profiles/model"
                  aria-current={
                    route.page === 'model-profiles'
                      ? 'page'
                      : route.page === 'model-profile'
                        ? 'true'
                        : undefined
                  }
                >
                  <Icon name="layers" />
                  {t('nav-model-profiles')}
                </a>
              </li>
            </ul>
            {/* About is the nav's last entry, in a list of its own with no
                label of its own: one place, not a group of many (13 N.6). */}
            <ul>
              <li>
                <a
                  href="#/about"
                  aria-current={route.page === 'about' ? 'page' : undefined}
                >
                  <Icon name="info" />
                  {t('nav-about')}
                </a>
              </li>
            </ul>
          </nav>
        )}
        <main id="main" tabIndex={-1}>
          {session === null ? (
            // Signed out, the sign-in page is the whole of main at every hash
            // (09 S1a.1), and it is ONE page: the hash moving keeps the form
            // and its h1 in place, so the focus an in-app arrival asks for
            // stays on the heading the visitor is reading. The notice says why
            // only where the visitor asked for a page of their own (S1a.7).
            <SignIn
              required={route.page !== 'overview' && route.page !== 'sign-in'}
              revoked={notice === 'revoked'}
              ended={notice === 'ended'}
              onSignedIn={signedIn}
            />
          ) : (
            <Page
              key={at}
              route={route}
              store={store}
              session={session}
              host={host}
              filterText={filter?.text ?? ''}
              onFilterText={typedFilter}
              onSignedOut={signedOut}
              onRenewed={renewed}
              onDirty={setDirty}
            />
          )}
        </main>
      </div>
      {leaving !== null && <DiscardDialog onKeep={keep} onDiscard={discard} />}
    </FocusRequest.Provider>
  )
}

/** The banner's end: the user and Sign out. Signed out or pending it is
 * nothing: the page is the sign-in form, so a link to it would be one more
 * Tab stop pointing at where the focus already is (09 S1a.4). */
function SessionBar({
  session,
  onSignOut,
}: {
  session: Session | null | undefined
  onSignOut: (e: MouseEvent<HTMLButtonElement>) => void
}) {
  if (!session) {
    return null
  }
  return (
    <div className="session">
      <span>{t('session-signed-in-as', session.user)}</span>
      <button type="button" className="secondary" onClick={onSignOut}>
        <Icon name="log-out" />
        {t('session-sign-out')}
      </button>
    </div>
  )
}

/**
 * One nav link per instance, under the label of the host that holds them: the
 * latest list a read gave since the session started; none until one succeeds.
 */
function NavInstances({
  route,
  store,
  host,
}: {
  route: Route
  store: InstancesStore
  /** The host's name, from the shell's one read (fe/briefs/13-node-about.md,
   * 13 N.4): it names the group. null before the answer and after a failure,
   * when the words of today stand. */
  host: string | null
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
        {host ?? t('nav-instances')}
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
  host,
  filterText,
  onFilterText,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  route: Route
  store: InstancesStore
  session: Session | undefined
  /** The host's name the shell read once for this session (13 N.3, N.5). */
  host: string | null
  filterText: string
  onFilterText: (text: string) => void
  onSignedOut: (reason: SignedOut) => void
  onRenewed: (session: Session) => boolean
  onDirty: (dirty: boolean) => void
}) {
  if (session === undefined) {
    return <p role="status">{t('loading')}</p>
  }
  if (route.page === 'overview' || route.page === 'sign-in') {
    // Signed in, #/sign-in is an alias of #/ (09 S1a.10); the alias effect
    // follows with the address.
    return <Overview store={store} host={host} onSignedOut={onSignedOut} />
  }
  if (route.page === 'not-found') {
    return <NotFound />
  }
  if (route.page === 'model-profiles') {
    return (
      <ModelProfiles
        filterText={filterText}
        onFilterText={onFilterText}
        onSignedOut={onSignedOut}
        onRenewed={onRenewed}
      />
    )
  }
  if (route.page === 'model-profile') {
    return (
      <ModelProfilePage
        id={route.id}
        onSignedOut={onSignedOut}
        onRenewed={onRenewed}
        onDirty={onDirty}
      />
    )
  }
  if (route.page === 'about') {
    // The versions of this host, read again at every arrival (13 N.7).
    return <About onSignedOut={onSignedOut} />
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
