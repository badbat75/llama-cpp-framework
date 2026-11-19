// fe/src/HearthIcons.test.tsx
// H8a, the Lucide icons of the shell (fe/briefs/08-hearth-h8-icons.md), the DOM
// plane: H8a.1 the `Icon` component for every name of the brief's shapes block,
// H8a.2 the nav, H8a.3 the banner with the moon moved out of `ThemeSwitch.tsx`,
// H8a.8 the sweep over every `svg` of the shell, and with it the mark count of
// H8a.9. The stylesheet plane (H8a.4 to H8a.7) is in HearthH8a.test.ts.
//
// `Icon.tsx` exists (H8a.1 is written), so the module loads and every case
// runs. The import stays INSIDE the cases that need it, and the path stays in
// a variable: a module the bundler cannot analyse at transform time reds the
// cases alone, never the whole file, which is what names the criterion that is
// unmet if the component ever moves.
//
// Every icon is decorative, so no accessible name moves: the links and buttons
// are still found by the name the strings file gives them, and the names of the
// banner's tab order are the text, unchanged (H4.2, H5a.1). jsdom lays nothing
// out: the sizes and the alignment are read as declarations in the other file,
// and how it looks is the designer's UX pass.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { createElement, type ReactElement } from 'react'
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { en, fakeBff, isHidden, startAt, type BffOptions } from './test/bff.ts'

// ── The shapes of the brief's list, as data ───────────────────────────────
// Each entry is the children of the `svg`, in order: the tag and every
// attribute. The table is a hand-typed copy of the brief's shapes block
// (section 1), not parsed out of the brief: the brief is the source, and every
// path here was typed from it, so a typo in a path reds the case that compares
// it against the component.
type Shape = [tag: string, attrs: Record<string, string>]

const SHAPES: Record<string, Shape[]> = {
  house: [
    ['path', { d: 'M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8' }],
    [
      'path',
      {
        d: 'M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
      },
    ],
  ],
  layers: [
    [
      'path',
      {
        d: 'M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z',
      },
    ],
    [
      'path',
      {
        d: 'M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12',
      },
    ],
    [
      'path',
      {
        d: 'M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17',
      },
    ],
  ],
  menu: [
    ['path', { d: 'M4 5h16' }],
    ['path', { d: 'M4 12h16' }],
    ['path', { d: 'M4 19h16' }],
  ],
  moon: [
    [
      'path',
      {
        d: 'M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401',
      },
    ],
  ],
  'log-in': [
    ['path', { d: 'm10 17 5-5-5-5' }],
    ['path', { d: 'M15 12H3' }],
    ['path', { d: 'M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4' }],
  ],
  'log-out': [
    ['path', { d: 'm16 17 5-5-5-5' }],
    ['path', { d: 'M21 12H9' }],
    ['path', { d: 'M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4' }],
  ],
  play: [
    [
      'path',
      {
        d: 'M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z',
      },
    ],
  ],
  square: [['rect', { width: '18', height: '18', x: '3', y: '3', rx: '2' }]],
  'rotate-cw': [
    ['path', { d: 'M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8' }],
    ['path', { d: 'M21 3v5h-5' }],
  ],
  'chevron-left': [['path', { d: 'm15 18-6-6 6-6' }]],
  // In the map from H8a, for the chevrons of H10 and H11a: no placement yet.
  'chevron-down': [['path', { d: 'm6 9 6 6 6-6' }]],
  search: [
    ['path', { d: 'm21 21-4.34-4.34' }],
    ['circle', { cx: '11', cy: '11', r: '8' }],
  ],
  info: [
    ['circle', { cx: '12', cy: '12', r: '10' }],
    ['path', { d: 'M12 16v-4' }],
    ['path', { d: 'M12 8h.01' }],
  ],
  'triangle-alert': [
    [
      'path',
      {
        d: 'm21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3',
      },
    ],
    ['path', { d: 'M12 9v4' }],
    ['path', { d: 'M12 17h.01' }],
  ],
  'lock-open': [
    ['rect', { width: '18', height: '11', x: '3', y: '11', rx: '2', ry: '2' }],
    ['path', { d: 'M7 11V7a5 5 0 0 1 9.9-1' }],
  ],
  'circle-alert': [
    ['circle', { cx: '12', cy: '12', r: '10' }],
    ['line', { x1: '12', x2: '12', y1: '8', y2: '12' }],
    ['line', { x1: '12', x2: '12.01', y1: '16', y2: '16' }],
  ],
}

