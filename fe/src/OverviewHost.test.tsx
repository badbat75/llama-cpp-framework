// fe/src/OverviewHost.test.tsx
// Slice 13's host name on the Overview (fe/briefs/13-node-about.md, 13 N.5 and
// 13 N.12): the `h2` over the tiles says which machine this is, keeps today's
// words when the shell has no name, and comes with no new class. "13 N.5" is
// the brief's criterion 5.
//
// Slice 18 moved where the shell reads the host (fe/briefs/18-about-node-info.md,
// 18 AN.15: GET /api/version); what the page shows stays 13 N.5's.
//
// The sheet's class freeze and its overflow-wrap rules are pinned by
// Reflow.test.ts (10 R1.8, 10 R1.1, and 12 T1 layout for the tile's facts).
// What is pinned here is the shape of the name in the DOM: plain text in a
// block, so the body's inherited break reaches a name with no break point.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  VERSION,
  VERSION_HOST,
  en,
  fakeBff,
  reply,
  startAt,
} from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

/** The words of today, the fallback of the nav label and of this h2. */
const FALLBACK = en('instances-title')

/**
 * The signed-in app on the Overview. The shell's host read is GET /api/version
 * (18 AN.15), answered by `version`.
 */
async function open(over: Parameters<typeof fakeBff>[0] = {}) {
  const bff = fakeBff({ version: VERSION_HOST, ...over })
  render(<App />)
  const main = screen.getByRole('main')
  await screen.findByRole('button', { name: en('session-sign-out') })
  // The list is on screen: the tiles are part of "unchanged" in 13 N.5.
  await within(main).findAllByRole('listitem')
  return { bff, main }
}

/** The Overview's h2 over the tiles, once it reads `name`. */
function heading(main: HTMLElement, name: string): Promise<HTMLElement> {
  return within(main).findByRole('heading', { level: 2, name })
}

/** The h2s of the whole document, in order, as text. */
function h2s(): string[] {
  return Array.from(document.querySelectorAll('h2')).map((h) =>
    (h.textContent ?? '').trim(),
  )
}

// ── 13 N.5: the name over the tiles ─────────────────────────────────────────

test("13 N.5: with the shell having read the host, the Overview's h2 over the tiles is the host name alone, with no extra element and no class", async () => {
  const { main } = await open()
  const h2 = await heading(main, VERSION_HOST.name)
  expect(h2.children.length, 'plain text: no span, no icon').toBe(0)
  expect(h2.className, 'no class, so no new look').toBe('')
  expect(
    h2s(),
    'the instances section is the only h2 of the page, today and after',
  ).toEqual([VERSION_HOST.name])
  expect(
    within(main).queryByText(FALLBACK),
    'the words of today are gone once the name is read',
  ).toBeNull()
  // The h1 and the tiles are untouched: 13 N.5 asks for the same page.
  expect(
    within(main).getByRole('heading', { level: 1, name: en('overview-title') }),
    'the h1 is the Overview',
  ).toBeTruthy()
  expect(within(main).getAllByRole('listitem').length, 'both tiles').toBe(2)
})

test('13 N.5: the name shown is the one of the reply the shell read', async () => {
  // A fresh reply per read: the Overview's version line (13 N.10) reads the host
  // too (18 AN.15), and a Response body is good once (test/bff.ts).
  const { main } = await open({
    version: () => reply(200, { ...VERSION, name: 'router-3' }),
  })
  const h2 = await heading(main, 'router-3')
  expect(h2.textContent?.trim(), 'the host of this reply').toBe('router-3')
})

