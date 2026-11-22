// Hearth H6b, the boot fact on the tile (fe/briefs/08-hearth-h6-overview.md),
// as slice 13 retargets it (fe/briefs/13-node-about.md, 13 N.1 and 13 N.2).
//
// Two halves. The behaviour, in jsdom: the row is the `boot` of the instance's
// item in GET /api/instances, so it is there with the tile in that answer's
// first render; the Overview sends no read of /api/instances/<id>/boot at all,
// and a tile can no longer end the session. The row is one labelled pair in a
// definition list that is the tile's last child, in the same words.
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

// ── The page ────────────────────────────────────────────────────────────────

/** An item of the list with its `boot` spelled any way a reply spells it. */
function item(id: string, boot?: unknown): Instance {
  const base: Record<string, unknown> = {
    id,
    engine: 'llama.cpp',
    state: 'active',
  }
  if (boot !== undefined) base.boot = boot
  return base as unknown as Instance
}

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

/** Every call to a boot path, as "<METHOD> <path>". Since 13 N.2 there are none. */
function bootCalls(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter((c) => c.path.endsWith('/boot'))
    .map((c) => `${c.method} ${c.path}`)
}

/** The boot row of one tile: the `dl` that holds the Start at boot pair. */
function bootRowOf(tile: HTMLLIElement): HTMLDListElement | null {
  const term = Array.from(tile.querySelectorAll('dl.tile-facts dt')).find(
    (dt) => dt.textContent?.trim() === en('boot-label'),
  )
  return (term?.closest('dl') as HTMLDListElement) ?? null
}

