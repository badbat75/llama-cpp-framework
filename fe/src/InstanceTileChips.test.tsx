// Brief 17, slice U17b (fe/briefs/17-ux-pass.md): the Overview's tiles show an
// instance's tags as fixed chips, not as one string joined by ", ". The whole
// app renders at #/ against a fake bff (mocked fetch); "17b.n" is the brief's
// criterion n. The tile's rows stay pinned by brief 12 in
// InstanceTiles.test.tsx, which U17b amends where it read the joined text; the
// chips' own rules are brief 15 section 10 R3 (the ellipsis and its 15em
// wrap), R5 (the expanded pill on hover) and fe/DESIGN.md 5.2 Chip.
//
// 17b.4 (a 32-character tag inside its tile at 320 px, no sideways scroll) is
// the UX check's `overview-tiles` state, which jsdom cannot measure: it is
// pinned in fe/ux/check.js.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import {
  en,
  fakeBff,
  isHidden,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

// The sheet as text: jsdom lays out and paints nothing, so 17b.3's
// declarations are read off index.css the way Reflow.test.ts reads it (the same
// rule regex, comments dropped, the last value of a property in a block); those
// files export no helper, and importing a test file registers its tests here.
const CSS = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
  .replace(/\/\*[\s\S]*?\*\//g, '')
  .replace(/\s+/g, ' ')

/** The value of `name` in a block, the last one when it is declared twice. */
function decl(body: string, name: string): string {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

/** Every `name: value` of a block, as written, in order. */
function decls(body: string): string[] {
  return body
    .split(';')
    .map((d) => d.trim())
    .filter((d) => d.includes(':'))
    .map((d) => d.replace(/\s*:\s*/, ': '))
}

/** The sheet with every at-rule block, header included, taken out: the
 *  top-level rules alone, so a selector that also appears inside a media query
 *  is read where it stands on its own. */
function dropAt(css: string): string {
  const out: string[] = []
  let i = 0
  for (;;) {
    const at = css.indexOf('@media', i)
    if (at < 0) {
      out.push(css.slice(i))
      break
    }
    let depth = 0
    let end = css.indexOf('{', at)
    for (; end < css.length; end++) {
      if (css[end] === '{') depth++
      else if (css[end] === '}' && --depth === 0) break
    }
    out.push(css.slice(i, at))
    i = end + 1
  }
  return out.join('')
}

/** The body of the one rule whose selector is exactly `sel`, in the sheet
 *  without media queries. */
function topBlock(sel: string): string {
  const hits = [...dropAt(CSS).matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter(
    (m) => (m[1] ?? '').trim() === sel,
  )
  expect(
    hits.map((m) => (m[1] ?? '').trim()),
    `exactly one rule whose selector is "${sel}" outside any media query`,
  ).toEqual([sel])
  return hits[0]![2] ?? ''
}

/** Every at-rule of the sheet: its header (without the brace) and its body. */
function atRules(): { header: string; body: string }[] {
  const out: { header: string; body: string }[] = []
  const re = /@media[^{}]*\{/g
  for (let m = re.exec(CSS); m !== null; m = re.exec(CSS)) {
    // The match ends at the block's opening brace, so the walk starts inside
    // it: the depth is 1 there, and a nested brace (the rule the block holds)
    // counts on top of it. The body ends at the `}` that closes the block, not
    // at the first inner one.
    let depth = 1
    let end = m.index + m[0].length
    for (; end < CSS.length; end++) {
      if (CSS[end] === '{') depth++
      else if (CSS[end] === '}' && --depth === 0) break
    }
    out.push({
      header: m[0].replace(/\{\s*$/, '').trim(),
      body: CSS.slice(m.index + m[0].length, end),
    })
  }
  return out
}

/** The rules an at-rule body holds, as the selector and its own body. */
function rulesIn(body: string): Array<[string, string]> {
  return [...body.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => [
    (m[1] ?? '').trim(),
    (m[2] ?? '').trim(),
  ])
}

// ── Fixtures and helpers ─────────────────────────────────────────────────

/** A tag at the manager's cap (32 characters, brief 15 section 0). */
const LONG = 'abcdefghijklmnopqrstuvwxyz0123'

/** The three items: the brief's fixture, a second tile, and the widest one. */
const ITEM = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 8080,
  tags: ['edge', 'v2', 'gpu'],
  instanceTags: ['v2', 'gpu'],
  boot: true,
}
const CODER = {
  id: 'coder',
  engine: 'llama.cpp',
  state: 'inactive',
  port: 8580,
  tags: ['coding', 'fast'],
  instanceTags: ['coding'],
  boot: false,
}
const LONG_ITEM = {
  id: 'long',
  engine: 'strata',
  state: 'activating',
  port: 65535,
  tags: [`${LONG}0`, `${LONG}1`],
  instanceTags: [`${LONG}0`],
  boot: true,
}

/** The list answered from a function, so a test can answer any shape. */
function itemsAnswer(items: () => unknown[]): BffOptions['instances'] {
  return () => reply(200, JSON.stringify(items()))
}

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  startAt('')
})

/** The tiles list (`ul.tiles`), never a tile's own chips list: 17b.1 puts a
 *  `ul.chips role="list"` inside every tile's Tags cell, so a bare
 *  findByRole('list') matches several. The tiles list is the one list that is
 *  not inside a tile. */
async function tilesList(container: HTMLElement): Promise<HTMLElement> {
  const lists = await within(container).findAllByRole('list')
  const tiles = lists.filter((el) => el.closest('.tile') === null)
  expect(tiles, 'one list outside the tiles').toHaveLength(1)
  return tiles[0]!
}

/** The Overview with these items, and its tiles list. */
async function openOverview(items: () => unknown[]) {
  const bff = fakeBff({ instances: itemsAnswer(items) })
  render(<App />)
  await screen.findByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const list = await tilesList(screen.getByRole('main'))
  return { bff, list }
}

/** The tile of one instance id, once it is on screen. */
async function tileOf(list: HTMLElement, id: string): Promise<HTMLElement> {
  const link = await within(list).findByRole('link', { name: id })
  const tile = link.closest('li')
  expect(tile, `the tile of ${id} is a list item`).not.toBeNull()
  return tile as HTMLElement
}

/** The terms of the tile's facts. */
function terms(tile: HTMLElement): string[] {
  const dl = tile.querySelector('dl.tile-facts')
  return Array.from(dl?.querySelectorAll('dt') ?? []).map((dt) =>
    (dt.textContent ?? '').trim(),
  )
}

/** The tile's Tags value cell (its `dd`), once the row is there. */
function tagsCell(tile: HTMLElement): HTMLElement {
  const dl = tile.querySelector('dl.tile-facts')
  expect(dl, 'the tile has its facts').not.toBeNull()
  const dts = Array.from(dl!.querySelectorAll('dt'))
  const dds = Array.from(dl!.querySelectorAll('dd'))
  expect(dts.length, 'one value per term').toBe(dds.length)
  const at = dts.findIndex(
    (dt) => (dt.textContent ?? '').trim() === en('instance-tags'),
  )
  expect(
    at,
    `the tile has a ${en('instance-tags')} row`,
  ).toBeGreaterThanOrEqual(0)
  return dds[at]!
}

/** The `ul.chips` the Tags cell holds, once it is there. */
async function chipsList(cell: HTMLElement): Promise<HTMLElement> {
  const list = await within(cell).findByRole('list')
  expect(list.tagName, 'the list of tags is a ul').toBe('UL')
  expect(
    list.className.split(/\s+/),
    'it carries the chips class of fe/DESIGN.md 5.2',
  ).toContain('chips')
  expect(
    list.getAttribute('role'),
    'the list is a list: UA styles drop its role with list-style: none',
  ).toBe('list')
  return list
}

/** The chips of a list, as items, in the order they show. */
function chipItems(list: HTMLElement): HTMLElement[] {
  return within(list).getAllByRole('listitem')
}

/** The text of each item. */
function chipTexts(items: HTMLElement[]): string[] {
  return items.map((li) => (li.textContent ?? '').trim())
}

/** The span a chip's text is in (15 section 10 R3). */
function textSpan(li: HTMLElement): HTMLElement {
  const span = li.querySelector(':scope > span')
  expect(
    span,
    'the chip text is in a span of its own: an ellipsis needs a block box of its own (15 section 10 R3)',
  ).not.toBeNull()
  return span as HTMLElement
}

/** States how much of a chip's text shows and how wide it is: R5 measures the
 *  cut with `span.scrollWidth > span.clientWidth`, and jsdom lays out nothing,
 *  so the test says it. */
function measure(li: HTMLElement, shown: number, needed: number) {
  const span = textSpan(li)
  for (const [name, value] of [
    ['clientWidth', shown],
    ['scrollWidth', needed],
  ] as Array<[string, number]>)
    Object.defineProperty(span, name, { configurable: true, value })
}

/** The expanded pill of 15 section 10 R5, inside the chip, while it shows. */
function fullPill(li: HTMLElement): HTMLElement | null {
  const el = li.querySelector('.chip-full')
  return el === null || isHidden(el) ? null : (el as HTMLElement)
}

/** The pointer entering and leaving a chip. React 19 builds onMouseEnter and
 *  onMouseLeave from pointerover/pointerout and mouseover/mouseout; a listener
 *  of its own hears the plain mouseenter/mouseleave, so all three go. */
function pointerEnter(li: HTMLElement) {
  fireEvent.pointerOver(li)
  fireEvent.mouseOver(li)
  fireEvent.mouseEnter(li)
}
function pointerLeave(li: HTMLElement, to: HTMLElement) {
  fireEvent.pointerOut(li, { relatedTarget: to })
  fireEvent.mouseOut(li, { relatedTarget: to })
  fireEvent.mouseLeave(li, { relatedTarget: to })
}

// ── 17b.1: the tags are a list of chips ──────────────────────────────────

test('17b.1: the Tags dd holds a ul.chips role="list" of three items edge, v2 and gpu in the order of tags, each text in a span of its own, with no button in the list', async () => {
  const { list } = await openOverview(() => [ITEM])
  const cell = tagsCell(await tileOf(list, ITEM.id))
  const chips = await chipsList(cell)
  expect(chipTexts(chipItems(chips)), 'the tags, as list items').toEqual([
    'edge',
    'v2',
    'gpu',
  ])
  for (const li of chipItems(chips))
    expect(
      (textSpan(li).textContent ?? '').trim(),
      'the text is in the span the ellipsis rule of R3 cuts',
    ).toBe((li.textContent ?? '').trim())
  expect(
    within(chips).queryAllByRole('button'),
    'a tile chip is fixed: no Remove tag button (the page edits them)',
  ).toEqual([])
})

test('17b.1: a tile with no tags shows no Tags row, as today', async () => {
  const { list } = await openOverview(() => [
    { ...ITEM, tags: [] },
    { ...ITEM, id: 'coder', tags: undefined },
  ])
  for (const id of [ITEM.id, 'coder']) {
    const tile = await tileOf(list, id)
    expect(terms(tile), `${id}: no ${en('instance-tags')} row`).not.toContain(
      en('instance-tags'),
    )
  }
})

test('17b.1: every tile shows its own tags, in the order of tags, the instance and the profile ones alike, the tile not telling them apart', async () => {
  const { list } = await openOverview(() => [ITEM, CODER, LONG_ITEM])
  for (const item of [ITEM, CODER, LONG_ITEM]) {
    const chips = await chipsList(tagsCell(await tileOf(list, item.id)))
    expect(
      chipTexts(chipItems(chips)),
      `${item.id}: the tags as the item carries them`,
    ).toEqual(item.tags)
    expect(
      chips.querySelectorAll('li').length,
      `${item.id}: one item per tag`,
    ).toBe(item.tags.length)
  }
})

// ── 17b.2: the joined string is gone ─────────────────────────────────────

test('17b.2: no edge, v2, gpu text joined by commas remains on the tile: each tag is an item of its own', async () => {
  const { list } = await openOverview(() => [ITEM])
  const tile = await tileOf(list, ITEM.id)
  const joined = ITEM.tags.join(', ')
  const cell = tagsCell(tile)
  expect(
    cell.textContent,
    'the value cell no longer spells the list with commas',
  ).not.toContain(joined)
  for (const el of Array.from(tile.querySelectorAll('*')))
    expect(
      el.textContent,
      `no ${el.tagName.toLowerCase()} on the tile carries the joined string`,
    ).not.toContain(joined)
})

// ── 17b.3: the sheet draws a tile chip ───────────────────────────────────

test('17b.3: .tile .chip is one rule, declaring background: var(--bg) and the --control border (the tile is --surface)', () => {
  const body = topBlock('.tile .chip')
  expect(
    decl(body, 'background'),
    'the tile is --surface, so the chip is drawn with --bg',
  ).toBe('var(--bg)')
  expect(
    decl(body, 'border'),
    'the edge forced colours show (fe/DESIGN.md 5.2 Chip)',
  ).toBe('var(--border-w) solid var(--control)')
})

test('17b.3: the .chip > span rules of 15 section 10 R3 stand, top level and in the 15em block, so a tile chip is cut with an ellipsis and wraps only under 15em', () => {
  expect(decls(topBlock('.chip > span')), 'the four of R3, in order').toEqual([
    'min-width: 0',
    'overflow: hidden',
    'white-space: nowrap',
    'text-overflow: ellipsis',
  ])
  const blocks = atRules().filter((a) => a.header.includes('max-width: 15em'))
  expect(
    blocks.map((a) => a.header),
    'exactly one 15em block',
  ).toEqual(['@media (max-width: 15em)'])
  const rules = rulesIn(blocks[0]!.body)
  expect(
    rules.map(([sel]) => sel),
    'the block holds only the chip span (15 section 10 R3)',
  ).toEqual(['.chip > span'])
  expect(
    decls(rules[0]![1]),
    'the last resort of R3: the tag breaks at the letter',
  ).toEqual(['white-space: normal', 'overflow-wrap: anywhere'])
})

// ── The expanded pill of 15 section 10 R5, on a tile chip ────────────────

test('17b.1 (rule, 15 section 10 R5): the pointer entering a tile chip whose text is cut opens a span.chip-full, aria-hidden, holding the whole tag, and leaving hides it; a whole tag opens nothing', async () => {
  const { list } = await openOverview(() => [ITEM, LONG_ITEM])
  const long = chipItems(
    await chipsList(tagsCell(await tileOf(list, LONG_ITEM.id))),
  )[0]!
  const edge = chipItems(
    await chipsList(tagsCell(await tileOf(list, ITEM.id))),
  )[0]!
  measure(long, 40, 200)
  measure(edge, 120, 120)
  pointerEnter(long)
  const pill = fullPill(long)
  expect(
    pill,
    'the cut chip on the tile opens the pill under the pointer',
  ).not.toBeNull()
  expect(
    pill!.getAttribute('aria-hidden'),
    'the list item already reads the whole tag, so the pill is not read twice',
  ).toBe('true')
  expect(pill!.textContent, 'the whole tag, not the cut one').toBe(`${LONG}0`)
  pointerEnter(edge)
  expect(fullPill(edge), 'a whole tag opens nothing').toBeNull()
  pointerLeave(long, list)
  expect(
    fullPill(long),
    'the pointer left the chip: the pill is gone',
  ).toBeNull()
})

test('17b.1 (rule, 15 section 10 R5): a tile chip is not focusable and carries no title: the tile has its one tab stop, the id link, and the pill is the only way the mouse sees a cut tag', async () => {
  const { list } = await openOverview(() => [ITEM, LONG_ITEM])
  for (const item of [ITEM, LONG_ITEM]) {
    const tile = await tileOf(list, item.id)
    const chips = await chipsList(tagsCell(tile))
    for (const li of chipItems(chips)) {
      expect(
        li.getAttribute('tabindex'),
        `${item.id}: a tile chip takes no tab stop (R5 amended)`,
      ).toBe(null)
      expect(
        li.getAttribute('title'),
        `${item.id}: no title (R3: the pill says it)`,
      ).toBe(null)
      expect(
        li.querySelectorAll('a, button, input, select, textarea').length,
        `${item.id}: the chip holds no control`,
      ).toBe(0)
    }
    expect(
      tile.querySelectorAll('a, button, input, select, textarea').length,
      `${item.id}: the id link is the tile's only control`,
    ).toBe(1)
  }
})

// ── 17b.5: the dl keeps its pairs, the dd its break ──────────────────────

test('17b.5: the dl keeps its dt/dd pairs with the list inside the Tags dd, and the sheet keeps overflow-wrap: anywhere on .tile-facts dd (12 T1 layout)', async () => {
  const { list } = await openOverview(() => [ITEM])
  const tile = await tileOf(list, ITEM.id)
  const dl = tile.querySelector('dl.tile-facts')
  expect(dl, 'the tile has its facts').not.toBeNull()
  const rows = Array.from(dl!.children)
  expect(
    rows.map(
      (row) =>
        `${row.querySelectorAll('dt').length}/${row.querySelectorAll('dd').length}`,
    ),
    'every row of the dl is one term and one value',
  ).toEqual(['1/1', '1/1', '1/1'])
  const cell = tagsCell(tile)
  expect(cell.tagName, 'the value is a dd').toBe('DD')
  const chips = await chipsList(cell)
  expect(chips.closest('dd'), 'the list sits in the Tags value cell').toBe(cell)
  const body = topBlock('.tile-facts dd')
  expect(
    decl(body, 'overflow-wrap'),
    'the value cell still breaks a word with no break point',
  ).toBe('anywhere')
})
