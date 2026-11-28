// fe/src/NodesMenu2.test.tsx
// Brief 20, slice N1 (fe/briefs/20-nodes-menu.md), criteria 20 N.7 to 20 N.12:
// the Node info entry at the end of the host's list, the Profiles group, About
// alone in the last list, the keyboard and menu order, the two rules of
// `index.css` with the two new icon shapes, and the signed-out shell with the
// Overview's h2. 20 N.1 to 20 N.6 are in NodesMenu1.test.tsx.
//
// The app is rendered whole against the fake bff. jsdom does no layout, so the
// two rules of 20 N.11 are pinned in the stylesheet text the way
// HearthH5b.test.ts and Reflow.test.ts read it (a small local copy of that
// reader, as those files keep their own), and what they do at 320 px, with a
// 40-character id and a 40-character profile, is the designer's UX pass (brief
// section 6). The route rules of 20 N.8 are pinned on parseRoute, the function
// the shell routes with, and on the page the address lands on.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { createElement, type ReactElement } from 'react'
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { parseRoute } from './route.ts'
import {
  NODE,
  VERSION,
  VERSION_HOST,
  en,
  fakeBff,
  held,
  isHidden,
  navigate,
  reply,
  startAt,
  type BffOptions,
  type Instance,
} from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────────

/** The nav region, once it is there. */
const navOf = async (): Promise<HTMLElement> =>
  (await screen.findByRole('navigation', {
    name: en('nav-label'),
  })) as HTMLElement

/** The nav's links, in the order the document holds them: the tab order. */
const navLinks = (nav: HTMLElement): HTMLAnchorElement[] =>
  Array.from(nav.querySelectorAll('a'))

/** The words of an element, whitespace collapsed. */
const words = (el: Element | null): string =>
  (el?.textContent ?? '').replace(/\s+/g, ' ').trim()

/** The text an eye reads: every text node outside a `.visually-hidden` run. */
function shown(root: Node): string {
  let out = ''
  for (const node of Array.from(root.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) out += node.textContent ?? ''
    else if (
      node.nodeType === Node.ELEMENT_NODE &&
      !(node as Element).matches('.visually-hidden')
    )
      out += shown(node)
  }
  return out.replace(/\s+/g, ' ').trim()
}

const byId = (id: string): HTMLElement | null => document.getElementById(id)

/** The nav's list of the one host, once the instance links are there. */
const hostList = (nav: HTMLElement): HTMLUListElement | null =>
  nav.querySelector('ul[aria-labelledby="nav-host"]')

/** The Menu button. */
const menuButton = (): HTMLElement =>
  screen.getByRole('button', { name: en('nav-menu') })

/** The focusable elements, in document order. */
function tabbable(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el) &&
      el.closest('dialog:not([open])') === null,
  )
}

/** The app, signed in, with the host's list on screen. The nav arrives with
 * the session and the links with the list read, so the helper waits for one,
 * by its address: a fixture that carries a server profile names the link
 * "id, server profile x" (20 N.5), where a name of `default` matches nothing
 * (brief section 5). */
async function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  const nav = await navOf()
  await waitFor(() =>
    expect(
      nav.querySelector(
        'ul[aria-labelledby="nav-host"] a[href^="#/instances/"]',
      ),
    ).not.toBeNull(),
  )
  return { bff, nav }
}

/** The two instances of 20 N.10: one with a server profile, one without. */
const TWO: Instance[] = [
  {
    id: 'default',
    engine: 'llama.cpp',
    state: 'active',
    serverProfile: 'qwen-27b',
  },
  { id: 'coder', engine: 'llama.cpp', state: 'inactive', serverProfile: null },
]

/** The icon a link is drawn with: its first child, decorative. */
function iconOf(link: Element, what: string): Element {
  const icon = link.firstElementChild
  expect(icon, `${what}: the link holds an icon`).not.toBeNull()
  expect(icon?.tagName.toLowerCase(), `${what}: an svg`).toBe('svg')
  expect(
    icon?.classList.contains('icon'),
    `${what}: the class the sheet sizes`,
  ).toBe(true)
  expect(icon?.getAttribute('aria-hidden'), `${what}: decorative`).toBe('true')
  return icon!
}