/** The boot row of one tile, once it is on screen. */
async function bootRow(
  list: HTMLElement,
  id: string,
): Promise<HTMLDListElement> {
  const tile = await tileOf(list, id)
  await waitFor(() =>
    expect(bootRowOf(tile), `${id}: the row is there`).not.toBeNull(),
  )
  return bootRowOf(tile) as HTMLDListElement
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

describe('H6b.1 and H6b.2 (13 N.1, 13 N.2): where the boot state comes from', () => {
  // The state is the list's, so it arrives with the list, and the tile asks
  // for nothing. (09 S1a.1: signed out there is no Overview at all.)
  test('H6b.1 (13 N.1): while the session is read there is no list and no row, and nothing is read beside the session', async () => {
    let resolveSession: (r: Response) => void = () => {}
    const session = new Promise<Response>((r) => {
      resolveSession = r
    })
    const { bff } = open(fakeBff({ session: () => session }))
    await screen.findByText(en('loading'))
    expect(
      within(screen.getByRole('main')).queryByRole('list'),
      'the tiles are not there yet',
    ).toBeNull()
    expect(document.querySelectorAll('dl'), 'no row').toHaveLength(0)
    expect(bootCalls(bff), 'no boot read at all').toEqual([])
    resolveSession(reply(200, { user: 'alice' }))
    const list = await tiles()
    expect(within(list).getAllByRole('listitem')).toHaveLength(INSTANCES.length)
    expect(
      bootCalls(bff),
      'the list carries the state: the tiles read nothing',
    ).toEqual([])
    expect(bff.to('GET', '/api/session'), 'one session read').toHaveLength(1)
  })

  // 13 N.1: `boot: true` is On, `boot: false` is Off, in the first render.
  test('H6b.2 (13 N.1): the rows are the list answers, On for true and Off for false, with no boot read', async () => {
    const { bff } = open(
      fakeBff({ instances: [item('default', true), item('coder', false)] }),
    )
    const list = await tiles()
    expect(await bootRows(list)).toEqual([
      `default: ${en('instances-boot-on')}`,
      `coder: ${en('instances-boot-off')}`,
    ])
    expect(
      bootCalls(bff),
      'no read, so no poll: the tiles ask for nothing',
    ).toEqual([])
    expect(
      bff.to('GET', '/api/instances'),
      'the list stays one read',
    ).toHaveLength(1)
  })

  // The id is the list's, so a name that is not a path segment needs no path
  // at all: it is shown and read as the list says.
  test('H6b.2 (13 N.2): an id that is not a path segment needs no encoded path, and shows its row', async () => {
    const { bff } = open(fakeBff({ instances: [item('gpu one', true)] }))
    expect(await bootRows(await tiles(), ['gpu one'])).toEqual([
      `gpu one: ${en('instances-boot-on')}`,
    ])
    expect(bootCalls(bff), 'no path is built for the id').toEqual([])
  })
})

describe('H6b.3, H6b.5 and H6b.7: the row as the visitor reads it', () => {
  // One pair: the reused label as the term, the new word as the definition, in
  // the `div` inside the `dl` the brief names, on the element the sheet styles.
  test('H6b.3: the row is dl.tile-facts with one div holding the term and the value, in the brief words', async () => {
    open(fakeBff({ instances: [item('default', true)] }))
    const dl = await bootRow(await tiles(), 'default')
    expect(dl.className, 'the row is the element the sheet styles').toContain(
      'tile-facts',
    )
    const kids = Array.from(dl.children)
    expect(kids, 'the row holds one div').toHaveLength(1)
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

  // The state words are the brief's and only these: On for true, Off for
  // false. No yes/no, true/false, enabled/disabled (H6b.7: words, not colour,
  // not a mark).
  test('H6b.3 and H6b.7: On when the list says true, Off when it says false, and no other word for the state', async () => {
    const { view: onView } = open(
      fakeBff({ instances: INSTANCES.map((i) => item(i.id, true)) }),
    )
    expect(await bootRows(await tiles())).toEqual(
      INSTANCES.map((i) => `${i.id}: ${en('instances-boot-on')}`),
    )
    onView.unmount()

    open(fakeBff({ instances: INSTANCES.map((i) => item(i.id, false)) }))
    expect(await bootRows(await tiles())).toEqual(
      INSTANCES.map((i) => `${i.id}: ${en('instances-boot-off')}`),
    )
    // The tiles, not the whole body: the nav carries "Node info", whose "No" is
    // no state word.
    const onTiles = (await tiles()).textContent ?? ''
    for (const word of ['Yes', 'No', 'True', 'False', 'Enabled', 'Disabled'])
      expect(onTiles, `the row says the state word, not ${word}`).not.toContain(
        word,
      )
  })

  // The row is the tile's last child, inside the tile: the head with the id
  // and the badge, the engine line, and nothing of the tile left out (H6b.8).
  test('H6b.3: the row is the last child of its own tile, after what H6a put there', async () => {
    open(
      fakeBff({
        instances: [
          {
            id: 'default',
            engine: 'llama.cpp',
            state: 'active',
            boot: false,
          },
          { id: 'coder', engine: 'strata', state: 'inactive', boot: true },
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

describe('H6b.4 (13 N.1): no claim about a state that is not known', () => {
  // A value that is not a boolean says nothing, as a failed read said nothing
  // today: no row, no word about boot, and the rest of the tile is left alone.
  const noRow: Array<[string, unknown]> = [
    ['the list leaves boot out', undefined],
    ['the list says null', null],
    ['the list says "true"', 'true'],
    ['the list says 1', 1],
    ['the list says an empty string', ''],
    ['the list says an object', { enabled: true }],
  ]
  for (const [what, boot] of noRow)
    test(`H6b.4 (13 N.1): no row, no claim and no read when ${what}`, async () => {
      const { bff } = open(fakeBff({ instances: [item('default', boot)] }))
      const list = await tiles()
      const tile = await tileIs(list, 'default', stateWord('active'))
      expect(tile.querySelector('dl'), 'no row anywhere').toBeNull()
      expect(tile.textContent, 'no word about boot').not.toMatch(/boot/i)
      expect(
        bootCalls(bff),
        'a value that says nothing is not retried',
      ).toEqual([])
    })

  // The tiles are not a status region: the rows are the list's, and nothing
  // arrives later to be read out over the visitor's own actions.
  test('H6b.4: a tile carries no live region, rows and no rows', async () => {
    const LIVE =
      '[aria-live], [role="status"], [role="alert"], [role="log"], [role="timer"], [role="marquee"]'
    open(fakeBff({ instances: [item('default')] }))
    const list = await tiles()
    expect(within(list).getAllByRole('listitem')).toHaveLength(1)
    expect(
      Array.from(document.querySelectorAll('.tile')).map(
        (t) => t.querySelectorAll(LIVE).length,
      ),
      'no row: nothing announced',
    ).toEqual([0])
    cleanup()

    open(fakeBff({ instances: INSTANCES.map((i) => item(i.id, true)) }))
    await bootRows(await tiles())
    expect(
      Array.from(document.querySelectorAll('.tile')).map(
        (t) => t.querySelectorAll(LIVE).length,
      ),
      'the rows, once they are here, are silent too',
    ).toEqual(INSTANCES.map(() => 0))
  })

  // With no list there is nothing to show: the failure message is the
  // Overview's own, and no boot read goes out.
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

  // A list that fails for a reason is the Overview's message, in the words of
  // today (a keyed refusal its own), and no tile shows a boot fact.
  const failures: Array<[string, () => Response, string]> = [
    [
      'the manager is unreachable (503 client-unavailable)',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('instances-manager-unavailable'),
    ],
    [
      'the manager refused the bff (502 client-not-authorized)',
      () => reply(502, { key: 'client-not-authorized', args: [] }),
      en('instances-not-authorized'),
    ],
    [
      'the bff answered 500',
      () => reply(500, 'secret-detail'),
      en('instances-failed'),
    ],
    [
      'a body that is not a list',
      () => reply(200, 'not json'),
      en('instances-failed'),
    ],
  ]
  for (const [what, instances, words] of failures)
    test(`H6b.4: no row, no claim and no boot read when the list read fails: ${what}`, async () => {
      const options: BffOptions = { instances }
      const { bff } = open(fakeBff(options))
      await screen.findByText(words)
      expect(document.querySelectorAll('dl'), 'no row anywhere').toHaveLength(0)
      expect(bootCalls(bff), 'the tiles ask for nothing').toEqual([])
    })
})

describe('H6b.2: the rows join with the sign-in banner', () => {
  test('09 S1a.1 (H6b.2): signing in from the page at #/ brings the whole Overview, Sign out takes it away', async () => {
    const { bff } = open(fakeBff({ session: 'signed-out' }))
    const banner = await screen.findByRole('banner')
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-out') }),
      'the banner offers no Sign out',
    ).toBeNull()

    // 09 S1a.1: signed out, `#/` is the sign-in page: there is no list and no
    // row to lose, and nothing of the server is asked for.
    const main = screen.getByRole('main')
    await within(main).findByRole('heading', {
      level: 1,
      name: en('signin-title'),
    })
    expect(within(main).queryByRole('list'), 'no tiles signed out').toBeNull()
    expect(document.querySelectorAll('dl')).toHaveLength(0)
    expect(bootCalls(bff), 'no boot read while signed out').toEqual([])

    // The form of the page signs in (09 S1a.4: the banner's pill is gone).
    fireEvent.change(
      await within(main).findByRole('textbox', { name: en('signin-username') }),
      { target: { value: 'alice' } },
    )
    fireEvent.change(
      within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
      { target: { value: 'hunter2' } },
    )
    fireEvent.click(
      within(main).getByRole('button', { name: en('signin-submit') }),
    )

    // The sign-in brings the list and the rows at once, in the same answer.
    const list = await tiles()
    expect(within(list).getAllByRole('listitem')).toHaveLength(INSTANCES.length)
    expect(
      bootCalls(bff),
      'the rows came with the list: the sign-in asked for nothing else',
    ).toEqual([])

    fireEvent.click(
      screen.getByRole('button', { name: en('session-sign-out') }),
    )
    await within(main).findByRole('heading', {
      level: 1,
      name: en('signin-title'),
    })
    expect(document.querySelectorAll('dl'), 'the rows went').toHaveLength(0)
    expect(
      within(main).queryByRole('list'),
      'the whole Overview went with the session (09 S1a.1)',
    ).toBeNull()
    expect(
      bootCalls(bff),
      'the rows never read, before or after the sign-out',
    ).toEqual([])
  })
})

describe('13 N.2: the rows follow a later list answer, with no read', () => {
  test('13 N.2: a list answer that flips boot moves the row, and the Overview sent no boot read on the way', async () => {
    const { bff } = open(fakeBff({ instances: [item('default', false)] }))
    const list = await tiles()
    expect(await bootRows(list, ['default'])).toEqual([
      `default: ${en('instances-boot-off')}`,
    ])
    bff.setInstances([item('default', true)])
    navigate('#/profiles/model')
    await screen.findByRole('heading', {
      level: 1,
      name: en('model-profiles-title'),
    })
    navigate('#/')
    expect(bootCalls(bff), 'following the new answer costs no read').toEqual([])
    expect(await bootRows(await tiles(), ['default'])).toEqual([
      `default: ${en('instances-boot-on')}`,
    ])
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
    ['.tile-facts dd', 'overflow-wrap', 'anywhere'],
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