test("13 N.5: with the name read failed, the same h2 reads Instances, today's words, and the tiles are there", async () => {
  const { main } = await open({
    version: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  const h2 = await heading(main, FALLBACK)
  expect(h2.className, 'the fallback carries no class').toBe('')
  expect(
    within(main).getAllByRole('listitem').length,
    'a failed host read costs a word, never the list',
  ).toBe(2)
})

test('13 N.5: an empty name, a null name and a name that is not a string all show the fallback, never a blank heading', async () => {
  for (const [what, name] of [
    ['the empty string', ''],
    ['only spaces', '   '],
    ['null', null],
    ['a number', 7],
    ['an object', { a: 1 }],
  ] as Array<[string, unknown]>) {
    cleanup()
    vi.unstubAllGlobals()
    const { main } = await open({
      version: () => reply(200, { ...VERSION, name }),
    })
    const h2 = await heading(main, FALLBACK)
    expect(
      (h2.textContent ?? '').trim(),
      `${what}: the fallback, not an empty h2`,
    ).toBe(FALLBACK)
    expect(
      within(main).getByRole('heading', {
        level: 1,
        name: en('overview-title'),
      }),
      `${what}: the page is still the Overview`,
    ).toBeTruthy()
  }
})

test('13 N.5: the Overview adds no read for the name: the shell one (18 AN.15) and the version line of 13 N.10 are the only GET /api/version, and nothing on the name asks again', async () => {
  const { bff, main } = await open()
  await heading(main, VERSION_HOST.name)
  expect(
    bff.to('GET', '/api/version').length,
    'the shell read of the host, plus the version line of 13 N.10: the h2 asks for nothing of its own',
  ).toBe(2)
  fireEvent.click(screen.getByRole('button', { name: en('session-sign-out') }))
  expect(bff.to('GET', '/api/version').length, 'sign-out reads nothing').toBe(2)
})

// ── 13 N.12: nothing else changed ───────────────────────────────────────────

test('13 N.12: a name with no break point is plain text in the h2, with no class of its own for the sheet to grow', async () => {
  const long = 'a-host-name-with-no-break-point'.repeat(9)
  expect(long.length, 'the 253 characters of the brief').toBeGreaterThan(250)
  const { main } = await open({
    // A fresh reply per read: the shell and the Overview's version line both
    // read the host (18 AN.15), and a body is good once.
    version: () => reply(200, { ...VERSION, name: long }),
  })
  const h2 = await heading(main, long)
  expect(h2.className, 'no class of its own').toBe('')
  expect(h2.children.length, 'no element inside it').toBe(0)
  const classes = new Set<string>()
  for (const el of Array.from(document.querySelectorAll('#root *')))
    for (const c of Array.from(el.classList)) classes.add(c)
  expect(
    [...classes].filter((c) => c.startsWith('node') || c.startsWith('host')),
    'the slice adds no class: the name rides on the sheet of today (10 R1.8)',
  ).toEqual([])
})

test("13 N.12: the tiles are today's tiles beside the name: the facts list stays the last child of each tile before its run row (23 A1.1)", async () => {
  const { main } = await open({
    instances: [
      {
        id: 'default',
        engine: 'llama.cpp',
        state: 'active',
        port: 8080,
        boot: true,
      },
      {
        id: 'coder',
        engine: 'llama.cpp',
        state: 'inactive',
        port: 8081,
        boot: false,
      },
    ],
  })
  const tiles = within(main).getAllByRole('listitem')
  expect(tiles.length, 'both instances').toBe(2)
  for (const tile of tiles) {
    const facts = tile.querySelector('dl')
    expect(facts, `${tile.textContent}: the facts list`).not.toBeNull()
    // 23 A1.1: the tile ends with its run row, so the facts list is the
    // tile's last child before it.
    const row = tile.lastElementChild as HTMLElement
    expect(
      within(row).getAllByRole('button'),
      `${tile.textContent}: the tile ends with its run row (23 A1.1)`,
    ).toHaveLength(3)
    expect(
      row.previousElementSibling,
      `${tile.textContent}: the facts are the last child before the run row`,
    ).toBe(facts)
    expect(
      facts?.classList.contains('tile-facts'),
      'the facts list of today, its own class',
    ).toBe(true)
  }
})