const NAMES = Object.keys(SHAPES)

// The eight attributes every icon carries (section 0). No `width`/`height` (CSS
// sizes it), no `title`, no label prop: every icon is decorative.
const ICON_ATTRS: Record<string, string> = {
  'aria-hidden': 'true',
  focusable: 'false',
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': '2',
  'stroke-linecap': 'round',
  'stroke-linejoin': 'round',
}

// Attributes an icon must NOT carry: a size of its own, a name of its own, a
// place in the tab order.
const BANNED = [
  'width',
  'height',
  'title',
  'aria-label',
  'aria-labelledby',
  'role',
  'tabindex',
]

/** The visible text of an element, spaces collapsed. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** `[tag, every attribute]` of an element: the shape of one child of an icon. */
function childOf(el: Element): Shape {
  return [
    el.tagName.toLowerCase(),
    Object.fromEntries([...el.attributes].map((a) => [a.name, a.value])),
  ]
}

/**
 * The component of the brief. `Icon.tsx` exists (H8a.1 is written), so the
 * import answers. Its path is kept in a variable, so the bundler cannot
 * analyse it at transform time: the module loads, and a case that needs it
 * reds on its own import, naming the module, if the component ever moves.
 *
 * `Icon`'s props name the `IconName` union, and a function of a union prop
 * is not assignable to one of a plain `string` prop, so the module is read
 * through this shape: the table above is data, and the cast is the tester's.
 */
type IconModule = { Icon: (props: { name: string }) => ReactElement }
const ICON_MODULE = './Icon.tsx'
const iconModule = async (): Promise<IconModule> =>
  (await import(ICON_MODULE)) as unknown as IconModule

/**
 * `host`'s first element child is the icon of `name`: an `svg.icon` with the
 * eight attributes, the shapes of the table, and no text of its own.
 */
function expectIcon(host: Element, name: string, why: string): void {
  const icon = host.children[0]
  expect(icon, `${why}: the host has a first element child`).toBeTruthy()
  expect(
    host.firstChild,
    `${why}: the icon is the host's FIRST child, text nodes included: the brief places the icon before the words it sits beside`,
  ).toBe(icon)
  expect(
    icon!.tagName.toLowerCase(),
    `${why}: the icon is an svg, and it is the FIRST child`,
  ).toBe('svg')
  expect(
    icon!.getAttribute('class'),
    `${why}: of the class the stylesheet sizes (.icon)`,
  ).toBe('icon')
  for (const [key, value] of Object.entries(ICON_ATTRS))
    expect(icon!.getAttribute(key), `${why}: the svg's ${key}`).toBe(value)
  expect(
    [...icon!.children].map(childOf),
    `${why}: the children of the brief's shapes block, in order`,
  ).toEqual(SHAPES[name])
  expect(text(icon!), `${why}: the icon draws, it holds no text`).toBe('')
}

/**
 * The focusable elements of `root` with the name they show, in DOM order (the
 * helper of HearthBanner.test.tsx, copied: a test file imports no other test
 * file). An `svg` with `aria-hidden` adds no text, so the names are the text.
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

/** The shell on the overview with the fake bff of `options`. */
function openApp(options: BffOptions = {}): void {
  fakeBff(options)
  render(<App />)
}

/** The banner, once the session has settled. */
async function settledBanner(): Promise<HTMLElement> {
  await screen.findByRole('link', { name: en('nav-overview') })
  return screen.getByRole('banner')
}

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  startAt('')
})

// ── H8a.1: the component, one case per name ───────────────────────────────

test.each(NAMES)(
  'H8a.1: Icon renders %s as one svg.icon with the eight attributes, the shapes of the brief and nothing else',
  async (name) => {
    const { Icon } = await iconModule()
    const { container } = render(createElement(Icon, { name }))
    const svgs = container.querySelectorAll('svg')
    expect(svgs.length, `${name}: exactly one svg, no wrapper`).toBe(1)
    const svg = svgs[0]!
    expect(svg.getAttribute('class'), `${name}: the class CSS sizes`).toBe(
      'icon',
    )
    for (const [key, value] of Object.entries(ICON_ATTRS))
      expect(svg.getAttribute(key), `${name}: the svg's ${key}`).toBe(value)
    for (const key of BANNED)
      expect(
        svg.hasAttribute(key),
        `${name}: no ${key} attribute: the CSS sizes the box, and the icon is decorative, so it carries no name of its own`,
      ).toBe(false)
    expect(
      [...svg.children].map(childOf),
      `${name}: the children of the brief's shapes block, in order, with every attribute`,
    ).toEqual(SHAPES[name])
    expect(svg.textContent, `${name}: the svg holds no text`).toBe('')
  },
)