// The children of a shape: the tag and every attribute. A hand-typed copy of
// lucide-static 1.74.0 `icons/<name>.svg`, the source 20 N.11 names, typed in
// the file's own order, so a path the component spells differently is red.
type Shape = [tag: string, attrs: Record<string, string>]

// `IconName` is a union of the names the component knows, and a component of a
// union prop is not assignable to one of a plain `string` prop, so the module is
// read through this shape (as HearthIcons.test.tsx does): the table below is
// data, and the cast is the tester's. The path is kept in a variable so the
// bundler cannot analyse it at transform time.
const ICON_MODULE = './Icon.tsx'

const LUCIDE: Record<string, Shape[]> = {
  // 20 N.4: the host label.
  computer: [
    ['path', { d: 'M12 18h6' }],
    ['path', { d: 'M6 18h.01' }],
    ['path', { d: 'M8 6h1' }],
    ['rect', { x: '2', y: '14', width: '20', height: '8', rx: '2' }],
    ['rect', { x: '4', y: '2', width: '16', height: '12', rx: '2' }],
  ],
  // 20 N.5: every instance link.
  box: [
    [
      'path',
      {
        d: 'M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z',
      },
    ],
    ['path', { d: 'm3.3 7 8.7 5 8.7-5' }],
    ['path', { d: 'M12 22V12' }],
  ],
  // 18 AN.6, kept by 20 N.7: Node info.
  cpu: [
    ['path', { d: 'M12 20v2' }],
    ['path', { d: 'M12 2v2' }],
    ['path', { d: 'M17 20v2' }],
    ['path', { d: 'M17 2v2' }],
    ['path', { d: 'M2 12h2' }],
    ['path', { d: 'M2 17h2' }],
    ['path', { d: 'M2 7h2' }],
    ['path', { d: 'M20 12h2' }],
    ['path', { d: 'M20 17h2' }],
    ['path', { d: 'M20 7h2' }],
    ['path', { d: 'M7 20v2' }],
    ['path', { d: 'M7 2v2' }],
    ['rect', { x: '4', y: '4', width: '16', height: '16', rx: '2' }],
    ['rect', { x: '8', y: '8', width: '8', height: '8', rx: '1' }],
  ],
  // 13 N.6, kept by 20 N.9: About.
  info: [
    ['circle', { cx: '12', cy: '12', r: '10' }],
    ['path', { d: 'M12 16v-4' }],
    ['path', { d: 'M12 8h.01' }],
  ],
}

/** The shape of an `svg`, tag and attributes of every child, in order. */
const shapeOf = (svg: Element): Shape[] =>
  Array.from(svg.children).map((el) => [
    el.tagName.toLowerCase(),
    Object.fromEntries(Array.from(el.attributes).map((a) => [a.name, a.value])),
  ])

/** The `svg` inside `link` has the shape of lucide `name`. */
function expectShape(link: Element, name: string, what: string): void {
  expect(
    shapeOf(iconOf(link, what)),
    `${what}: the shape of lucide-static 1.74.0 icons/${name}.svg`,
  ).toEqual(LUCIDE[name]!)
}

// ── 20 N.7: Node info, at the end of the host's list ────────────────────────

test('20 N.7: the host list ends with the Node info link, its cpu icon first, its text nav-node, the Profiles group after the host', async () => {
  const { nav } = await open({ version: VERSION_HOST, instances: TWO })
  const list = hostList(nav)
  expect(list, 'the host list is there').not.toBeNull()
  const items = Array.from(list!.children)
  const last = items[items.length - 1]!
  expect(last.tagName.toLowerCase(), 'Node info is the last item').toBe('li')
  const links = Array.from(list!.querySelectorAll('a'))
  const node = links[links.length - 1]!
  expect(
    node.getAttribute('href'),
    'the address is unchanged (decision 4)',
  ).toBe('#/node')
  expect(words(node), 'the text is nav-node').toBe(en('nav-node'))
  expect(node.getAttribute('aria-label'), 'named by its text').toBeNull()
  expectShape(node, 'cpu', 'Node info keeps its cpu icon (decision 7)')
  expect(
    links.map((a) => a.getAttribute('href')),
    'the instance links first, in the manager order, Node info last',
  ).toEqual(['#/instances/default', '#/instances/coder', '#/node'])
  const groups = Array.from(nav.querySelectorAll(':scope > ul'))
  expect(
    groups[groups.length - 1]!.contains(node),
    'Node info is NOT in the nav last list: it moved into the host list',
  ).toBe(false)
})

