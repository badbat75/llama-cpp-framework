// Hearth H6a (fe/briefs/08-hearth-h6-overview.md, section 1): the Overview's
// instances become a list of tiles. Every test is numbered as the criterion it
// pins; the CSS of H6a.5 to H6a.8 is pinned as stylesheet text in
// HearthH6a.test.ts.
//
// The names of the parts are the brief's: `tiles` (the `ul`), `tile` (the `li`),
// `tile-head`, `tile-id`, `badge`, `tile-engine`. The shape span inside the
// badge keeps `shape shape-*` (`StateBadge.tsx`): the rules the stylesheet has
// for it are the same rules, so H6a does not rename it.
//
// jsdom measures no layout, so the geometry the brief claims (one column at
// 320 px, no sideways scroll, the badge wrapping under a long id, the 28 px
// target, the shape contrast) belongs to its section 4 UX pass. What jsdom can
// pin of those claims is pinned here: the list is one `ul` of `li` tiles, a
// tile is one box holding all its parts, the badge sits in the head row where
// it can wrap, and the shape span holds no text.
import { cleanup, render, screen, within } from '@testing-library/react'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  INSTANCES,
  deferred,
  en,
  fakeBff,
  reply,
  startAt,
  type Instance,
} from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

/** The Instances section of the Overview (its `h2` is `instances-title`).
 * The session read settles first: right after the mount `main` holds only
 * the loading `p`, so the heading is awaited (the suite's pattern). */
async function instancesSection(): Promise<HTMLElement> {
  const heading = await screen.findByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const section = heading.closest('section')
  expect(section).toBeTruthy()
  return section as HTMLElement
}

/** The tile items of the Overview's instances list. */
async function tiles(): Promise<HTMLElement[]> {
  return within(await instancesSection()).findAllByRole('listitem')
}

/** The class names of an element. */
function classes(element: Element): string[] {
  return element.className.split(/\s+/)
}

/** Every element of a table below `root`, counted through the DOM tree. */
function tableParts(root: Element): Element[] {
  return Array.from(
    root.querySelectorAll(
      'table, thead, tbody, tfoot, tr, th, td, caption, col, colgroup',
    ),
  )
}

/** One instance with the `state`. */
function oneState(state: string): Instance[] {
  return [{ id: 'default', engine: 'llama.cpp', state }]
}

// ── H6a.1: the list ────────────────────────────────────────────────────────

test('H6a.1: the instances are a ul.tiles of li.tile, in the order received, with no table left', async () => {
  fakeBff()
  render(<App />)

  const section = await instancesSection()
  const list = await within(section).findByRole('list')
  expect(list.tagName).toBe('UL')
  expect(classes(list)).toContain('tiles')
  // the semantics are spelled out, for the engine that drops those of a bare
  // ul (the follow-up of H6, section 4)
  expect(list.getAttribute('role')).toBe('list')

  // one item per instance, in the order the bff answered
  const items = within(list).getAllByRole('listitem')
  expect(items.length).toBe(INSTANCES.length)
  for (const item of items) expect(classes(item)).toContain('tile')
  expect(
    items.map((item) => item.querySelector('.tile-id')?.textContent),
  ).toEqual(INSTANCES.map((instance) => instance.id))

  // the grid is gone: no cell of a table is left in the section
  expect(tableParts(section)).toEqual([])
})

// The follow-up of H6 (08-hearth-h6-overview.md, section 4): WebKit drops the
// list semantics of a `ul` whose `list-style` is none, on purpose, and advises
// an explicit `role="list"` (WebKit bug 170179, comment 1). Playwright 1.65 has
// no WebKit accessibility tree to measure it with, so the attribute is the pin.
test('H6 follow-up: ul.tiles carries role="list", so the count of instances is a list in every engine', async () => {
  fakeBff()
  render(<App />)

  const list = await within(await instancesSection()).findByRole('list')
  expect(list.tagName).toBe('UL')
  expect(classes(list)).toContain('tiles')
  expect(list.getAttribute('role')).toBe('list')
  // the role of the list does not touch those of its items
  const items = within(list).getAllByRole('listitem')
  expect(items.length).toBe(INSTANCES.length)
  for (const item of items) expect(item.getAttribute('role')).toBeNull()
})

// ── H6a.2: the tile ────────────────────────────────────────────────────────

