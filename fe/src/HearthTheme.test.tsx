// H4, the light/dark theme switch (fe/briefs/07-hearth-h4-theme.md): the
// toggle in the banner, the choice in localStorage under `pai-theme`, the
// `data-theme` attribute on `<html>`, and the system preference. The whole app
// renders against the fake bff, as in App.test.tsx.
//
// The theme module and the button component are not imported here: the brief
// suggests names for them (section 5) and pins no path, so every criterion is
// read through what the page shows. The stylesheet side (H4.10, H4.11) and the
// `main.tsx` text pin of H4.4 are in HearthH4.test.ts.
//
// The harness has no prefers-color-scheme, so the system theme is light. Its matchMedia stub is
// real: addEventListener registers a native listener, a dispatchEvent of 'change' carries
// `matches`, removeEventListener drops it (checked with a throwaway harness test). H4.8 drives
// that with its own listener bookkeeping below.
//
// OPEN: the brief's H4.9 "toggles with Enter and Space" cannot be read in the harness, which
// fires no click from a key on a button. It is pinned as the closest proxy: the switch is a
// native `button` of `type="button"`, which the harness and every browser activate on both keys.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  INSTANCES,
  deferred,
  en,
  fakeBff,
  isHidden,
  startAt,
} from './test/bff.ts'

/** The localStorage key of the brief (section 0). */
const KEY = 'pai-theme'

/** The attributes the brief's H4.1 pins on the icon. */
const ICON: Record<string, string> = {
  'aria-hidden': 'true',
  focusable: 'false',
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': '2',
  'stroke-linecap': 'round',
  'stroke-linejoin': 'round',
}

/** Lucide's `moon` path, as the brief's H4.1 spells it. */
const MOON =
  'M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401'

beforeEach(() => {
  startAt('#/')
  fakeBff()
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  localStorage.clear()
  document.documentElement.removeAttribute('data-theme')
  startAt('')
})

/**
 * The theme button by its accessible name, which is the same in every state
 * (section 0: the name of a toggle does not change with its state).
 */
function themeButton(): HTMLElement {
  return screen.getByRole('button', { name: en('theme-dark') })
}

/**
 * The tabbable elements in DOM order and their names, like ModelProfiles's
 * `tabbable` and `nameOf`, narrowed to the banner for H4.2.
 */
function tabbableIn(root: Element): string[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ]
    .filter(
      (el) =>
        !(el as HTMLButtonElement).disabled &&
        el.getAttribute('tabindex') !== '-1' &&
        !isHidden(el),
    )
    .map((el) => (el.textContent ?? '').replace(/\s+/g, ' ').trim())
}

/** The elements that announce themselves: a press must add none of these. */
function liveRegions(): Element[] {
  return [
    ...document.querySelectorAll(
      '[role="status"], [role="alert"], [aria-live]',
    ),
  ]
}

type Media = {
  /** Every query matchMedia was called with, in order. */
  queries: string[]
  /** The change listeners registered and not yet removed. */
  listeners: EventListener[]
  /** The value a fired change reports. */
  set: (matches: boolean) => void
  /** Fires a change, as the browser does when the OS scheme flips. */
  fire: () => void
}

/**
 * Stands in for `window.matchMedia`, which jsdom does not have (brief section
 * 0): `matches` is settable and the `change` listener is kept, so the test
 * fires it. `null` leaves `window.matchMedia` absent, the case the brief names.
 */
function stubMedia(matches: boolean | null): Media {
  const queries: string[] = []
  const listeners: EventListener[] = []
  if (matches === null) {
    // jsdom's own state: no matchMedia at all, so nothing to record.
    return { queries, listeners, set: () => {}, fire: () => {} }
  }
  const mql = new EventTarget()
  const nativeAdd = EventTarget.prototype.addEventListener
  const nativeRemove = EventTarget.prototype.removeEventListener
  // The scheme the stub reports: `set` moves it, the getter and the fired
  // event read it (the parameter keeps the `null` of the absent case, which
  // TypeScript will not narrow away inside these closures).
  let system = matches
  Object.defineProperty(mql, 'matches', { get: () => system })
  Object.defineProperty(mql, 'media', {
    value: '(prefers-color-scheme: dark)',
  })
  // The listener is registered for real, so the dispatch reaches it, and kept,
  // so the test can assert the app removed it (brief H4.8).
  mql.addEventListener = ((
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: AddEventListenerOptions | boolean,
  ) => {
    if (type === 'change') listeners.push(listener as EventListener)
    nativeAdd.call(mql, type, listener, options)
  }) as typeof mql.addEventListener
  mql.removeEventListener = ((
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: EventListenerOptions | boolean,
  ) => {
    if (type === 'change') {
      const at = listeners.indexOf(listener as EventListener)
      if (at >= 0) listeners.splice(at, 1)
    }
    nativeRemove.call(mql, type, listener, options)
  }) as typeof mql.removeEventListener
  vi.stubGlobal('matchMedia', (query: string) => {
    queries.push(query)
    return mql
  })
  return {
    queries,
    listeners,
    set: (m: boolean) => {
      system = m
    },
    fire() {
      act(() => {
        // The browser fires a MediaQueryListEvent, whose `matches` is the new
        // state; jsdom has no such event, so the plain one carries the field.
        const event = new Event('change') as Event & { matches: boolean }
        event.matches = system
        mql.dispatchEvent(event)
      })
    },
  }
}

