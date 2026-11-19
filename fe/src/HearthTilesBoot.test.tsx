// Hearth H6b, the boot fact on the tile (fe/briefs/08-hearth-h6-overview.md).
//
// Two halves. The behaviour, in jsdom: the row is read through the bff
// (GET /api/instances/<id>/boot) for a signed-in visitor only, is absent while
// the read is pending and after every failure shape, and is one labelled pair
// in a definition list that is the tile's last child.
//
// The styling, as stylesheet text: jsdom lays out and paints nothing, so the
// declarations the brief gives are read off index.css the way HearthH2a.test.ts
// and HearthH6a.test.ts read it (the rule regex with comments dropped, prop()
// the last value of a property in a block, atRanges() and rangeEnd() for the
// blocks). The values are the brief's, written as it writes them, and an
// absence the brief states is pinned as an absence.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import App from './App.tsx'
import {
  INSTANCES,
  en,
  fakeBff,
  held,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── The page ────────────────────────────────────────────────────────────────

/** The whole app at the current hash, with this bff answering (fetch is stubbed). */
function open(bff: ReturnType<typeof fakeBff>) {
  expect(bff.calls, 'the bff is asked only once the page renders').toHaveLength(
    0,
  )
  return { bff, view: render(<App />) }
}

/** The instances list of main (H6a.1: a `ul` of tiles; main holds no other list). */
async function tiles(): Promise<HTMLElement> {
  await screen.findByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  return within(screen.getByRole('main')).findByRole('list')
}

/** The tile of one instance id, once it is on screen. */
async function tileOf(list: HTMLElement, id: string): Promise<HTMLLIElement> {
  const link = await within(list).findByRole('link', { name: id })
  const tile = link.closest('li')
  expect(tile, `the tile of ${id} is a list item`).not.toBeNull()
  return tile as HTMLLIElement
}

/** The tile as H6a left it and H6b.8 must leave it: the id link, its href, the badge. */
async function tileIs(list: HTMLElement, id: string, state: string) {
  const tile = await tileOf(list, id)
  expect(
    within(tile).getByRole('link', { name: id }).getAttribute('href'),
    `${id}: the tile links to the instance page`,
  ).toBe(`#/instances/${encodeURIComponent(id)}`)
  expect(
    tile.querySelector('.badge')?.textContent,
    `${id}: the state is still the badge's text`,
  ).toContain(state)
  return tile
}

/** The word the badge shows for a manager state. */
function stateWord(state: string): string {
  return en(`instance-state-${state}`)
}

/** Every boot call the bff saw, as "<METHOD> <path>". */
function bootCalls(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter((c) => c.path.endsWith('/boot'))
    .map((c) => `${c.method} ${c.path}`)
}

/** The reads of one instance's boot state (the path carries the encoded id). */
function bootReads(bff: ReturnType<typeof fakeBff>, id: string): number {
  return bff.to('GET', `/api/instances/${encodeURIComponent(id)}/boot`).length
}

/** The boot row of one tile, once it is on screen. */
async function bootRow(
  list: HTMLElement,
  id: string,
): Promise<HTMLDListElement> {
  const tile = await tileOf(list, id)
  await waitFor(() =>
    expect(tile.querySelector('dl'), `${id}: the row is there`).not.toBeNull(),
  )
  return tile.querySelector('dl') as HTMLDListElement
}

/** The pairs of a definition list, as the visitor reads them: term, definition. */
function pairs(dl: HTMLDListElement): Array<[string, string]> {
  const terms = Array.from(dl.querySelectorAll('dt'))
  const values = Array.from(dl.querySelectorAll('dd'))
  expect(terms.length, 'the row has a term').toBeGreaterThan(0)
  expect(values, 'one value per term').toHaveLength(terms.length)
  return terms.map((term, i) => [
    (term.textContent ?? '').trim(),
    ((values[i] as HTMLElement).textContent ?? '').trim(),
  ])
}

/** What each tile says about boot, in the order of the instances list. */
async function bootRows(
  list: HTMLElement,
  ids: string[] = INSTANCES.map((i) => i.id),
): Promise<string[]> {
  const rows: string[] = []
  for (const id of ids) {
    const dl = await bootRow(list, id)
    expect(dl.querySelectorAll('dt'), `${id}: one term`).toHaveLength(1)
    expect(dl.querySelectorAll('dd'), `${id}: one value`).toHaveLength(1)
    rows.push(`${id}: ${pairs(dl)[0]?.[1]}`)
  }
  return rows
}

/** The page with every boot read on its way and never answered. */
async function withPending() {
  const h = held()
  const seen: string[] = []
  const { bff, view } = open(
    fakeBff({
      boot: {
        read: (id, _n, init) => {
          seen.push(id)
          return h.answer(init)
        },
      },
    }),
  )
  const list = await tiles()
  await waitFor(() =>
    expect(seen.slice().sort()).toEqual(INSTANCES.map((i) => i.id).sort()),
  )
  return { bff, view, list }
}

describe('H6b.1 and H6b.2: who reads the boot state, and how often', () => {
  test('H6b.1: signed out, the tiles list the instances, no boot read is sent and no tile has a dl', async () => {
    const { bff } = open(fakeBff({ session: 'signed-out' }))
    const list = await tiles()
    expect(
      within(list)
        .getAllByRole('listitem')
        .map((li) => within(li).getByRole('link').textContent),
      'the tiles are the instances of the manager',
    ).toEqual(INSTANCES.map((i) => i.id))
    expect(
      document.querySelectorAll('dl'),
      'no boot row for a signed-out visitor',
    ).toHaveLength(0)
    expect(bootCalls(bff), 'no boot read for a signed-out visitor').toEqual([])
  })

  // The read is the signed-in page's: it starts with the session, never
  // before, so no tile can show a state the session has not earned.
  test('H6b.1: while the session is read there is no list, no row and no boot read, and the session is read once', async () => {
    const s = held()
    const { bff } = open(fakeBff({ session: (init) => s.answer(init) }))
    await screen.findByText(en('loading'))
    expect(
      within(screen.getByRole('main')).queryByRole('list'),
      'the tiles are not there yet',
    ).toBeNull()
    expect(document.querySelectorAll('dl'), 'no row').toHaveLength(0)
    expect(bootCalls(bff), 'no boot read before the session is known').toEqual(
      [],
    )
    s.resolve(reply(200, { user: 'alice' }))
    expect(await bootRows(await tiles())).toEqual(
      INSTANCES.map((i) => `${i.id}: ${en('instances-boot-off')}`),
    )
    expect(bff.to('GET', '/api/session'), 'one session read').toHaveLength(1)
  })

  // One read per instance, by the manager's instance id, and never again.
  test('H6b.2: signed in, each tile sends one boot read with its own id, nothing is polled and the list is read once', async () => {
    const { bff } = open(fakeBff())
    expect(await bootRows(await tiles())).toEqual(
      INSTANCES.map((i) => `${i.id}: ${en('instances-boot-off')}`),
    )
    for (const id of INSTANCES.map((i) => i.id))
      expect(
        bootReads(bff, id),
        `${id}: the read is made once, when the tile mounts`,
      ).toBe(1)
    expect(
      bootCalls(bff),
      'no poll: the boot calls are the number of instances',
    ).toHaveLength(INSTANCES.length)
    expect(
      bff.to('GET', '/api/instances'),
      'the list stays one read',
    ).toHaveLength(1)
  })

  // The id goes in the path, so an id that is not a path segment is sent
  // encoded, and asked for once.
  test('H6b.2: an id that is not a path segment is sent encoded, and sent once', async () => {
    const { bff } = open(
      fakeBff({
        instances: [{ id: 'gpu one', engine: 'llama.cpp', state: 'active' }],
      }),
    )
    expect(await bootRows(await tiles(), ['gpu one'])).toEqual([
      `gpu one: ${en('instances-boot-off')}`,
    ])
    expect(bootCalls(bff), 'the id goes in the path encoded').toEqual([
      'GET /api/instances/gpu%20one/boot',
    ])
    expect(
      bff.to('GET', '/api/instances/gpu one/boot'),
      'never the raw id',
    ).toHaveLength(0)
  })
})

describe('H6b.3, H6b.5 and H6b.7: the row as the visitor reads it', () => {
  // One pair: the reused label as the term, the new word as the definition, in
  // the `div` inside the `dl` the brief names, on the element the sheet styles.
  test('H6b.3: the row is dl.tile-facts with one div holding the term and the value, in the brief words', async () => {
    open(fakeBff({ boot: { read: () => reply(200, { enabled: true }) } }))
    const dl = await bootRow(await tiles(), 'default')
    expect(dl.className, 'the row is the element the sheet styles').toContain(
      'tile-facts',
    )
    const kids = Array.from(dl.children)
    expect(
      kids.map((n) => n.tagName),
      'the row holds one div',
    ).toEqual(['DIV'])
    expect(
      Array.from(kids[0]?.children ?? []).map((n) => n.tagName),
      'the div holds the term and the value, in that order',
    ).toEqual(['DT', 'DD'])
    expect(pairs(dl)).toEqual([[en('boot-label'), en('instances-boot-on')]])
    expect(
      within(dl)
        .getAllByRole('term')
        .map((n) => n.textContent?.trim()),
      'the term is the label of the instance page, reused',
    ).toEqual([en('boot-label')])
    expect(
      within(dl)
        .getAllByRole('definition')
        .map((n) => n.textContent?.trim()),
      'the value is the state word',
    ).toEqual([en('instances-boot-on')])
  })

  // The state words are the brief's and only these: On for enabled, Off for
  // disabled. No yes/no, true/false, enabled/disabled (H6b.7: words, not
  // colour, not a mark).
  test('H6b.3 and H6b.7: On when the manager says enabled, Off when it says disabled, and no other word for the state', async () => {
    const { view: onView } = open(
      fakeBff({ boot: { read: () => reply(200, { enabled: true }) } }),
    )
    expect(await bootRows(await tiles())).toEqual(
      INSTANCES.map((i) => `${i.id}: ${en('instances-boot-on')}`),
    )
    onView.unmount()

    open(fakeBff({ boot: { read: () => reply(200, { enabled: false }) } }))
    expect(await bootRows(await tiles())).toEqual(
      INSTANCES.map((i) => `${i.id}: ${en('instances-boot-off')}`),
    )
    for (const word of ['Yes', 'No', 'True', 'False', 'Enabled', 'Disabled'])
      expect(
        document.body.textContent,
        `the row says the state word, not ${word}`,
      ).not.toContain(word)
  })

  // The row is the tile's last child, inside the tile: the head with the id
  // and the badge, the engine line, and nothing of the tile left out (H6b.8).
  test('H6b.3: the row is the last child of its own tile, after what H6a put there', async () => {
    open(
      fakeBff({
        instances: [
          { id: 'default', engine: 'llama.cpp', state: 'active' },
          { id: 'coder', engine: 'strata', state: 'inactive' },
        ],
      }),
    )
    const list = await tiles()
    for (const i of [
      { id: 'default', state: stateWord('active') },
      { id: 'coder', state: stateWord('inactive') },
    ]) {
      const dl = await bootRow(list, i.id)
      const tile = await tileIs(list, i.id, i.state)
      expect(
        Array.from(tile.children),
        `${i.id}: the row is a child of the tile, not of a wrapper`,
      ).toContain(dl)
      expect(
        tile.lastElementChild,
        `${i.id}: the row is the tile's last child`,
      ).toBe(dl)
      expect(tile.querySelectorAll('dl'), `${i.id}: one row`).toHaveLength(1)
      expect(
        tile.querySelector('.tile-head'),
        `${i.id}: the head is before the row`,
      ).toBeTruthy()
      expect(
        tile.querySelector('.tile-engine'),
        `${i.id}: the engine line of a tile whose engine tells it apart`,
      ).toBeTruthy()
      expect(
        Array.from(tile.children).indexOf(dl),
        `${i.id}: after the head and the engine line`,
      ).toBe(tile.children.length - 1)
    }
  })
})

describe('H6b.4: no claim about a state that is not known', () => {
  // While a read is pending the tile shows no boot fact at all: not a value,
  // not a guess, not a word about boot.
  test('H6b.4: while the read is pending there is no row and no word about boot', async () => {
    const { list } = await withPending()
    expect(document.querySelectorAll('dl')).toHaveLength(0)
    for (const id of INSTANCES.map((i) => i.id)) {
      const tile = await tileOf(list, id)
      expect(tile.querySelector('dl'), `${id}: no row`).toBeNull()
      expect(tile.textContent, `${id}: no word about boot`).not.toMatch(/boot/i)
    }
  })

  // The tiles are not a status region: a row that arrives late must not be
  // read out over the visitor's own actions.
  test('H6b.4: a tile carries no live region, pending or not', async () => {
    const LIVE =
      '[aria-live], [role="status"], [role="alert"], [role="log"], [role="timer"], [role="marquee"]'
    const { view, list } = await withPending()
    expect(
      Array.from(document.querySelectorAll('.tile'))
        .flatMap((t) => Array.from(t.querySelectorAll(LIVE)))
        .map((n) => n.tagName.toLowerCase()),
      'nothing is announced while the rows are on their way',
    ).toEqual([])
    expect(within(list).getAllByRole('listitem')).toHaveLength(INSTANCES.length)
    view.unmount()

    open(fakeBff())
    await bootRows(await tiles())
    expect(
      Array.from(document.querySelectorAll('.tile')).map(
        (t) => t.querySelectorAll(LIVE).length,
      ),
      'the rows, once they are here, are silent too',
    ).toEqual(INSTANCES.map(() => 0))
  })

  // With no list there is nothing to read: the failure message is the Overview's
  // own, and no boot read goes out.
  test('H6b.4: when the bff cannot be reached the Overview says so and no boot read is sent', async () => {
    const { bff } = open(
      fakeBff({
        instances: () => Promise.reject(new TypeError('Failed to fetch')),
      }),
    )
    await screen.findByText(en('instances-failed'))
    expect(document.querySelectorAll('dl'), 'no row anywhere').toHaveLength(0)
    expect(bootCalls(bff), 'no boot read without a list').toEqual([])
  })

  // After a failure the boot fact is simply not there, in every shape the
  // brief names, and the rest of the tile is left alone.
  const failures: Array<[string, BffOptions['boot']]> = [
    [
      'the manager is unreachable (503 client-unavailable)',
      { read: () => reply(503, { key: 'client-unavailable', args: [] }) },
    ],
    [
      'the manager refused the bff (502 client-not-authorized)',
      { read: () => reply(502, { key: 'client-not-authorized', args: [] }) },
    ],
    [
      'the instance is gone (404 client-no-instance)',
      { read: () => reply(404, { key: 'client-no-instance', args: [] }) },
    ],
    ['the manager answered 500', { read: () => reply(500, 'secret-detail') }],
    ['a body that is not {"enabled": <bool>}', { read: () => reply(200, {}) }],
    [
      'a body whose enabled is not a bool',
      { read: () => reply(200, { enabled: 'yes' }) },
    ],
    ['a body that is not json', { read: () => reply(200, 'not json') }],
    [
      'a network failure',
      { read: () => Promise.reject(new TypeError('down')) },
    ],
  ]
  for (const [what, boot] of failures)
    test(`H6b.4: no row, no claim and no poll when ${what}`, async () => {
      const { bff } = open(fakeBff({ boot }))
      const list = await tiles()
      for (const i of INSTANCES)
        await waitFor(() => expect(bootReads(bff, i.id)).toBe(1))
      expect(document.querySelectorAll('dl'), 'no row anywhere').toHaveLength(0)
      for (const i of INSTANCES) {
        const tile = await tileIs(list, i.id, stateWord(i.state))
        expect(tile.querySelector('dl'), `${i.id}: no row`).toBeNull()
        expect(tile.textContent, `${i.id}: no word about boot`).not.toMatch(
          /boot/i,
        )
      }
      expect(
        bootCalls(bff),
        'one read each: a failure is not retried or polled',
      ).toHaveLength(INSTANCES.length)
    })
})

describe('H6b.2: the rows join with the sign-in banner', () => {
  test('H6b.2: signing in from the banner adds the rows, Sign out removes them, and the tiles stay', async () => {
    const { bff } = open(fakeBff({ session: 'signed-out' }))
    const banner = await screen.findByRole('banner')
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-out') }),
      'the banner offers Sign in, not Sign out',
    ).toBeNull()

    // The tiles are already there for the signed-out visitor: signing in adds
    // the rows, it does not bring the list (H6b.2).
    const list = await tiles()
    expect(within(list).getAllByRole('listitem')).toHaveLength(INSTANCES.length)
    expect(document.querySelectorAll('dl')).toHaveLength(0)
    expect(bootCalls(bff), 'no boot read while signed out').toEqual([])

    fireEvent.click(
      within(banner).getByRole('link', { name: en('session-sign-in') }),
    )
    fireEvent.change(
      await screen.findByRole('textbox', { name: en('signin-username') }),
      { target: { value: 'alice' } },
    )
    fireEvent.change(
      screen.getByLabelText(en('signin-password'), { selector: 'input' }),
      { target: { value: 'hunter2' } },
    )
    fireEvent.click(screen.getByRole('button', { name: en('signin-submit') }))

    expect(await bootRows(await tiles())).toEqual(
      INSTANCES.map((i) => `${i.id}: ${en('instances-boot-off')}`),
    )
    expect(
      bootCalls(bff),
      'the sign-in mounts the rows: one read per instance, no more',
    ).toHaveLength(INSTANCES.length)

    fireEvent.click(
      screen.getByRole('button', { name: en('session-sign-out') }),
    )
    await waitFor(() => expect(document.querySelectorAll('dl')).toHaveLength(0))
    expect(
      within(await tiles()).getAllByRole('listitem'),
      'the tiles stay: only the rows went',
    ).toHaveLength(INSTANCES.length)
    expect(
      bootCalls(bff),
      'the rows never polled, before or after the sign-out',
    ).toHaveLength(INSTANCES.length)
  })
})

