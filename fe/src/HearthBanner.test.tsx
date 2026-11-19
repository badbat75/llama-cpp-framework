// H5a, the banner of the Hearth shell (fe/briefs/07-hearth-h5-shell.md, section 1,
// criterion H5a.1): the tools group. The banner keeps the names, roles, ids and tab
// order of H4.2 and the rules 1a.1 and 1a.3 (App.test.tsx and
// HearthTheme.test.tsx pin those, unchanged; 09 S1a.4 dropped the session part
// and the Menu button from the signed-out and pending banner), so this file pins only the wrapper:
// the header's element children, the class and the absence of a role of
// `div.banner-tools`, its element children in order, and the names by role inside
// it, in the three session states. The stylesheet side (H5a.2 to H5a.7) is in
// HearthH5a.test.ts: jsdom lays nothing out, so the transparent banner, the
// icon-only button below 480 px and the 320 px padding are read there as
// declarations, and how they look is the designer's UX pass.
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  deferred,
  en,
  fakeBff,
  isHidden,
  startAt,
  type BffOptions,
} from './test/bff.ts'

/** `[tag, id, classes]` of an element: what "its element children, in order" is. */
function shape(el: Element): [string, string, string] {
  return [el.tagName.toLowerCase(), el.id, [...el.classList].sort().join(' ')]
}

/** The visible text of an element, spaces collapsed. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/**
 * The focusable elements of `root` with the name they show, in DOM order (the
 * helper of HearthTheme.test.tsx's H4.2, narrowed to the banner here).
 */
function tabNames(root: Element): string[] {
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
    .map(text)
}

/** The app on the overview with the fake bff of `options`; the banner is in at once. */
function openApp(options: BffOptions = {}): HTMLElement {
  fakeBff(options)
  render(<App />)
  return screen.getByRole('banner')
}

/** The banner's two element children: the home link and, today, the tools. */
function childrenOf(banner: HTMLElement): Element[] {
  return [...banner.children]
}

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  startAt('')
})

// ── H5a.1: the tools group ──────────────────────────────────────────────

test('H5a.1: signed in, the header has exactly two element children, the home link and div.banner-tools, and the div holds the Menu button, the theme button and the session part in that order, with the names, ids and tab order of H4.2', async () => {
  const banner = openApp()
  await screen.findByRole('button', { name: en('session-sign-out') })
  expect(banner.tagName, 'the header is the banner').toBe('HEADER')
  const kids = childrenOf(banner)
  expect(kids.length, 'the header has exactly two element children').toBe(2)
  const [home, tools] = kids
  expect(shape(home!), 'the home link comes first').toEqual([
    'a',
    '',
    'app-name',
  ])
  expect(home!.getAttribute('href'), 'it is the way home').toBe('#/')
  expect(
    shape(tools!),
    'the second element child is one plain div of the class banner-tools',
  ).toEqual(['div', '', 'banner-tools'])
  expect(
    tools!.getAttribute('role'),
    'the div carries no role: HTML-AAM gives a div the generic role, which is not in the accessibility tree, so names, order and focus stay',
  ).toBeNull()
  expect(tools!.hasAttribute('aria-label'), 'it is not labelled').toBe(false)
  expect(
    tools!.hasAttribute('aria-labelledby'),
    'and not labelled by another element',
  ).toBe(false)
  expect(
    screen.getAllByRole('banner').length,
    'the wrapper adds no landmark: the header is still the only banner',
  ).toBe(1)
  expect(
    [...tools!.children].map(shape),
    'the group holds, in order, the Menu button, the theme button and the session part',
  ).toEqual([
    ['button', 'menu-button', 'menu-button secondary'],
    ['button', '', 'secondary theme-switch'],
    ['div', '', 'session'],
  ])
  const menu = tools!.children[0]!
  expect(
    menu.getAttribute('aria-expanded'),
    'the Menu button keeps what it says (H5a.7: the Menu rules do not move)',
  ).toBe('false')
  expect(menu.getAttribute('aria-controls'), 'and what it controls').toBe('nav')
  const buttons = within(tools! as HTMLElement).getAllByRole('button')
  expect(
    buttons.map(text),
    'the names by role inside the group: Menu, Dark theme, Sign out',
  ).toEqual([en('nav-menu'), en('theme-dark'), en('session-sign-out')])
  expect(
    within(tools! as HTMLElement).queryAllByRole('link'),
    'signed in, the group holds no link',
  ).toHaveLength(0)
  const name = tools!.querySelector('div.session > span')
  expect(
    name,
    'the signed-in user name stays in the DOM (H5a.5 hides it below 480 px, it never removes it)',
  ).not.toBeNull()
  expect(name!.textContent, 'the string by key').toBe(
    en('session-signed-in-as', 'alice'),
  )
  expect(
    tabNames(banner),
    'the tab order in the banner: home, Menu, Dark theme, Sign out (H4.2)',
  ).toEqual([
    en('app-name'),
    en('nav-menu'),
    en('theme-dark'),
    en('session-sign-out'),
  ])
})

// 09 S1a.4: signed out there is no nav, so the Menu button and the session
// pill go with it. The theme button stays (the brief says so, section 6).
test('09 S1a.4 (H5a.1): signed out, the tools group holds the theme button only, and the tab order is home, Dark theme', async () => {
  const banner = openApp({ session: 'signed-out' })
  const kids = childrenOf(banner)
  expect(kids.length, 'two element children signed out as well').toBe(2)
  const tools = kids[1]!
  expect(shape(tools), 'the same wrapper').toEqual(['div', '', 'banner-tools'])
  expect(
    [...tools.children].map(shape),
    'the group holds the theme button only: the Menu button and the session part went with the nav',
  ).toEqual([['button', '', 'secondary theme-switch']])
  expect(
    within(tools as HTMLElement).queryAllByRole('link'),
    'no Sign in link anywhere in the group (09 S1a.4)',
  ).toHaveLength(0)
  expect(
    within(tools as HTMLElement)
      .getAllByRole('button')
      .map(text),
    'Dark theme, and no Menu',
  ).toEqual([en('theme-dark')])
  expect(document.getElementById('menu-button'), 'no Menu button').toBeNull()
  expect(
    tabNames(banner),
    'the tab order in the banner: home, Dark theme',
  ).toEqual([en('app-name'), en('theme-dark')])
})

test('09 S1a.1 (H5a.1): while the session is read, the tools group holds the theme button and nothing else', () => {
  // Nothing is awaited: this is the state before the session reply lands.
  const pending = deferred()
  const banner = openApp({ session: () => pending.promise })
  const kids = childrenOf(banner)
  expect(kids.length, 'two element children while the session is read').toBe(2)
  const tools = kids[1]!
  expect(shape(tools), 'the same wrapper').toEqual(['div', '', 'banner-tools'])
  expect(
    [...tools.children].map(shape),
    'the group holds the theme button only: neither the session part nor the Menu button show while the session is read',
  ).toEqual([['button', '', 'secondary theme-switch']])
  expect(tools.querySelectorAll('.session').length, 'no session part').toBe(0)
  expect(
    within(tools as HTMLElement).queryAllByRole('link'),
    'no Sign in link while the session is read',
  ).toHaveLength(0)
  expect(
    within(tools as HTMLElement)
      .queryAllByRole('button')
      .map(text),
    'the tool button is named as in H4.2',
  ).toEqual([en('theme-dark')])
})
