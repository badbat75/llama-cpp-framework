// Brief 12, slice T1 (fe/briefs/12-instance-tags.md): the port and the tags on
// the Overview's tiles. The whole app renders at #/ against a fake bff (mocked
// fetch); "12 T1.n" is the brief's criterion n. The instance-page criteria are
// in InstanceTags.test.tsx. Since brief 13 (13 N.1) the boot row comes from
// the item's `boot` in the list answer, so the fixtures carry it and no tile
// sends a boot read of its own (13 N.2). Brief 17 slice U17b amends the Tags
// row: its value is a list of chips, so `facts` reads the items (the chips
// themselves, their CSS and the pill are pinned in InstanceTileChips.test.tsx).
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test } from 'vitest'
import App from './App.tsx'
import { en, fakeBff, reply, startAt, type BffOptions } from './test/bff.ts'

/** The brief's fixture item, with the boot the T1 tests show (13 N.1). */
const ITEM = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 8080,
  tags: ['edge', 'v2', 'gpu'],
  instanceTags: ['v2', 'gpu'],
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

// ── Helpers ─────────────────────────────────────────────────────────────

/** The Overview's tiles list (`ul.tiles`), never a tile's own chips list:
 *  17b.1 puts a `ul.chips role="list"` inside every tile's Tags cell, so a
 *  bare findByRole('list') matches several. The tiles list is the one list
 *  that is not inside a tile. */
async function tiles(): Promise<HTMLElement> {
  await screen.findByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const lists = await within(screen.getByRole('main')).findAllByRole('list')
  const found = lists.filter((el) => el.closest('.tile') === null)
  expect(found, 'one list outside the tiles').toHaveLength(1)
  return found[0]!
}

/** The tile link of one instance id: 16 17f.1 names a tile by the item's name,
 *  which is the engine and the port when the item has one, so the tile of an id
 *  is found by the address it leads to, not by its text. */
function tileLink(list: HTMLElement, id: string): HTMLAnchorElement | null {
  return list.querySelector<HTMLAnchorElement>(
    `a.tile-id[href="#/instances/${encodeURIComponent(id)}"]`,
  )
}

/** The tile of one instance id, once it is on screen. */
async function tileOf(list: HTMLElement, id: string): Promise<HTMLLIElement> {
  await waitFor(() =>
    expect(tileLink(list, id), `the tile link of ${id}`).not.toBeNull(),
  )
  const tile = tileLink(list, id)!.closest('li')
  expect(tile, `the tile of ${id} is a list item`).not.toBeNull()
  return tile as HTMLLIElement
}

/** The tile's facts as the visitor reads them: term, value. Null with none.
 *  Brief 17 U17b amends the Tags value: the cell is a list of chips, so its
 *  value is read as its items, in order, joined for reading (17b.1: the tags
 *  are the items, no longer one string the tile spells with commas). */
function facts(tile: HTMLElement): Array<[string, string]> | null {
  const dl = tile.querySelector('dl.tile-facts')
  if (dl === null) return null
  const terms = Array.from(dl.querySelectorAll('dt'))
  const values = Array.from(dl.querySelectorAll('dd'))
  expect(terms.length, 'one value per term').toBe(values.length)
  return terms.map((term, i) => {
    const dd = values[i] as HTMLElement
    const list = dd.querySelector('ul')
    const value =
      list === null
        ? (dd.textContent ?? '')
        : Array.from(list.querySelectorAll('li'))
            .map((li) => (li.textContent ?? '').trim())
            .join(', ')
    return [(term.textContent ?? '').trim(), value.trim()]
  })
}

/** The terms of the tile's facts. */
function terms(tile: HTMLElement): string[] {
  return (facts(tile) ?? []).map(([term]) => term)
}

/** Waits until the tile's facts hold `n` rows, and returns them. */
async function rows(tile: HTMLElement, n: number) {
  await waitFor(() => expect(facts(tile), `${n} rows`).toHaveLength(n))
  return facts(tile)
}

/** The Overview, with these items. */
async function openOverview(items: () => unknown[]) {
  const bff = fakeBff({ instances: itemsAnswer(items) })
  render(<App />)
  const list = await tiles()
  return { bff, list, tile: await tileOf(list, ITEM.id) }
}

// ── T1.2: the three rows, in order ──────────────────────────────────────

test('12 T1.2 (as 17b.1 amends it): the tile has Port, Tags and Start at boot in this order, all three with the first render (13 N.1: the row is there with the tile)', async () => {
  const { tile } = await openOverview(() => [ITEM])
  // The Tags value is the tile's list of chips read as its items (17b.1).
  expect(await rows(tile, 3), 'the three rows come with the tile').toEqual([
    [en('server-port'), '8080'],
    [en('instance-tags'), 'edge, v2, gpu'],
    [en('boot-label'), en('instances-boot-on')],
  ])
})

test('12 T1.2 (as 17b.1 amends it): the Port and Tags rows are there when the item carries no boot', async () => {
  const { tile } = await openOverview(() => [{ ...ITEM, boot: null }])
  expect(
    await rows(tile, 2),
    'a null boot keeps the two rows (13 N.1)',
  ).toEqual([
    [en('server-port'), '8080'],
    [en('instance-tags'), 'edge, v2, gpu'],
  ])
})

// ── T1.3: the rows that are not there ───────────────────────────────────

describe('12 T1.3: no row for a port or tags that are not there', () => {
  const cases: [string, () => unknown[], string][] = [
    ['port 0', () => [{ ...ITEM, port: 0 }], en('server-port')],
    ['port absent', () => [{ ...ITEM, port: undefined }], en('server-port')],
    ['tags empty', () => [{ ...ITEM, tags: [] }], en('instance-tags')],
    ['tags absent', () => [{ ...ITEM, tags: undefined }], en('instance-tags')],
  ]
  for (const [what, items, term] of cases) {
    test(`12 T1.3: ${what}: no ${term} row`, async () => {
      const { tile } = await openOverview(items)
      expect(await rows(tile, 2), 'the tile has its other rows').toHaveLength(2)
      expect(terms(tile), `no ${term} row`).not.toContain(term)
    })
  }
})

test('12 T1.3: no port, no tags and no boot: the tile has no dl', async () => {
  const { tile } = await openOverview(() => [
    { id: ITEM.id, engine: 'llama.cpp', state: 'active' },
  ])
  expect(facts(tile), 'nothing to show, so no facts at all').toBeNull()
})

test('12 T1.3: a port that is a string and tags that are a string: the list shows, without those rows', async () => {
  const { bff, tile } = await openOverview(() => [
    { ...ITEM, port: '8080', tags: 'edge' },
  ])
  expect(
    screen.queryByText(en('instances-failed')),
    'the list is not marked failed',
  ).toBeNull()
  expect(
    within(tile).getByRole('link', { name: ITEM.id }),
    'the tile is there',
  ).toBeTruthy()
  expect(await rows(tile, 1), 'only the row that reads well is left').toEqual([
    [en('boot-label'), en('instances-boot-on')],
  ])
  expect(bff.to('GET', '/api/instances'), 'the list is read once').toHaveLength(
    1,
  )
})