test('20 N.7: the Node info link carries aria-current="page" on #/node and none on the other pages, and the page still answers with its h1', async () => {
  const { nav } = await open({ version: VERSION_HOST, node: NODE })
  const node = () => within(nav).getByRole('link', { name: en('nav-node') })
  expect(node().hasAttribute('aria-current'), 'not on the Overview').toBe(false)
  for (const hash of ['#/instances/default', '#/profiles/model', '#/about']) {
    navigate(hash)
    await screen.findByRole('main')
    expect(node().hasAttribute('aria-current'), `not on ${hash}`).toBe(false)
  }
  navigate('#/node')
  await screen.findByRole('heading', { level: 1, name: en('node-title') })
  expect(node().getAttribute('aria-current'), 'on its own page').toBe('page')
})

// ── 20 N.8: Profiles ────────────────────────────────────────────────────────

// Brief 16, 17a.2, added the Server profiles entry to this group; brief 26
// (26.1) added the Instance profiles entry. The group is the same, the list
// holds the three entries, Instance profiles first.
test('20 N.8 (17a.2, 26.1): the Profiles group is p#nav-profiles reading nav-profiles, text only, and its list holds the Instance profiles, the Server profiles and the Model profiles link, in that order', async () => {
  const { nav } = await open({ version: VERSION_HOST, instances: TWO })
  const label = byId('nav-profiles')
  expect(label?.tagName, 'a paragraph, like every nav label').toBe('P')
  expect(label?.classList.contains('nav-label'), 'reusing .nav-label').toBe(
    true,
  )
  expect(words(label), 'reading nav-profiles').toBe(en('nav-profiles'))
  expect(
    label?.querySelector('svg'),
    'a group label stays text only (decision 7)',
  ).toBeNull()
  const list = nav.querySelector('ul[aria-labelledby="nav-profiles"]')
  expect(list, 'the list is named by the label').not.toBeNull()
  expect(
    Array.from(list!.querySelectorAll('a')).map((a) => a.getAttribute('href')),
    'Instance profiles, then Server profiles, then Model profiles',
  ).toEqual(['#/profiles/instance', '#/profiles/server', '#/profiles/model'])
})

test('20 N.8 (17a.1, 17a.2): the Model profiles entry keeps 2d.64 (page on the list, true on a profile page), and #/profiles/server is the list page, with the group entry current', async () => {
  // The route rule of 20 N.8, on the function the shell routes with. Brief 16
  // 17a.1 gave the address a page.
  expect(parseRoute('#/profiles/server'), 'the list').toEqual({
    page: 'server-profiles',
  })
  expect(parseRoute('#/profiles/server/edge-01'), 'one profile').toEqual({
    page: 'server-profile',
    id: 'edge-01',
  })
  const { nav } = await open({ version: VERSION_HOST, instances: TWO })
  const model = () =>
    within(nav).getByRole('link', { name: en('nav-model-profiles') })
  const server = () =>
    within(nav).getByRole('link', { name: en('nav-server-profiles') })
  expect(model().hasAttribute('aria-current'), 'not on the Overview').toBe(
    false,
  )
  navigate('#/profiles/model')
  await screen.findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  expect(model().getAttribute('aria-current'), 'page on the list').toBe('page')
  navigate('#/profiles/model/gemma-3-12b-it')
  await screen.findByRole('heading', { level: 1, name: 'gemma-3-12b-it' })
  expect(model().getAttribute('aria-current'), 'true on a profile page').toBe(
    'true',
  )
  navigate('#/profiles/server')
  await screen.findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  expect(server().getAttribute('aria-current'), 'page on the pool list').toBe(
    'page',
  )
  expect(model().hasAttribute('aria-current'), 'not the other entry').toBe(
    false,
  )
  navigate('#/profiles/server/edge-01')
  await screen.findByRole('heading', { level: 1, name: 'edge-01' })
  expect(server().getAttribute('aria-current'), 'true on a profile page').toBe(
    'true',
  )
})