// ── H8a.2: the nav ────────────────────────────────────────────────────────

test('H8a.2: the Overview link holds Icon house then its text and the Model profiles link Icon layers then its text, the instance links hold no icon, and the names by role are unchanged', async () => {
  openApp()
  const nav = await screen.findByRole('navigation', { name: en('nav-label') })
  await within(nav).findByRole('link', { name: 'default' })
  const overview = within(nav).getByRole('link', { name: en('nav-overview') })
  expectIcon(overview, 'house', 'the Overview link')
  expect(
    text(overview),
    'the text is the string, so the name the screen reader reads is unchanged',
  ).toBe(en('nav-overview'))
  const profiles = within(nav).getByRole('link', {
    name: en('nav-model-profiles'),
  })
  expectIcon(profiles, 'layers', 'the Model profiles link')
  expect(text(profiles), 'and its name').toBe(en('nav-model-profiles'))
  for (const id of ['default', 'coder']) {
    const link = within(nav).getByRole('link', { name: id })
    expect(text(link), `the instance link ${id} keeps its id as its text`).toBe(
      id,
    )
    expect(
      link.querySelector('svg'),
      `the instance link ${id} holds no icon: its id is data`,
    ).toBeNull()
  }
  expect(
    nav.querySelectorAll('svg.icon').length,
    'two icons in the nav: the two named entries, no icon on an instance id',
  ).toBe(2)
})

// ── H8a.3: the banner ─────────────────────────────────────────────────────

test('H8a.3 signed in: the Menu button holds Icon menu, the theme button Icon moon then its label, Sign out holds Icon log-out, and the names, ids, roles, aria-pressed, aria-expanded and tab order of H4.2 and H5a.1 are unchanged', async () => {
  openApp()
  const banner = await settledBanner()
  await screen.findByRole('button', { name: en('session-sign-out') })
  const menu = within(banner).getByRole('button', { name: en('nav-menu') })
  expectIcon(menu, 'menu', 'the Menu button')
  expect(menu.id, 'the id Escape and aria-controls use').toBe('menu-button')
  expect(
    menu.getAttribute('aria-expanded'),
    'the state the button says (H5a.7)',
  ).toBe('false')
  expect(menu.getAttribute('aria-controls'), 'and what it controls').toBe('nav')
  const theme = within(banner).getByRole('button', { name: en('theme-dark') })
  expectIcon(theme, 'moon', 'the theme button')
  expect(
    [...theme.children].map((el) => el.tagName.toLowerCase()),
    'the icon then the label, as H4.1 pins',
  ).toEqual(['svg', 'span'])
  expect(
    theme.getAttribute('aria-pressed'),
    'the state is a pressed button, not a switch (H4.1)',
  ).toBeTruthy()
  const label = theme.children[1]!
  expect(
    label.classList.contains('theme-switch-label'),
    'the label span carries the class H5 hides on narrow screens',
  ).toBe(true)
  expect(text(label), 'the label is the string, not a state word').toBe(
    en('theme-dark'),
  )
  const signOut = within(banner).getByRole('button', {
    name: en('session-sign-out'),
  })
  expectIcon(signOut, 'log-out', 'Sign out')
  expect(
    tabNames(banner),
    'the tab order in the banner is the text, in the order of H4.2: an icon adds no stop and no name',
  ).toEqual([
    en('app-name'),
    en('nav-menu'),
    en('theme-dark'),
    en('session-sign-out'),
  ])
})

test('H8a.3 signed out: the banner holds no Sign in and no Menu button (09 S1a.4), the theme icon is there as signed in, and the tab order is home then Dark theme', async () => {
  openApp({ session: 'signed-out' })
  // The session settled: the sign-in page is on screen (09 S1a.1).
  await screen.findByRole('heading', { level: 1, name: en('signin-title') })
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('link', { name: en('session-sign-in') }),
    '09 S1a.4: no a.session in the banner: the page IS the form',
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('nav-menu') }),
    '09 S1a.4: no Menu button signed out',
  ).toBeNull()
  expect(
    document.getElementById('menu-button'),
    'and its id is gone with it',
  ).toBeNull()
  expectIcon(
    within(banner).getByRole('button', { name: en('theme-dark') }),
    'moon',
    'the theme button',
  )
  expect(
    tabNames(banner),
    'the tab order in the banner signed out: home, Dark theme (09 S1a.5)',
  ).toEqual([en('app-name'), en('theme-dark')])
})