describe('H4: the light/dark theme switch', () => {
  test('H4.1: the label is the one string key of the brief', () => {
    expect(en('theme-dark')).toBe('Dark theme')
  })

  test('H4.1: the banner holds a real toggle button: the moon icon then the label, in the three session states', async () => {
    // Signed in, the default session of the fake bff.
    render(<App />)
    await screen.findByRole('button', { name: en('session-sign-out') })
    const banner = screen.getByRole('banner')
    const btn = within(banner).getByRole('button', { name: en('theme-dark') })
    expect(btn.tagName).toBe('BUTTON')
    expect(
      btn.getAttribute('type'),
      'a native button: Enter and Space activate it',
    ).toBe('button')
    expect(
      [...btn.classList].filter((c) =>
        ['secondary', 'theme-switch'].includes(c),
      ),
      'the classes of the brief, so the pressed look of the toolbars applies',
    ).toEqual(['secondary', 'theme-switch'])
    expect(
      btn.getAttribute('aria-pressed'),
      'the state is a pressed button, not a switch',
    ).toBeTruthy()
    expect(
      Array.from(btn.children).map((el) => el.tagName.toLowerCase()),
      'children in this order: the icon, then the label',
    ).toEqual(['svg', 'span'])
    const icon = btn.children[0]!
    for (const [name, value] of Object.entries(ICON))
      expect(icon.getAttribute(name), `the icon's ${name}`).toBe(value)
    expect(icon.querySelectorAll('path'), 'one path').toHaveLength(1)
    expect(icon.querySelector('path')!.getAttribute('d'), 'Lucide moon').toBe(
      MOON,
    )
    const label = btn.children[1]!
    expect(
      label.classList.contains('theme-switch-label'),
      'the label span carries the class H5 hides on narrow screens',
    ).toBe(true)
    expect(label.textContent, 'the label is the string, not a state word').toBe(
      en('theme-dark'),
    )
    expect(
      btn.getAttribute('aria-label'),
      'the name comes from the label text, so it is read as written',
    ).toBeNull()
    expect(
      within(btn).queryAllByRole('status'),
      'the icon is hidden from the screen reader, so the button adds no live text',
    ).toHaveLength(0)
    cleanup()

    // Signed out (09 S1a.1: the page is the sign-in page, there is no
    // session link to wait for).
    fakeBff({ session: 'signed-out' })
    render(<App />)
    await screen.findByRole('heading', { level: 1, name: en('signin-title') })
    expect(
      within(screen.getByRole('banner')).getByRole('button', {
        name: en('theme-dark'),
      }),
      'signed out: the switch is there, the theme is the user own, no sign-in needed',
    ).toBeTruthy()
    cleanup()

    // While the session is read.
    const pending = deferred()
    fakeBff({ session: () => pending.promise })
    render(<App />)
    expect(
      within(screen.getByRole('banner')).getByRole('button', {
        name: en('theme-dark'),
      }),
      'while the session is read: the switch is already there',
    ).toBeTruthy()
  })

  test('H4.2: signed in, in DOM and tab order the switch sits between the Menu button and the session part', async () => {
    render(<App />)
    await screen.findByRole('button', { name: en('session-sign-out') })
    expect(tabbableIn(screen.getByRole('banner'))).toEqual([
      en('app-name'),
      en('nav-menu'),
      en('theme-dark'),
      en('session-sign-out'),
    ])
    expect(
      document.querySelectorAll(
        'a[href], button, input, select, textarea, [tabindex]',
      )[0],
      'the skip link stays the first focusable element (1a.1)',
    ).toBe(screen.getByRole('link', { name: en('skip-to-content') }))
  })

  // 09 S1a.4: signed out there is no nav, so the Menu button and the session
  // part are gone: the theme switch is the only tool left in the banner.
  test('09 S1a.4 (H4.2): signed out, the switch is the only tool in the banner, after the home link', async () => {
    fakeBff({ session: 'signed-out' })
    render(<App />)
    await screen.findByRole('heading', { level: 1, name: en('signin-title') })
    expect(tabbableIn(screen.getByRole('banner'))).toEqual([
      en('app-name'),
      en('theme-dark'),
    ])
  })

  const INITIAL: {
    label: string
    stored: string | null
    system: boolean | null
    pressed: string
    theme: string | null
  }[] = [
    {
      label: 'nothing stored and the system prefers dark',
      stored: null,
      system: true,
      pressed: 'true',
      theme: null,
    },
    {
      label: 'nothing stored and the system prefers light',
      stored: null,
      system: false,
      pressed: 'false',
      theme: null,
    },
    {
      label: 'nothing stored and matchMedia is absent',
      stored: null,
      system: null,
      pressed: 'false',
      theme: null,
    },
    {
      label: 'dark stored on a light system',
      stored: 'dark',
      system: false,
      pressed: 'true',
      theme: 'dark',
    },
    {
      label: 'light stored on a dark system',
      stored: 'light',
      system: true,
      pressed: 'false',
      theme: 'light',
    },
    {
      label: 'a stored value that is neither light nor dark',
      stored: 'sepia',
      system: true,
      pressed: 'true',
      theme: null,
    },
  ]

  test.each(INITIAL)(
    'H4.3: $label: aria-pressed is "$pressed", data-theme $theme',
    ({ label, stored, system, pressed, theme }) => {
      if (stored !== null) localStorage.setItem(KEY, stored)
      const media = stubMedia(system)
      render(<App />)
      expect(
        themeButton().getAttribute('aria-pressed'),
        `${label}: the pressed state is the choice, and the system with no choice`,
      ).toBe(pressed)
      expect(
        document.documentElement.getAttribute('data-theme'),
        theme === null
          ? `${label}: no choice in effect, so the attribute is absent and the stylesheet follows the OS`
          : `${label}: the attribute mirrors the stored choice`,
      ).toBe(theme)
      if (system !== null)
        expect(
          media.queries,
          `${label}: the system preference is read from the media query of the brief`,
        ).toContain('(prefers-color-scheme: dark)')
    },
  )

  test('H4.4: the stored choice is on <html> in the same tick the app renders, not after a paint', () => {
    localStorage.setItem(KEY, 'dark')
    stubMedia(false)
    render(<App />)
    // No await, no act flush: a choice that an effect applies after the first
    // paint, or that only main.tsx applies, is not there yet at this point.
    expect(
      document.documentElement.getAttribute('data-theme'),
      'the stored dark is on <html> as the app renders',
    ).toBe('dark')
    expect(
      themeButton().getAttribute('aria-pressed'),
      'and the button starts pressed',
    ).toBe('true')
  })

  test('H4.5: a press away from the system sets the scheme, the pressed state and the stored choice', () => {
    for (const [system, want] of [
      [false, 'dark'],
      [true, 'light'],
    ] as [boolean, string][]) {
      localStorage.clear()
      stubMedia(system)
      render(<App />)
      const btn = themeButton()
      expect(
        btn.getAttribute('aria-pressed'),
        `system ${system}: the button starts following it`,
      ).toBe(system ? 'true' : 'false')
      fireEvent.click(btn)
      expect(
        btn.getAttribute('aria-pressed'),
        `system ${system}: the press moves away from it`,
      ).toBe(system ? 'false' : 'true')
      expect(
        document.documentElement.getAttribute('data-theme'),
        `system ${system}: the scheme of the page`,
      ).toBe(want)
      expect(
        localStorage.getItem(KEY),
        `system ${system}: the choice is remembered, under the key of the brief`,
      ).toBe(want)
      expect(
        Object.keys(localStorage),
        'the theme writes only its own key',
      ).toEqual([KEY])
      cleanup()
    }
  })

  test('H4.6: a press back to the system drops the choice: attribute and entry go, and two clicks store nothing', () => {
    // System light, stored dark: one press returns to the OS.
    localStorage.setItem(KEY, 'dark')
    stubMedia(false)
    render(<App />)
    const btn = themeButton()
    expect(
      btn.getAttribute('aria-pressed'),
      'stored dark on a light system',
    ).toBe('true')
    fireEvent.click(btn)
    expect(
      btn.getAttribute('aria-pressed'),
      'the system theme again: not pressed',
    ).toBe('false')
    expect(
      document.documentElement.hasAttribute('data-theme'),
      'the attribute is removed: the stylesheet follows the OS again',
    ).toBe(false)
    expect(localStorage.getItem(KEY), 'the entry is removed').toBeNull()
    cleanup()
    localStorage.clear()

    // Mirror: system dark, stored light.
    localStorage.setItem(KEY, 'light')
    stubMedia(true)
    render(<App />)
    const other = themeButton()
    expect(
      other.getAttribute('aria-pressed'),
      'stored light on a dark system',
    ).toBe('false')
    fireEvent.click(other)
    expect(
      other.getAttribute('aria-pressed'),
      'the system theme again: pressed',
    ).toBe('true')
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false)
    expect(localStorage.getItem(KEY), 'the entry is removed').toBeNull()
    cleanup()
    localStorage.clear()

    // Two clicks from nothing stored: away and back, and nothing is stored.
    stubMedia(false)
    render(<App />)
    const twice = themeButton()
    fireEvent.click(twice)
    fireEvent.click(twice)
    expect(
      localStorage.getItem(KEY),
      'the way back to following the system is a press, and it leaves no entry',
    ).toBeNull()
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false)
    expect(twice.getAttribute('aria-pressed')).toBe('false')
  })

  test('H4.7: a press leaves the focus on the button, keeps the name, and announces, routes and retitles nothing', async () => {
    render(<App />)
    await screen.findByRole('heading', { level: 1, name: en('overview-title') })
    // Settle the Overview's read before the precondition: 17a.2 puts the
    // page's loading status in the section from the first frame, so a pending
    // read is a live region by design now. The intent here is the settled
    // page: a press adds none to it.
    const tiles = await screen.findAllByRole('link', {
      name: INSTANCES[0]!.id,
    })
    expect(tiles.some((a) => a.closest('.tile'))).toBe(true)
    const btn = themeButton()
    // 23 A1.2: the settled tiles each carry the run row's status line, by
    // design. The page announces nothing: every live region is empty.
    const regions = liveRegions()
    expect(
      regions.map((el) => el.textContent?.trim()),
      'the live regions a press starts from say nothing',
    ).toEqual(regions.map(() => ''))
    const title = document.title
    btn.focus()
    fireEvent.click(btn)
    expect(
      document.activeElement,
      'the focus stays on the button: no focus move, so nothing is announced',
    ).toBe(btn)
    expect(
      themeButton().textContent?.replace(/\s+/g, ' ').trim(),
      'the name is the same word in both states',
    ).toBe(en('theme-dark'))
    expect(
      liveRegions(),
      'no role=status, role=alert or aria-live appears beside the tiles’ run-status lines: aria-pressed is the announcement',
    ).toEqual(regions)
    expect(window.location.hash, 'a press is not a navigation').toBe('#/')
    expect(document.title, 'a press does not retitle the page').toBe(title)
  })

  test('H4.8: with nothing stored the button follows the system while the page is open; a stored choice changes nothing', () => {
    const media = stubMedia(false)
    render(<App />)
    const btn = themeButton()
    expect(btn.getAttribute('aria-pressed')).toBe('false')
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false)
    media.set(true)
    media.fire()
    expect(
      btn.getAttribute('aria-pressed'),
      'the OS turned dark with nothing stored: the pressed state follows',
    ).toBe('true')
    expect(
      document.documentElement.hasAttribute('data-theme'),
      'and the attribute stays absent: the stylesheet follows the OS itself',
    ).toBe(false)
    // A press now stores a choice: from there the OS moving changes nothing.
    fireEvent.click(btn)
    expect(btn.getAttribute('aria-pressed'), 'light on a dark system').toBe(
      'false',
    )
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
    expect(localStorage.getItem(KEY)).toBe('light')
    for (const system of [false, true]) {
      media.set(system)
      media.fire()
      expect(
        btn.getAttribute('aria-pressed'),
        `a stored choice: the OS moving to ${system} changes nothing`,
      ).toBe('false')
      expect(
        document.documentElement.getAttribute('data-theme'),
        `a stored choice: the OS moving to ${system} changes nothing`,
      ).toBe('light')
    }
  })

  test('H4.8: the app leaves no system-preference listener behind when it goes away', () => {
    const media = stubMedia(false)
    const view = render(<App />)
    expect(
      media.listeners.length,
      'one change listener while the app is mounted',
    ).toBe(1)
    view.unmount()
    expect(
      media.listeners,
      'the listener is removed when the app unmounts',
    ).toEqual([])
  })

  test('H4.9: a storage that refuses every read and write leaves the page working, from the system, and a press still flips the scheme', async () => {
    const boom = () => {
      throw new Error('storage is blocked')
    }
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(boom)
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(boom)
    vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(boom)
    stubMedia(true)
    render(<App />)
    await screen.findByRole('heading', { level: 1, name: en('overview-title') })
    const btn = themeButton()
    expect(
      btn.getAttribute('aria-pressed'),
      'a refused read counts as nothing stored, so the button starts from the system',
    ).toBe('true')
    expect(
      document.documentElement.hasAttribute('data-theme'),
      'and the attribute is absent',
    ).toBe(false)
    fireEvent.click(btn)
    expect(
      btn.getAttribute('aria-pressed'),
      'the press flips the state although the write is refused',
    ).toBe('false')
    expect(
      document.documentElement.getAttribute('data-theme'),
      'and the attribute flips too, for the life of the page',
    ).toBe('light')
  })
})