// ── 20 N.9: About, alone in the last list ───────────────────────────────────

test('20 N.9: the last list of the nav has no label and holds one item, the About link with its info icon first, About last in the nav, aria-current page on #/about only', async () => {
  const { nav } = await open({ version: VERSION_HOST, instances: TWO })
  const groups = Array.from(nav.querySelectorAll(':scope > ul'))
  const last = groups[groups.length - 1]!
  expect(
    last.hasAttribute('aria-labelledby'),
    'no label: one entry is not a group',
  ).toBe(false)
  const items = Array.from(last.children)
  expect(
    items.map((el) => el.tagName.toLowerCase()),
    'one item',
  ).toEqual(['li'])
  const links = Array.from(last.querySelectorAll('a'))
  expect(
    links.map((a) => a.getAttribute('href')),
    'and it is About',
  ).toEqual(['#/about'])
  expectShape(links[0]!, 'info', 'About keeps its info icon')
  expect(words(links[0]!), 'the text is nav-about').toBe(en('nav-about'))
  expect(
    navLinks(nav).at(-1)?.getAttribute('href'),
    'About is the last link the nav reads',
  ).toBe('#/about')
  const about = () => within(nav).getByRole('link', { name: en('nav-about') })
  expect(about().hasAttribute('aria-current'), 'not on the Overview').toBe(
    false,
  )
  for (const hash of ['#/instances/default', '#/node', '#/profiles/model']) {
    navigate(hash)
    await screen.findByRole('main')
    expect(about().hasAttribute('aria-current'), `not on ${hash}`).toBe(false)
  }
  navigate('#/about')
  await screen.findByRole('heading', { level: 1, name: en('about-title') })
  expect(about().getAttribute('aria-current'), 'on its own page').toBe('page')
})

// ── 20 N.10: keyboard and menu ──────────────────────────────────────────────