// The shape moved into the component: the inline drawing is gone from the file,
// so a second copy of the moon cannot drift from the map.
test('H8a.3: ThemeSwitch.tsx renders Icon and holds no drawing of its own (comments dropped, so a needle in a comment is no hit)', () => {
  const src = readFileSync(join(import.meta.dirname, 'ThemeSwitch.tsx'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/^\s*\/\/.*$/gm, '')
  expect(
    src.includes('const MOON'),
    'the MOON constant is gone: the moon comes from Icon',
  ).toBe(false)
  expect(
    src.includes(SHAPES['moon']![0]![1].d),
    'the path literal of the moon is gone from the file',
  ).toBe(false)
  expect(
    src.includes('<svg'),
    'no inline svg is left in the theme switch',
  ).toBe(false)
  expect(
    src.includes('./Icon.tsx'),
    'the file takes the shape from the component',
  ).toBe(true)
})

// ── H8a.8: the sweep, and the mark of H8a.9 ───────────────────────────────

test.each([
  { signed: 'in', options: {} as BffOptions, banner: 3, nav: 2 },
  // 09 S1a.4: signed out there is no nav, so the nav has no icon to sweep and
  // the banner has the theme icon only.
  {
    signed: 'out',
    options: { session: 'signed-out' } as BffOptions,
    banner: 1,
    nav: 0,
  },
])(
  'H8a.8 signed $signed: every svg of the banner and the nav is aria-hidden and not focusable, the icons are the ones of the shell, and no icon is alone',
  async ({ options, banner: wantBanner, nav: wantNav }) => {
    openApp(options)
    // The session settles first: signed in, the nav appears with it; signed
    // out there is no nav at all (09 S1a.4), and the sign-in page is what
    // settles the read.
    if (wantNav === 0) {
      await screen.findByRole('heading', {
        level: 1,
        name: en('signin-title'),
      })
    } else {
      await screen.findByRole('navigation', { name: en('nav-label') })
    }
    const banner = screen.getByRole('banner')
    const nav = screen.queryByRole('navigation', { name: en('nav-label') })
    expect(
      nav === null,
      wantNav === 0 ? 'no nav signed out (09 S1a.4)' : 'the nav is there',
    ).toBe(wantNav === 0)
    const navEl: Element = nav ?? document.createElement('div')
    const svgs = [
      ...banner.querySelectorAll('svg'),
      ...navEl.querySelectorAll('svg'),
    ]
    expect(
      svgs.length,
      'the shell draws: the banner and the nav hold svgs at all',
    ).toBeGreaterThan(0)
    for (const svg of svgs) {
      const who = `${svg.getAttribute('class') ?? 'svg of no class'} in ${
        banner.contains(svg) ? 'the banner' : 'the nav'
      }`
      expect(
        svg.getAttribute('aria-hidden'),
        `${who}: assistive technology reads the text beside it, not the drawing (H8a.8)`,
      ).toBe('true')
      expect(
        svg.getAttribute('focusable'),
        `${who}: it takes no place in the tab order (IE and legacy engines)`,
      ).toBe('false')
    }
    expect(
      banner.querySelectorAll('svg.icon').length,
      'the banner holds exactly the icons of the shell: Menu, the theme and the session part',
    ).toBe(wantBanner)
    expect(
      navEl.querySelectorAll('svg.icon').length,
      'the nav holds the icons of its named entries only',
    ).toBe(wantNav)
    expect(
      banner.querySelectorAll('svg.mark').length,
      'the mark is one svg of its own class (H8a.9: the mark does not move)',
    ).toBe(1)
    expect(
      banner.querySelectorAll('svg.mark.icon').length,
      'the mark keeps the class H3 gives it, it is not an .icon',
    ).toBe(0)
    for (const icon of [
      ...banner.querySelectorAll('.icon'),
      ...navEl.querySelectorAll('.icon'),
    ]) {
      const host = icon.parentElement
      expect(host, 'an icon has a host').not.toBeNull()
      expect(
        text(host!),
        'no icon is alone: the element that holds it holds the text it sits beside (1.4.1: nothing reads only from an icon)',
      ).not.toBe('')
    }
  },
)
