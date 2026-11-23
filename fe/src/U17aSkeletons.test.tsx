// Brief 17, slice U17a (fe/briefs/17-ux-pass.md): the skeleton while a page's
// first read is pending. The rule (fe/DESIGN.md 5.2 Skeleton, and its section 6
// Loading row) has two halves: the page's loading text is a `role="status"`
// rendered at once, now inside a `.visually-hidden` element, and 300 ms after
// the read starts the page's shape shows as a skeleton, `aria-hidden="true"`,
// that animates nowhere. A fast answer therefore shows no flash and a slow one
// shows the layout to come.
//
// The whole app renders against the fake bff (mocked fetch) on fake timers, as
// Boot.test.tsx does, so the 300 ms is the test's own clock and the answer
// lands when the test says so. Every page's skeleton shape is the brief's
// table. What the check.js UX pass adds is the same two states in a browser.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { act, cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  INSTANCES,
  NODE,
  deferred,
  en,
  fakeBff,
  reply,
  startAt,
  VERSION_HOST,
  type BffOptions,
} from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Fake timers for the 300 ms; setImmediate and microtasks stay real. */
function fakeClock() {
  vi.useFakeTimers({
    toFake: [
      'setTimeout',
      'clearTimeout',
      'setInterval',
      'clearInterval',
      'Date',
    ],
  })
}