test('20 N.10: at #/about the Tab path is the shell, Overview, the two instance links in the manager order, Node info, Model profiles, About, and no label takes a stop', async () => {
  const { nav } = await open({ version: VERSION_HOST, instances: TWO })
  navigate('#/about')
  const h1 = await screen.findByRole('heading', {
    level: 1,
    name: en('about-title'),
  })
  expect(document.activeElement, 'the page takes the focus on its h1').toBe(h1)
  // The Menu button is the shell's, before the theme switch (H5b.7, 13 N.11),
  // and the visible text is what Tab names. 16 17f.2: the entry is the name
  // alone, so the visible text IS the name (brief 20 decision 5 is closed).
  expect(
    tabbable().map(shown),
    'the tab order of 20 N.10, About last, the page adding no stop',
  ).toEqual([
    en('skip-to-content'),
    en('app-name'),
    en('nav-menu'),
    en('theme-dark'),
    en('session-sign-out'),
    en('nav-overview'),
    'default',
    'coder',
    en('nav-node'),
    // Brief 26 (26.1): Instance profiles leads the Profiles group.
    en('nav-instance-profiles'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
  expect(
    tabbable().filter((el) => el.closest('.nav-label') !== null).length,
    'Nodes, the host and Profiles are words: no stop',
  ).toBe(0)
  expect(
    nav.contains(document.activeElement as Node),
    'the focus is not in the nav',
  ).toBe(false)
})

test('20 N.10: the Menu button opens the nav holding the same links in the same order, a click on an instance, Node info, Model profiles or About closes it, Escape closes it with the focus on the Menu button', async () => {
  const { nav } = await open({ version: VERSION_HOST, instances: TWO })
  const button = menuButton()
  expect(button.getAttribute('aria-expanded'), 'closed at first').toBe('false')
  fireEvent.click(button)
  expect(button.getAttribute('aria-expanded'), 'the click opens it').toBe(
    'true',
  )
  expect(
    navLinks(nav).map((a) => shown(a)),
    'the same links, in the same order, as the Tab path',
  ).toEqual([
    en('nav-overview'),
    'default',
    'coder',
    en('nav-node'),
    // Brief 26 (26.1): Instance profiles leads the Profiles group.
    en('nav-instance-profiles'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
  for (const href of [
    '#/instances/default',
    '#/node',
    '#/profiles/model',
    '#/about',
  ]) {
    const link = nav.querySelector<HTMLAnchorElement>(`a[href="${href}"]`)!
    expect(link, `${href}: the entry is there`).toBeTruthy()
    fireEvent.click(link)
    expect(
      button.getAttribute('aria-expanded'),
      `${href}: following the entry closes the menu`,
    ).toBe('false')
    await screen.findByRole('main')
    fireEvent.click(button)
    expect(button.getAttribute('aria-expanded'), `${href}: opened again`).toBe(
      'true',
    )
  }
  fireEvent.keyDown(button, { key: 'Escape' })
  expect(button.getAttribute('aria-expanded'), 'Escape closes it').toBe('false')
  expect(
    document.activeElement,
    'and the focus is on the Menu button, where the user is',
  ).toBe(button)
})

// ── 20 N.11: the two rules and the two shapes ───────────────────────────────
// jsdom does no layout: the stylesheet is read as text, the way HearthH5b.test.ts
// and Reflow.test.ts read it. The reader is a small local copy, as those files
// export no helper.

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const RULES = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim(),
  compact: m[1].replace(/\s+/g, ''),
  flat: m[1].trim().replace(/\s+/g, ' '),
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: the order of the rules inside an @media block says
// nothing about the cascade of the top-level ones.
const TOP = RULES.filter((r) => r.depth === 0)

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

// Every property a block declares, custom properties included.
const declared = (body: string): string[] =>
  body
    .split(';')
    .map((d) => d.slice(0, d.indexOf(':')).trim())
    .filter((name) => name !== '')

const single = (list: typeof RULES, what: string): (typeof RULES)[number] => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length
          ? ` (${list.map((r) => r.sel.replace(/\s+/g, ' ')).join(' | ')})`
          : ''),
    )
  return list[0]
}

const topBy = (compact: string, what: string) =>
  single(
    TOP.filter((r) => r.compact === compact),
    `the top-level rule whose selector is exactly ${compact} (${what})`,
  )

test('20 N.11 (as the N1 review amends it): index.css gains .nav-label:has(> .icon) with the entries layout, and .nav-instance with flex 1 1 4rem, min-width 0 and overflow-wrap anywhere, each one rule, each declaring nothing else and no custom property', () => {
  const host = topBy(
    '.nav-label:has(>.icon)',
    'the host label laying its icon out as the entries do',
  )
  expect(
    declared(host.body).sort(),
    'the rule declares the three properties of 20 N.11 and nothing else',
  ).toEqual(['align-items', 'display', 'gap'])
  expect(prop(host.body, 'display'), 'a row, like .nav a').toBe('flex')
  expect(prop(host.body, 'align-items'), 'centred').toBe('center')
  expect(prop(host.body, 'gap'), 'the entries gap (H8a.5)').toBe(
    'var(--space-2)',
  )

  const instance = topBy('.nav-instance', 'the instance text run')
  expect(
    declared(instance.body).sort(),
    'the rule declares the three properties of the amended 20 N.11 and nothing else',
  ).toEqual(['flex', 'min-width', 'overflow-wrap'])
  expect(
    prop(instance.body, 'flex'),
    'the basis that keeps the run beside the icon at 100 % text (decision 8, N1 review D1)',
  ).toBe('1 1 4rem')
  expect(
    prop(instance.body, 'min-width'),
    'so the flex pill can shrink below the text',
  ).toBe('0')
  expect(
    prop(instance.body, 'overflow-wrap'),
    'anywhere, not break-word: an id with no break point stays one word the pill must fit (DESIGN.md 4)',
  ).toBe('anywhere')

  // "Exactly two rules and no custom property": no other rule styles the new
  // classes (the sheet's `.nav a > span:not(.nav-instance)` excludes it by
  // name, so it is not one of them), and neither rule declares a `--` property
  // (H1a.12, no size token).
  const naming = (needle: string) =>
    RULES.filter(
      (r) =>
        r.compact.includes(needle) &&
        !(needle === '.nav-instance' && r.compact.includes(':not(')),
    ).map((r) => r.sel.replace(/\s+/g, ' '))
  expect(naming('.nav-instance'), 'one rule styles the instance text').toEqual([
    '.nav-instance',
  ])
  expect(
    naming('.nav-label:has'),
    'one rule styles the host label with its icon',
  ).toEqual(['.nav-label:has(> .icon)'])
  for (const rule of [host, instance])
    expect(
      declared(rule.body).filter((name) => name.startsWith('--')),
      'no custom property: no token, no colour (decision 8)',
    ).toEqual([])
  // The host list rides the rule that indents a labelled group (H5b.5).
  expect(
    topBy(
      '.navul[aria-labelledby]',
      'the labelled nav group indented beside a rule',
    ).flat,
    'the nested host list takes the group rule',
  ).toBe('.nav ul[aria-labelledby]')
})

test('20 N.11: Icon renders computer and box with the path data of lucide-static 1.74.0, and Icon.tsx cites the criterion beside each', async () => {
  const source = readFileSync(join(import.meta.dirname, 'Icon.tsx'), 'utf8')
  for (const [name, citation] of [
    ['computer', '20 N.4'],
    ['box', '20 N.5'],
  ] as const) {
    // The citation sits in the entry's own block: within the 300 characters
    // after one of the name's occurrences.
    const cited = [...source.matchAll(new RegExp(`'${name}'`, 'g'))].some((m) =>
      source.slice(m.index ?? 0, (m.index ?? 0) + 300).includes(citation),
    )
    expect(cited, `the ${name} entry cites ${citation}`).toBe(true)
  }
  const { Icon } = (await import(ICON_MODULE)) as unknown as {
    Icon: (props: { name: string }) => ReactElement
  }
  for (const name of ['computer', 'box']) {
    const { container, unmount } = render(createElement(Icon, { name }))
    const svg = container.querySelector('svg')
    expect(svg, `${name}: the component renders an svg`).not.toBeNull()
    expect(
      shapeOf(svg!),
      `${name}: the shape of lucide-static 1.74.0 icons/${name}.svg`,
    ).toEqual(LUCIDE[name]!)
    unmount()
  }
})

// ── 20 N.12: signed out, and the Overview h2 ────────────────────────────────

test('20 N.12: signed out there is no nav, so no Nodes label, no host entry and no menu; a pending session reads it the same way', async () => {
  const bff = fakeBff({
    version: VERSION_HOST,
    instances: TWO,
    session: 'signed-out',
  })
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('signin-title') })
  expect(screen.queryByRole('navigation'), 'no nav signed out').toBeNull()
  expect(byId('nav-nodes'), 'no Nodes label').toBeNull()
  expect(byId('nav-host'), 'no host entry').toBeNull()
  expect(document.getElementById('menu-button'), 'no Menu button').toBeNull()
  expect(bff.to('GET', '/api/version').length, 'and no host read').toBe(0)
  cleanup()
  vi.unstubAllGlobals()
  // The session on its way: the shell has no user to build a nav for.
  const h = held()
  fakeBff({ version: VERSION_HOST, instances: TWO, session: h.answer })
  render(<App />)
  expect(
    screen.queryByRole('navigation'),
    'no nav while the session is read',
  ).toBeNull()
  await act(async () => {
    h.resolve(reply(200, { user: 'alice' }))
  })
  const nav = await navOf()
  expect(
    words(byId('nav-nodes')),
    'the signed-in shell has the Nodes label',
  ).toBe(en('nav-nodes'))
  expect(byId('nav-host'), 'and the host entry').not.toBeNull()
  // 16 17f.2: the entry is the name alone, and this item has no label and no
  // port, so the name is the id.
  await within(nav).findByRole('link', { name: 'default' })
})

test('20 N.12: the h2 over the Overview tiles still reads the host name, and instances-title when the read answers with none', async () => {
  fakeBff({ version: VERSION_HOST, instances: TWO })
  render(<App />)
  expect(
    (await screen.findByRole('heading', { level: 2, name: VERSION_HOST.name }))
      .textContent,
    'the host name the shell read',
  ).toBe(VERSION_HOST.name)
  cleanup()
  vi.unstubAllGlobals()
  fakeBff({ version: VERSION, instances: TWO })
  render(<App />)
  expect(
    (
      await screen.findByRole('heading', {
        level: 2,
        name: en('instances-title'),
      })
    ).textContent,
    'a reply with no name keeps the title',
  ).toBe(en('instances-title'))
})