test('H6a.2: a tile is the tile-head row with the id link and the badge, and nothing else', async () => {
  // Signed out, so the boot row of H6b never joins the tile: this test counts
  // the tile's children as they stand, and the row arrives with the session
  // read, which would race the asserts. The tile with the row is pinned by
  // HearthTilesBoot.test.tsx.
  fakeBff({ session: 'signed-out' })
  render(<App />)

  const [tile] = await tiles()
  expect(tile).toBeTruthy()

  // 1. `div.tile-head`, holding `a.tile-id` first then `span.badge`
  const head = tile.querySelector('.tile-head')
  expect(head).toBeTruthy()
  expect(head!.children.length).toBe(2)

  const link = within(tile).getByRole('link', { name: 'default' })
  expect(classes(link)).toContain('tile-id')
  expect(link.getAttribute('href')).toBe('#/instances/default')
  // the link's name is exactly the id, with nothing inside it
  expect(link.textContent).toBe('default')
  expect(link.children.length).toBe(0)
  expect(head!.firstElementChild).toBe(link)

  const badge = link.nextElementSibling
  expect(badge).toBeTruthy()
  expect(badge!.tagName).toBe('SPAN')
  expect(classes(badge!)).toContain('badge')
  expect(badge!.textContent).toBe(en('instance-state-active'))
  // the badge's text is exactly the state label, and the shape inside it is a
  // mark that says nothing: the text carries the state (1.4.1)
  const shape = badge!.firstElementChild
  expect(shape).toBeTruthy()
  expect(classes(shape!)).toContain('shape')
  expect(shape!.getAttribute('aria-hidden')).toBe('true')
  expect(shape!.textContent).toBe('')
  // H6a.2 fixes the order inside the badge: the shape, then the label. A text
  // node before the shape is the label read before its mark, and the first
  // child is a text node, not the shape.
  expect(badge!.firstChild, 'the badge is the shape, then the label text').toBe(
    shape,
  )

  // the tile is one box: the head, and nothing else with one engine
  expect(tile.children.length).toBe(1)
  expect(tile.querySelector('.tile-engine')).toBeNull()
  expect(tableParts(tile)).toEqual([])
})

const engineCases = [
  { engines: ['llama.cpp', 'llama.cpp'], shown: [] as string[] },
  { engines: ['llama.cpp', 'Strata'], shown: ['llama.cpp', 'Strata'] },
  {
    engines: ['llama.cpp', 'Strata', 'vllm'],
    shown: ['llama.cpp', 'Strata', 'vllm'],
  },
]
test.each(engineCases)(
  'H6a.2: the engine line is there only when the reply holds two distinct engines: $engines',
  async ({ engines, shown }) => {
    // one instance per engine: the fixture must show every engine listed.
    // Signed out (retargeted for H6b): the boot row is the signed-in page's
    // (H6b.1), so here the tile is exactly the DOM H6a left, and the engine
    // line is its last child; the signed-in tile with the row is pinned by
    // HearthTilesBoot.test.tsx (H6b.3: the row is the tile's last child).
    fakeBff({
      session: 'signed-out',
      instances: engines.map((engine, n) => ({
        id: `i${n}`,
        engine,
        state: n === 0 ? 'active' : 'inactive',
      })),
    })
    render(<App />)

    const items = await tiles()
    const lines = Array.from(
      (await instancesSection()).querySelectorAll('.tile-engine'),
    ).map((line) => line.textContent)
    expect(lines).toEqual(shown.map((engine) => en('instance-engine', engine)))
    for (const [n, engine] of shown.entries()) {
      const item = items[n]!
      // a `p` of the tile, its last child, naming the engine
      expect(item.querySelector('.tile-engine')?.tagName).toBe('P')
      expect(item.lastElementChild).toBe(item.querySelector('.tile-engine'))
      expect(within(item).getByText(en('instance-engine', engine))).toBeTruthy()
    }
  },
)

const stateCases = [
  { state: 'active', label: 'instance-state-active' },
  { state: 'inactive', label: 'instance-state-inactive' },
  { state: 'activating', label: 'instance-state-activating' },
  { state: 'deactivating', label: 'instance-state-deactivating' },
  { state: 'failed', label: 'instance-state-failed' },
  { state: 'maintenance', label: 'other' },
]
test.each(stateCases)(
  'H6a.2: the badge of a $state instance says exactly the state label',
  async ({ state, label }) => {
    const text =
      label === 'other' ? en('instance-state-other', state) : en(label)
    fakeBff({ instances: oneState(state) })
    render(<App />)

    const [tile] = await tiles()
    const badge = tile.querySelector('.badge')
    expect(badge).toBeTruthy()
    expect(badge!.textContent).toBe(text)
    // the label is the text of the tile, so it is read (4.1.2)
    expect(within(tile).getByText(text)).toBeTruthy()
  },
)

// ── H6a.3: the states the list does not have ───────────────────────────────