/** Lets every answer and render that is due now land; the clock stays. */
async function flush() {
  await act(async () => {
    for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** The app at `hash` on the fake clock, with `options` behind fetch. */
async function open(options: BffOptions, hash: string) {
  fakeClock()
  startAt(hash)
  fakeBff(options)
  render(<App />)
  await flush()
  return { main: screen.getByRole('main') }
}

/** A read that never answers: the page stays pending, so the skeleton stays. */
const pending = () => deferred().promise

/**
 * The read's answer, late by `ms`: it stays pending past the 300 ms, so the
 * skeleton is up when it lands, and it lands on the fake clock (the reject of
 * a read that cannot reach the service included).
 */
function late(answer: () => Response | Promise<Response>, ms = 500) {
  return () =>
    new Promise<Response>((resolve, reject) => {
      setTimeout(() => {
        void Promise.resolve(answer()).then(resolve, reject)
      }, ms)
    })
}

/** The skeleton's line blocks: `.skeleton` alone, a box being its own class. */
const lines = (root: Element) =>
  root.querySelectorAll('.skeleton:not(.skeleton-box)')

/** The skeleton's box blocks. */
const boxes = (root: Element) => root.querySelectorAll('.skeleton-box')

/** Every skeleton block under `root`, of either kind. */
const blocks = (root: Element) =>
  root.querySelectorAll('.skeleton, .skeleton-box')

/** The `role="status"` in `main` that reads exactly `text`, when there is one.
 * A queryAll, not a getAll: the 17a.5 absence pins call this after the answer,
 * when the page may hold no status at all, and a getAll throws on that page. */
function statusReading(main: HTMLElement, text: string): Element | undefined {
  return within(main)
    .queryAllByRole('status')
    .find((el) => el.textContent?.trim() === text)
}

/**
 * The page's loading status: a `role="status"`, holding the text, inside a
 * `.visually-hidden` element (17a.2). Both nestings answer: the hidden element
 * wrapped the status, or the status wrapped it.
 *
 * A `role="status"` is not a name-from-content role, so the text is matched
 * where it is written, on the status element.
 */
function expectStatus(main: HTMLElement, text: string) {
  const status = statusReading(main, text)
  expect(
    status,
    `the page has a role="status" reading "${text}"`,
  ).not.toBeUndefined()
  const hidden =
    status!.closest('.visually-hidden') ??
    status!.querySelector('.visually-hidden')
  expect(
    hidden,
    `"${text}" is in the DOM and not shown: inside a .visually-hidden element`,
  ).not.toBeNull()
  expect(
    hidden!.textContent?.trim(),
    'the hidden text is the loading words',
  ).toBe(text)
  return status
}

/** The elements a Tab can stop on. */
const isTabStop = (el: Element): boolean =>
  el.matches('a[href], button, input, select, textarea, [tabindex]') &&
  el.getAttribute('tabindex') !== '-1'

/** Every element under an `aria-hidden="true"` root of `main`, the skeleton's
 * own subtree (17a.6). */
function underHidden(main: Element): Element[] {
  const nodes: Element[] = []
  for (const root of main.querySelectorAll('[aria-hidden="true"]'))
    nodes.push(...root.querySelectorAll('*'))
  return nodes
}

/** The Instances section of the Overview, by its `h2`. */
function instancesSection(main: HTMLElement): Element {
  const heading = within(main).getByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  return heading.closest('section') ?? main
}

// ── 17a.1: the new string ───────────────────────────────────────────────

test('17a.1: en_US.json has instances-loading with the English of the brief', () => {
  const table: Record<string, string> = strings
  expect(
    table['instances-loading'],
    'the Overview has a loading text of its own, as every other page has',
  ).toBe('Loading the instances…')
})

// ── 17a.2: the Overview, pending ────────────────────────────────────────

test('17a.2: the Overview with the instances read pending: the status text, visually hidden, and no tile', async () => {
  const gate = deferred()
  const { main } = await open({ instances: () => gate.promise }, '#/')

  expectStatus(main, en('instances-loading'))
  // The key is the Overview's own, not the shell's: the page says what it is
  // reading. And no tile stands where the answer will.
  expect(main.querySelectorAll('.tile').length, 'no tile').toBe(0)
  expect(main.querySelectorAll('.badge').length, 'no state badge').toBe(0)
  gate.resolve(reply(200, INSTANCES))
})

// ── 17a.3: the 300 ms, and the Overview's shape ─────────────────────────

test('17a.3: the Overview shows no skeleton before 300 ms, and two tiles of three lines at 300 ms, all aria-hidden', async () => {
  const gate = deferred()
  const { main } = await open({ instances: () => gate.promise }, '#/')
  expectStatus(main, en('instances-loading'))

  await advance(299)
  expect(
    blocks(main).length,
    'a fast answer shows no flash: 299 ms is not enough',
  ).toBe(0)

  await advance(1)
  const section = instancesSection(main)
  const tiles = section.querySelectorAll('.tile')
  expect(tiles.length, 'the tiles are there to come').toBe(2)
  for (const tile of tiles) {
    expect(
      tile.querySelectorAll('.skeleton').length,
      'each tile is held by three lines',
    ).toBe(3)
  }
  expect(blocks(section).length, 'two tiles of three lines').toBe(6)
  for (const el of blocks(section)) {
    expect(
      el.closest('[aria-hidden="true"]'),
      'a skeleton is never read: it is under an aria-hidden ancestor',
    ).not.toBeNull()
  }
  gate.resolve(reply(200, INSTANCES))
})

// ── 17a.4: the same rule on every page ──────────────────────────────────

test('17a.4 boot: the session read pending shows the kept loading text hidden, then one line and two boxes in main', async () => {
  const session = deferred()
  const { main } = await open({ session: () => session.promise }, '#/')
  expectStatus(main, en('loading'))

  await advance(299)
  expect(blocks(main).length, 'no skeleton at 299 ms').toBe(0)
  await advance(1)
  expect(lines(main).length, 'one line, the page heading').toBe(1)
  expect(boxes(main).length, 'two boxes').toBe(2)
  for (const el of blocks(main))
    expect(el.closest('[aria-hidden="true"]'), 'aria-hidden').not.toBeNull()
  session.resolve(reply(200, { user: 'alice' }))
})

test('17a.4 instance page: the list read pending shows the header loading text hidden, then three lines', async () => {
  const gate = deferred()
  const { main } = await open(
    { instances: () => gate.promise },
    '#/instances/default',
  )
  expectStatus(main, en('loading'))

  await advance(299)
  expect(blocks(main).length, 'no skeleton at 299 ms').toBe(0)
  await advance(1)
  expect(lines(main).length, 'the header is three lines').toBe(3)
  // Brief 16 17a.10: the Server form left this page, so its card is gone from
  // the shape; three lines are all the page has to come.
  expect(
    main.querySelectorAll('.card').length,
    'no card: no form is coming',
  ).toBe(0)
  expect(boxes(main).length, 'no box').toBe(0)
  gate.resolve(reply(200, INSTANCES))
})

test('16 17a.6 server profile page: the read pending shows its kept text hidden, then one line and the box in its card', async () => {
  const server = deferred()
  const { main } = await open(
    { serverProfile: () => server.promise },
    '#/profiles/server/default',
  )
  expectStatus(main, en('server-loading'))

  await advance(299)
  expect(blocks(main).length, 'no skeleton at 299 ms').toBe(0)
  await advance(1)
  expect(
    lines(main).length,
    'one line, the Used by row the document brings',
  ).toBe(1)
  expect(boxes(main).length, 'the form is held by one box').toBe(1)
  expect(
    boxes(main)[0]!.closest('.card'),
    'the box stands in the card it is holding',
  ).not.toBeNull()
  server.resolve(reply(200, {}))
})

test('16 17a.3 server profiles: the list read pending shows its kept text hidden, then three lines in the table place', async () => {
  const gate = deferred()
  const { main } = await open(
    { serverProfiles: () => gate.promise },
    '#/profiles/server',
  )
  expectStatus(main, en('server-profiles-loading'))

  await advance(299)
  expect(blocks(main).length, 'no skeleton at 299 ms').toBe(0)
  await advance(1)
  expect(lines(main).length, 'three lines where the rows will be').toBe(3)
  expect(main.querySelectorAll('table').length, 'no table yet').toBe(0)
  for (const el of blocks(main))
    expect(el.closest('[aria-hidden="true"]'), 'aria-hidden').not.toBeNull()
  gate.resolve(reply(200, []))
})

test('17a.4 Model profiles: the list read pending shows its kept text hidden, then three lines in the table place', async () => {
  const gate = deferred()
  const { main } = await open(
    { modelProfiles: () => gate.promise },
    '#/profiles/model',
  )
  expectStatus(main, en('model-profiles-loading'))

  await advance(299)
  expect(blocks(main).length, 'no skeleton at 299 ms').toBe(0)
  await advance(1)
  expect(lines(main).length, 'three lines where the rows will be').toBe(3)
  expect(main.querySelectorAll('table').length, 'no table yet').toBe(0)
  for (const el of blocks(main))
    expect(el.closest('[aria-hidden="true"]'), 'aria-hidden').not.toBeNull()
  gate.resolve(reply(200, []))
})

test('17a.4 model profile page: the read pending shows its kept text hidden, then two cards each holding a box', async () => {
  const gate = deferred()
  const { main } = await open(
    { modelProfile: () => gate.promise },
    '#/profiles/model/Qwen3-27B',
  )
  expectStatus(main, en('model-profile-loading'))

  await advance(299)
  expect(blocks(main).length, 'no skeleton at 299 ms').toBe(0)
  await advance(1)
  const cards = main.querySelectorAll('.card')
  expect(cards.length, 'two cards').toBe(2)
  for (const card of cards)
    expect(
      card.querySelectorAll('.skeleton-box').length,
      'each card holds one box',
    ).toBe(1)
  gate.resolve(reply(200, {}))
})

test('17a.4 About: the read pending shows its kept text hidden, then two lines', async () => {
  const gate = deferred()
  const { main } = await open({ version: () => gate.promise }, '#/about')
  expectStatus(main, en('about-loading'))

  await advance(299)
  expect(blocks(main).length, 'no skeleton at 299 ms').toBe(0)
  await advance(1)
  expect(
    lines(main).length,
    'two lines to come: the framework and the operating system (18 AN.2)',
  ).toBe(2)
  expect(main.querySelectorAll('dl').length, 'no list yet').toBe(0)
  for (const el of blocks(main))
    expect(el.closest('[aria-hidden="true"]'), 'aria-hidden').not.toBeNull()
  gate.resolve(reply(200, VERSION_HOST))
})

test('17a.4 Node info: the read pending shows its kept text hidden, then six lines', async () => {
  const gate = deferred()
  const { main } = await open({ node: () => gate.promise }, '#/node')
  expectStatus(main, en('node-loading'))

  await advance(299)
  expect(blocks(main).length, 'no skeleton at 299 ms').toBe(0)
  await advance(1)
  expect(
    lines(main).length,
    'six lines to come: the four of the top list and the engine (18 AN.9)',
  ).toBe(6)
  expect(main.querySelectorAll('dl').length, 'no list yet').toBe(0)
  for (const el of blocks(main))
    expect(el.closest('[aria-hidden="true"]'), 'aria-hidden').not.toBeNull()
  gate.resolve(reply(200, NODE))
})

// ── 17a.5: the answer, of any kind ──────────────────────────────────────

const answers: { name: string; answer: () => Response | Promise<Response> }[] =
  [
    { name: 'the list', answer: () => reply(200, INSTANCES) },
    { name: 'no instance', answer: () => reply(200, []) },
    {
      name: '503 client-unavailable',
      answer: () => reply(503, { key: 'client-unavailable', args: [] }),
    },
    {
      name: '502 client-not-authorized',
      answer: () => reply(502, { key: 'client-not-authorized', args: [] }),
    },
    {
      name: 'a read that cannot reach the service',
      answer: () => Promise.reject(new TypeError('Failed to fetch')),
    },
  ]

for (const { name, answer } of answers) {
  test(`17a.5 Overview, ${name}: the status and the skeleton go in the same render, and the focus stays`, async () => {
    const { main } = await open({ instances: late(answer) }, '#/')
    await advance(300)
    expect(blocks(main).length, 'the skeleton is up').toBeGreaterThan(0)
    expectStatus(main, en('instances-loading'))
    const focused = document.activeElement

    // The answer lands: both halves of the loading state are gone together,
    // and nothing moved the focus.
    await advance(200)
    expect(blocks(main).length, 'the skeleton is gone with the answer').toBe(0)
    expect(
      statusReading(main, en('instances-loading')),
      'the loading text is gone with the answer',
    ).toBeUndefined()
    expect(
      document.activeElement,
      'the answer took the focus from nowhere',
    ).toBe(focused)
  })
}

for (const [name, answer] of [
  ['the versions', () => reply(200, VERSION_HOST)],
  ['the failure', () => reply(503, { key: 'client-unavailable', args: [] })],
] as [string, () => Response][]) {
  test(`17a.5 About, ${name}: the status and the skeleton go in the same render, and the focus stays`, async () => {
    const { main } = await open({ version: late(answer) }, '#/about')
    await advance(300)
    expect(blocks(main).length, 'the skeleton is up').toBeGreaterThan(0)
    expectStatus(main, en('about-loading'))
    const focused = document.activeElement

    await advance(200)
    expect(blocks(main).length, 'the skeleton is gone with the answer').toBe(0)
    expect(
      statusReading(main, en('about-loading')),
      'the loading text is gone with the answer',
    ).toBeUndefined()
    expect(document.activeElement, 'the focus is what it was').toBe(focused)
  })
}

// ── 17a.6: the skeleton is nothing to read or touch ─────────────────────

const skeletonPages: { name: string; hash: string; options: BffOptions }[] = [
  { name: 'the Overview', hash: '#/', options: { instances: pending } },
  {
    name: 'the instance page',
    hash: '#/instances/default',
    options: { instances: pending, server: pending },
  },
]

for (const { name, hash, options } of skeletonPages) {
  test(`17a.6 ${name}: the skeleton holds no tab stop and no text`, async () => {
    const { main } = await open(options, hash)
    await advance(300)
    expect(blocks(main).length, 'the skeleton is up').toBeGreaterThan(0)

    const under = underHidden(main)
    expect(
      under.length,
      `${name}: the skeleton is made of elements`,
    ).toBeGreaterThan(0)
    let text = ''
    for (const el of under) text += el.textContent ?? ''
    expect(text, `${name}: the skeleton carries no text`).toBe('')
    expect(
      under.filter(isTabStop),
      `${name}: nothing in the skeleton takes the focus`,
    ).toEqual([])
  })
}

// ── 17a.7: the stylesheet ───────────────────────────────────────────────

// The sheet read as text, the way Reflow.test.ts does (comments dropped, the
// same rule regex): the two classes are declarations, so no browser is needed.
// Reflow.test.ts exports no helper and importing it would register its tests
// here, so the reader is a small local copy.
const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

const RULES = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1]!.trim().replace(/\s+/g, ' '),
  // The selector with its whitespace dropped, to match one written over lines.
  compact: m[1]!.replace(/\s+/g, ''),
  body: m[2]!,
}))