// ── The stylesheet, read as text ────────────────────────────────────────────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  compact: string
  body: string
  at: number
  depth: number
}

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim(),
  // Whitespace dropped: the selector is written over several lines.
  compact: m[1].replace(/\s+/g, ''),
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: a rule inside an @media block is an override, not the
// rule of the tile.
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

// Every property a block declares, in the order it declares them.
const declared = (body: string): string[] =>
  body
    .split(';')
    .map((d) => d.slice(0, d.indexOf(':')).trim())
    .filter((name) => name !== '' && !name.startsWith('--'))

const single = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length
          ? ` (${list.map((r) => r.sel.replace(/\s+/g, ' ')).join(' | ')})`
          : ''),
    )
  return list[0]
}

// The top-level rule whose selector is exactly this.
const topBy = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) => r.compact === sel.replace(/ /g, '')),
    `the top-level rule whose selector is exactly "${sel}" (${what})`,
  )

// [open, end] of every block whose header is this at-rule, in file order.
const atRanges = (header: string): Array<[number, number]> => {
  const out: Array<[number, number]> = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf(header, from)
    if (at < 0) return out
    const end = rangeEnd(at)
    out.push([at, end])
    from = end + 1
  }
}

// The offset of the brace that closes the block opened at `open`.
const rangeEnd = (open: number): number => {
  const start = CSS.indexOf('{', open)
  if (start < 0) throw new Error(`no block opens at ${open}`)
  let depth = 0
  for (let i = start; i < CSS.length; i++) {
    if (CSS[i] === '{') depth++
    if (CSS[i] === '}' && --depth === 0) return i
  }
  throw new Error(`the block at ${open} is never closed`)
}

