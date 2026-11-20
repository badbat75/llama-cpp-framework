// Slice 13, the boot state that comes with the instances list
// (fe/briefs/13-node-about.md, 13 N.1 and 13 N.2): the Overview tile shows
// Start at boot from the `boot` of its list item, in the first render of that
// answer, and the Overview sends no read of /api/instances/{id}/boot at all.
// The instance page keeps its own read (Boot.test.tsx). "13 N.1" is the
// brief's criterion 1.
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
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

/** An item of the list, with `boot` spelled any way a real answer spells it. */
function item(id: string, boot?: unknown): Instance {
  const base: Record<string, unknown> = {
    id,
    engine: 'llama.cpp',
    state: 'active',
  }
  if (boot !== undefined) base.boot = boot
  return base as unknown as Instance
}

/** The Overview with this bff: the tiles list, once it is on screen. */
async function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  const h2 = await screen.findByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const section = h2.parentElement as HTMLElement
  return { bff, list: await within(section).findByRole('list') }
}

/** The tile of one instance id, once it is on screen. */
async function tileOf(list: HTMLElement, id: string): Promise<HTMLLIElement> {
  const link = await within(list).findByRole('link', { name: id })
  const tile = link.closest('li')
  expect(tile, `the tile of ${id} is a list item`).not.toBeNull()
  return tile as HTMLLIElement
}

/** Every call to a boot path, as "<METHOD> <path>". */
function bootCalls(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter((c) => c.path.endsWith('/boot'))
    .map((c) => `${c.method} ${c.path}`)
}

/** The pairs of a definition list, as the visitor reads them: term, value. */
function pairs(el: Element): Array<[string, string]> {
  const terms = Array.from(el.querySelectorAll('dt'))
  const values = Array.from(el.querySelectorAll('dd'))
  expect(values, 'one value per term').toHaveLength(terms.length)
  return terms.map((term, i) => [
    (term.textContent ?? '').trim(),
    ((values[i] as HTMLElement).textContent ?? '').trim(),
  ])
}

/** What a tile says about Start at boot: the value, or null when there is no row. */
function bootOf(tile: HTMLLIElement): string | null {
  const term = Array.from(tile.querySelectorAll('dl.tile-facts dt')).find(
    (dt) => dt.textContent?.trim() === en('boot-label'),
  )
  if (term === undefined) return null
  const row = term.parentElement as HTMLElement
  return (row.querySelector('dd')?.textContent ?? '').trim()
}

// ── 13 N.1: the row is the list ────────────────────────────────────────────

test('13 N.1: an item whose boot is true shows the row On and one whose boot is false shows Off, in the first render of that answer', async () => {
  const { list } = await open({
    instances: [item('default', true), item('coder', false)],
  })
  // No second answer awaited: the row is there with the tile.
  expect(bootOf(await tileOf(list, 'default')), 'true is On').toBe(
    en('instances-boot-on'),
  )
  expect(bootOf(await tileOf(list, 'coder')), 'false is Off').toBe(
    en('instances-boot-off'),
  )
  const terms = Array.from(document.querySelectorAll('dl.tile-facts dt')).map(
    (dt) => dt.textContent?.trim(),
  )
  expect(
    terms.filter((term) => term === en('boot-label')).length,
    'one Start at boot row per tile, no more',
  ).toBe(2)
})

test('13 N.1: the row is the pair it was, dl.tile-facts with one div, boot-label the term, the tile last child', async () => {
  const { list } = await open({ instances: [item('default', true)] })
  const tile = await tileOf(list, 'default')
  const dl = tile.querySelector('dl.tile-facts')
  expect(dl, 'the row is the element the sheet styles').not.toBeNull()
  expect(
    Array.from(dl?.children ?? []).map((n) => n.tagName),
    'the row holds one div',
  ).toEqual(['DIV'])
  expect(pairs(dl as Element), 'the pair, in the reused words').toEqual([
    [en('boot-label'), en('instances-boot-on')],
  ])
  expect(tile.lastElementChild, 'the row is the tile last child').toBe(dl)
})