/** True when `cls` is one whole selector of the rule's list. */
const selects = (rule: { compact: string }, cls: string) =>
  rule.compact.split(',').some((part) => part === `.${cls}`)

/** The last value `name` declares in one rule. */
function declared(rule: { body: string }, name: string): string {
  let val = ''
  for (const d of rule.body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name)
      val = d
        .slice(i + 1)
        .trim()
        .replace(/\s+/g, ' ')
  }
  return val
}

/** The value `name` ends up with for `cls`, across the rules that select it. */
function valueFor(cls: string, name: string): string {
  let val = ''
  for (const rule of RULES.filter((r) => selects(r, cls)))
    val = declared(rule, name) || val
  return val
}

/** The rules whose selector list holds the whole descendant selector `sel`,
 * given in the compact form the reader really produces: `m[1]` keeps the
 * leading dot, so `.card .skeleton` compacts to `.card.skeleton`. */
const descendantRules = (sel: string) =>
  RULES.filter((r) => r.compact.split(',').includes(sel))

/** The compact form (`.card.skeleton`) back into the selector it stands for
 * (`.card .skeleton`): every dot of the compact form is one descendant step. */
const readable = (compact: string) => compact.split('.').join(' .').trim()

// The fill, as the designer re-ruled it in the U17a review (fe/briefs/17-ux-pass.md
// and fe/DESIGN.md 5.2 Skeleton): `--surface` on the page's `--page` is all but
// invisible in light mode, so the block takes `--bg` there, and `--surface`
// inside a lifted box (`.card`, `.tile`), whose fill is `--bg`.
test('17a.7 (as the U17a review re-ruled the fill): .skeleton and .skeleton-box declare the brief properties on the page, the lines gap, and neither animates', () => {
  for (const cls of ['skeleton', 'skeleton-box']) {
    expect(
      RULES.filter((r) => selects(r, cls)).length,
      `.${cls} is in the sheet`,
    ).toBeGreaterThan(0)
    for (const [name, value] of [
      ['display', 'block'],
      ['max-width', '100%'],
      ['background', 'var(--bg)'],
      ['border', 'var(--border-w) solid transparent'],
      ['border-radius', 'var(--radius)'],
    ]) {
      expect(valueFor(cls, name), `.${cls} ${name}`).toBe(value)
    }
  }
  // The heights follow the text size (WCAG 1.4.4): a line of text, and a box.
  expect(valueFor('skeleton', 'height'), '.skeleton height').toBe('1lh')
  expect(valueFor('skeleton-box', 'height'), '.skeleton-box height').toBe(
    '8rem',
  )

  // Two lines in a row are one --space-2 apart.
  const gap = RULES.filter((r) =>
    r.compact.split(',').some((part) => part === '.skeleton+.skeleton'),
  )
  expect(gap.length, '.skeleton + .skeleton is in the sheet').toBe(1)
  expect(
    declared(gap[0]!, 'margin-top'),
    'two lines in a row are one --space-2 apart',
  ).toBe('var(--space-2)')

  // The blocks do not animate, and no rule that selects them starts a motion
  // (U17g fades the content that replaces them, not them).
  for (const rule of RULES.filter((r) => r.compact.includes('.skeleton'))) {
    for (const name of [
      'animation',
      'animation-name',
      'transition',
      'transition-property',
    ]) {
      expect(
        declared(rule, name),
        `${rule.sel} declares no ${name}: a skeleton does not animate (17a.7)`,
      ).toBe('')
    }
  }
})