// The rules whose selector carries this selector, inside a block of this
// at-rule.
const inside = (header: string, sel: string): Rule[] =>
  atRanges(header).flatMap(([open, end]) =>
    RULES.filter(
      (r) =>
        r.depth > 0 &&
        r.at > open &&
        r.at < end &&
        r.compact.includes(sel.replace(/\s+/g, '')),
    ),
  )

describe('H6b.6: the boot row, in the stylesheet', () => {
  // The row is one classed element, `tile-facts`, laid out by the sheet and by
  // nothing in a style attribute: a column of rows, a row of space-between
  // pairs, the hairline above it, the label quiet, the value in the tile's
  // weight. The brief's declarations, once each.
  const wanted: Array<[string, string, string]> = [
    ['.tile-facts', 'margin', 'var(--space-2) 0 0'],
    ['.tile-facts', 'display', 'grid'],
    ['.tile-facts', 'gap', 'var(--space-2)'],
    ['.tile-facts > div', 'display', 'flex'],
    ['.tile-facts > div', 'flex-wrap', 'wrap'],
    ['.tile-facts > div', 'justify-content', 'space-between'],
    ['.tile-facts > div', 'gap', 'var(--space-1) var(--space-3)'],
    ['.tile-facts > div', 'padding-top', 'var(--space-2)'],
    ['.tile-facts > div', 'border-top', '1px solid var(--line)'],
    ['.tile-facts dt', 'color', 'var(--muted)'],
    ['.tile-facts dd', 'margin', '0'],
    ['.tile-facts dd', 'font-weight', '600'],
  ]
  for (const [sel, name, want] of wanted)
    test(`H6b.6: ${sel} declares ${name} as ${want}`, () => {
      const rule = topBy(sel, `the ${sel} rule of the boot row`)
      expect(prop(rule.body, name), `${sel}: ${name}`).toBe(want)
      expect(
        declared(rule.body).filter((n) => n === name).length,
        `${sel}: ${name} is declared once`,
      ).toBe(1)
    })

  // Nothing else: no colour, no length and no colour scheme of its own, so the
  // row follows the theme through the tokens alone, and the value carries the
  // badge's weight and not a new one.
  test('H6b.6: the row declares the brief list and nothing beyond it', () => {
    const brief = new Map<string, string[]>()
    for (const [sel, name] of wanted)
      brief.set(sel, [...(brief.get(sel) ?? []), name])
    for (const [sel, names] of brief)
      expect(
        declared(topBy(sel, `the ${sel} rule`).body).sort(),
        `${sel}: exactly the properties of the brief, no extra length, colour, scheme or motion`,
      ).toEqual([...names].sort())
    expect(
      prop(topBy('.badge', 'the badge rule').body, 'font-weight'),
      'the value carries the badge weight, not a new one',
    ).toBe('600')
  })

  // The row is the row in every media: no override in the width media (the
  // same row at 320 px), in the dark scheme, in the light one, in forced
  // colours; and it does not move.
  test('H6b.6: the row has no override of its own in any @media block', () => {
    for (const sel of [
      '.tile-facts',
      '.tile-facts > div',
      '.tile-facts dt',
      '.tile-facts dd',
    ])
      expect(
        [
          ...inside('@media (max-width:', sel),
          ...inside('@media (min-width:', sel),
          ...inside('@media (prefers-color-scheme: dark)', sel),
          ...inside('@media (prefers-color-scheme: light)', sel),
          ...inside('@media (forced-colors: active)', sel),
        ].map((r) => r.sel.replace(/\s+/g, ' ')),
        `${sel}: the row is the row in every media, theme and width`,
      ).toEqual([])
    expect(
      declared(topBy('.tile-facts > div', 'the pair rule').body).some((n) =>
        /^(transition|animation)(-|$)/.test(n),
      ),
      'the row does not move',
    ).toBe(false)
  })

  // The row is the tile's own element: the tile rules do not name it, and the
  // row has exactly the four rules of the brief.
  test('H6b.6: the row is not part of the head, the engine line, the badge or a link', () => {
    for (const sel of ['.tile-head', '.tile-id', '.tile-engine', '.badge'])
      expect(
        topBy(sel, `the ${sel} rule`).sel.includes('tile-facts'),
        `${sel} does not name the boot row: the row is its own element`,
      ).toBe(false)
    expect(
      RULES.filter((r) => r.compact.includes('.tile-facts')).map((r) =>
        r.sel.replace(/\s+/g, ' '),
      ),
      'the row has the four rules of the brief and no other: the element, the pair, the label, the value',
    ).toEqual([
      '.tile-facts',
      '.tile-facts > div',
      '.tile-facts dt',
      '.tile-facts dd',
    ])
  })
})