// The list is the only source: a value that is not a boolean says nothing, as
// a failed read says nothing today.
const NO_ROW: Array<[string, unknown]> = [
  ['null', null],
  ['the string "true"', 'true'],
  ['the number 1', 1],
  ['an empty string', ''],
  ['an object', { enabled: true }],
]
for (const [what, boot] of NO_ROW)
  test(`13 N.1: an item whose boot is ${what} shows no Start at boot row and no word about boot`, async () => {
    const { list } = await open({ instances: [item('default', boot)] })
    const tile = await tileOf(list, 'default')
    expect(bootOf(tile), `${what}: no row`).toBeNull()
    expect(tile.textContent, `${what}: no word about boot`).not.toMatch(/boot/i)
  })

test('13 N.1: an item with no boot field shows no row, and a tile with no fact at all has no dl', async () => {
  const { list } = await open({ instances: INSTANCES })
  const tile = await tileOf(list, 'default')
  expect(
    tile.querySelector('dl'),
    'no port, no tags, no boot: the dl rule of today holds',
  ).toBeNull()
  expect(bootOf(tile), 'no row').toBeNull()
})

test('13 N.1: the row shares the dl with the port and the tags rows, after them', async () => {
  const { list } = await open({
    instances: [
      {
        id: 'default',
        engine: 'llama.cpp',
        state: 'active',
        port: 8580,
        tags: ['edge'],
        boot: true,
      },
    ],
  })
  const tile = await tileOf(list, 'default')
  const dl = tile.querySelector('dl.tile-facts')
  expect(dl, 'one dl for the facts').not.toBeNull()
  expect(pairs(dl as Element), 'the facts, in the order of today').toEqual([
    [en('server-port'), '8580'],
    [en('instance-tags'), 'edge'],
    [en('boot-label'), en('instances-boot-on')],
  ])
})

// ── 13 N.2: the Overview never reads the boot route ────────────────────────

test('13 N.2: opening the Overview sends no boot read, and staying on it through any number of list answers sends none', async () => {
  const { bff } = await open()
  expect(bootCalls(bff), 'the list carries the state: no boot read').toEqual([])
  for (const boot of [true, false, null]) {
    bff.setInstances([item('default', boot)])
    navigate('#/profiles/model')
    await screen.findByRole('heading', {
      level: 1,
      name: en('nav-model-profiles'),
    })
    navigate('#/')
    await screen.findByRole('heading', {
      level: 2,
      name: en('instances-title'),
    })
    expect(bootCalls(bff), `no boot read for boot ${String(boot)}`).toEqual([])
  }
  expect(
    bff.to('GET', '/api/instances').length,
    'the list is read once per page, as always',
  ).toBeGreaterThanOrEqual(2)
})

test('13 N.2: a later list answer that flips boot moves the row with it, with no read', async () => {
  const { bff, list } = await open({ instances: [item('default', false)] })
  expect(bootOf(await tileOf(list, 'default')), 'first answer: Off').toBe(
    en('instances-boot-off'),
  )
  bff.setInstances([item('default', true)])
  navigate('#/profiles/model')
  await screen.findByRole('heading', {
    level: 1,
    name: en('nav-model-profiles'),
  })
  navigate('#/')
  const h2 = await screen.findByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const again = await tileOf(
    await within(h2.parentElement as HTMLElement).findByRole('list'),
    'default',
  )
  expect(bootOf(again), 'the row follows the new answer').toBe(
    en('instances-boot-on'),
  )
  expect(
    bff.to('GET', '/api/instances/default/boot').length,
    'following it costs no read',
  ).toBe(0)
})

test('13 N.2: a list read that answers 401 signs the app out, and no boot read happens on the way', async () => {
  const bff = fakeBff({
    instances: () => reply(401, { key: 'session-required', args: [] }),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  expect(
    within(main).getByText(en('signin-session-ended')),
    'the list 401 is the read that says the session ended',
  ).toBeTruthy()
  expect(bootCalls(bff), 'the tiles sent nothing').toEqual([])
})

test('13 N.2 (13 N.13): the instance page keeps its own boot read', async () => {
  startAt('#/instances/default')
  const bff = fakeBff()
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('group', { name: en('boot-label') })
  expect(
    bff.to('GET', '/api/instances/default/boot').length,
    'the page reads it once, as today',
  ).toBe(1)
})