// The other half of the fill ruling: the lifted boxes are `--bg`, so inside one
// the block takes the other token, `--surface`. The three selectors the review
// spells out, each met by a rule at the top level (the fill is the same at
// every width).
test('17a.7 (the U17a review): inside a .card or a .tile the skeleton fills with --surface', () => {
  for (const sel of [
    '.card.skeleton',
    '.card.skeleton-box',
    '.tile.skeleton',
  ]) {
    const rules = descendantRules(sel)
    expect(
      rules.length,
      `${readable(sel)} is in the sheet: the block is the card's colour inside a card`,
    ).toBeGreaterThan(0)
    for (const rule of rules) {
      expect(
        declared(rule, 'background'),
        `${readable(sel)}: --surface, the fill of the page it stands on`,
      ).toBe('var(--surface)')
      expect(
        rule.compact.includes('@'),
        `${readable(sel)}: written at the top level, not inside a media block`,
      ).toBe(false)
    }
  }
})

// The helper's own teeth: the pin above passes because the sheet has the
// selector, not because the reader matches anything. A selector that is not
// there finds no rule, and `readable` names what the compact form stands for,
// so a red above says which selector is missing.
test('17a.7 (the helper has teeth): a descendant selector that is not in the sheet finds no rule', () => {
  for (const sel of [
    '.card.not-a-class',
    '.tile.skeleton-box',
    '.page.skeleton',
  ]) {
    expect(
      descendantRules(sel).length,
      `${readable(sel)} is not in the sheet, and the helper says so`,
    ).toBe(0)
  }
  expect(
    readable('.card.skeleton'),
    'the compact form reads back as the selector',
  ).toBe('.card .skeleton')
  expect(readable('.skeleton'), 'a top-level class reads back too').toBe(
    '.skeleton',
  )
})