test('H6a.3: while the read runs, nothing is under the heading and there is no list', async () => {
  const gate = deferred()
  fakeBff({ instances: () => gate.promise })
  render(<App />)

  const section = await instancesSection()
  // as today: an empty section, no place held, no message
  await Promise.resolve()
  expect(section.textContent).toBe(en('instances-title'))
  expect(within(section).queryByRole('list')).toBeNull()
  expect(within(section).queryAllByRole('paragraph')).toEqual([])
  gate.resolve(reply(200, INSTANCES))
})

test('H6a.3: no instances: the p instances-empty in place of the list, with no table', async () => {
  fakeBff({ instances: [] })
  render(<App />)

  const section = await instancesSection()
  const paragraphs = await within(section).findAllByRole('paragraph')
  expect(paragraphs.map((p) => p.textContent)).toEqual([en('instances-empty')])
  expect(within(section).queryByRole('list')).toBeNull()
  expect(tableParts(section)).toEqual([])
})

const errorCases = [
  {
    name: 'a read that cannot reach the service',
    answer: () => Promise.reject(new TypeError('Failed to fetch')),
    text: en('instances-failed'),
  },
  {
    name: '503 client-unavailable',
    answer: () => reply(503, { key: 'client-unavailable', args: [] }),
    text: en('instances-manager-unavailable'),
  },
  {
    name: '502 client-not-authorized',
    answer: () => reply(502, { key: 'client-not-authorized', args: [] }),
    text: en('instances-not-authorized'),
  },
  {
    name: 'a body that is not a list of instances',
    answer: () => reply(200, '{"instances":[]}'),
    text: en('instances-failed'),
  },
]
test.each(errorCases)(
  'H6a.3: $name: the one p of its message, no list, no table',
  async ({ answer, text }) => {
    fakeBff({ instances: answer })
    render(<App />)

    const section = await instancesSection()
    const paragraphs = await within(section).findAllByRole('paragraph')
    expect(paragraphs.length).toBe(1)
    expect(paragraphs[0]!.textContent).toBe(text)
    expect(within(section).queryByRole('list')).toBeNull()
    expect(tableParts(section)).toEqual([])
  },
)

// ── H6a.4: the strings ─────────────────────────────────────────────────────

test('H6a.4 with H6b.5: the three table-header keys are gone, only the two boot words are added, the reused keys stay', () => {
  const keys = Object.keys(strings)

  // the tile says the same things the table said, with the keys the table had:
  // the section, its messages, the engine line and the state labels. H6b adds
  // exactly two keys, the words of the boot row (H6b.5), no other.
  const overview = keys.filter((key) => key.startsWith('instances-')).sort()
  expect(overview).toEqual(
    [
      'instances-boot-off',
      'instances-boot-on',
      'instances-empty',
      'instances-failed',
      'instances-manager-unavailable',
      'instances-not-authorized',
      'instances-title',
    ].sort(),
  )
  for (const key of [
    'instance-engine',
    'instance-state-active',
    'instance-state-inactive',
    'instance-state-activating',
    'instance-state-deactivating',
    'instance-state-failed',
    'instance-state-other',
  ])
    expect(keys, key).toContain(key)

  // the shape is a class, never a word the user reads
  for (const key of keys) {
    const name = key.toLowerCase()
    expect(name, key).not.toContain('tile')
    expect(name, key).not.toContain('badge')
  }
  for (const [key, text] of Object.entries(strings))
    expect(text.toLowerCase(), key).not.toContain('tile')

  // the Rust strings file the GUI shares: no word of the shape either
  const gui = readFileSync(
    join(import.meta.dirname, '../../crates/core/strings/en_US.toml'),
    'utf8',
  ).toLowerCase()
  expect(gui).not.toContain('tile')
  expect(gui).not.toContain('badge')
})

// ── H6a.9: nothing else moves ──────────────────────────────────────────────

test('H6a.9: the h1 and its focus target, the version line, the nav links and the one read are as they were', async () => {
  const bff = fakeBff()
  render(<App />)

  await within(await instancesSection()).findByRole('list')

  const h1 = screen.getByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(h1.getAttribute('id')).toBe('page-heading')
  expect(h1.getAttribute('tabindex')).toBe('-1')
  const version = screen.getByText(en('version-label', '2.99.0'))
  expect(version.tagName).toBe('P')
  expect(
    h1.compareDocumentPosition(version) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()

  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  for (const instance of INSTANCES)
    expect(within(nav).getByRole('link', { name: instance.id })).toBeTruthy()

  // the list is read once, as the table was: no poll, no second read
  expect(bff.to('GET', '/api/instances').length).toBe(1)
})