// ── 17a.3 (the U17a review, the tiles): the grid the answer replaces ──────

// The answer's tiles are a `.tiles` grid of 15rem tracks. The skeleton's two
// tiles must sit in the same grid, or they span the column, touching, and the
// answer is a jump in width and count (fe/briefs/17-ux-pass.md, U17a review).
test('17a.3 (the U17a review): the Overview skeleton tiles sit inside one .tiles grid, as the answer does', async () => {
  const gate = deferred()
  const { main } = await open({ instances: () => gate.promise }, '#/')
  await advance(300)

  const section = instancesSection(main)
  const tiles = section.querySelectorAll('.tile')
  expect(tiles.length, 'the two skeleton tiles are up').toBe(2)
  const grids = section.querySelectorAll('.tiles')
  expect(grids.length, 'the section has one .tiles grid while it reads').toBe(1)
  for (const tile of tiles)
    expect(
      tile.closest('.tiles'),
      'the tile is in the grid: a bare tile spans the column and jumps',
    ).toBe(grids[0])
  expect(
    grids[0]!.querySelectorAll('.tile').length,
    'both tiles are in that one grid',
  ).toBe(2)
  expect(
    grids[0]!.closest('[aria-hidden="true"]'),
    'the grid is part of the shape, so nothing to read',
  ).not.toBeNull()
  gate.resolve(reply(200, INSTANCES))
})
