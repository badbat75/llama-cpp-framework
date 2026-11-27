// Brief 22, slice L2 (fe/briefs/22-model-sources.md, "L2 acceptance list for
// the tester" item 11): 22 D4.37, the instance link of a Used by cell. The link
// is the instance's name and nothing else; the id shows beside it only when
// another instance carries the same name, where the name alone would send the
// reader to the wrong page. Two places show it: the Server profiles list's Used
// by cell (16 17f.4) and the server profile page's Used by row (16 17b.9).
//
// The rule is pinned through what the two places show, not through the props of
// Instances.tsx's InstanceLink: the link gets the name of an id today, so how
// it learns that a name is shared is the coder's choice (a name-of that knows
// the whole list, a second helper of useInstanceNames, a prop).
//
// What D4.37 does NOT touch is pinned as a guard: the Overview tile keeps its
// id line (16 17f.3), which is where the id of the instance is the reader's
// way to tell two tiles apart, and the list's link stays a plain link to the
// instance page.
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import {
  en,
  fakeBff,
  instanceName,
  reply,
  serverProfileDoc,
  startAt,
  type BffOptions,
  type Instance,
} from './test/bff.ts'

// ── The fixtures ──────────────────────────────────────────────────────────
/** One instance, named by its label, and the name it shows. */
const NAMED: Instance[] = [
  {
    id: 'default',
    engine: 'llama.cpp',
    state: 'active',
    port: 9931,
    label: 'Coding assistant',
  },
]
const NAME = instanceName(NAMED[0]!)

/** Two instances with the same label, told apart only by their ids. */
const TWINS: Instance[] = [
  ...NAMED,
  {
    id: 'staging',
    engine: 'llama.cpp',
    state: 'inactive',
    port: 9932,
    label: NAME,
  },
]

/** One instance with no label and no port: its name IS its id (16 17f.1). */
const AS_ID: Instance[] = [{ id: 'embed', engine: 'strata', state: 'failed' }]

/** A server profile row used by `instance`. */
const usedBy = (id: string, profile = 'qwen-27b') => ({
  id: profile,
  engine: 'llama.cpp',
  port: 9931,
  tags: [],
  instance: id,
})

// ── The harness ───────────────────────────────────────────────────────────
/** The app at `hash`, mounted on `NAMED` unless the test gives other
 * instances. */
function app(hash: string, options: BffOptions = {}) {
  startAt(hash)
  fakeBff({ instances: NAMED, ...options })
  render(<App />)
  return screen.getByRole('main')
}

/** The Server profiles list, its table read. */
async function serverList(options: BffOptions = {}): Promise<HTMLElement> {
  const main = app('#/profiles/server', {
    serverProfiles: () => reply(200, [usedBy('default')]),
    ...options,
  })
  return within(main).findByRole('table', {
    name: en('server-profiles-title'),
  }) as Promise<HTMLElement>
}

/** The row of one server profile. */
const rowOf = (table: HTMLElement, id: string): HTMLElement =>
  within(table).getByRole('row', { name: new RegExp(id) })

/** The server profile page of `qwen-27b`, used by the profile's `instance`. */
function pageOf(profile: string, options: BffOptions) {
  return app(`#/profiles/server/${profile}`, options)
}

/** The Used by cell of the page's Value list, once the read has landed (the
 * row is there only with the read: ServerProfilePage.tsx). */
async function pageUsedBy(main: HTMLElement): Promise<HTMLElement> {
  await waitFor(() =>
    expect(
      main.querySelector('dl.assets'),
      'the page Value list, with the read',
    ).not.toBeNull(),
  )
  const dd = main.querySelector('dl.assets dd')
  expect(dd, 'its Used by row').not.toBeNull()
  await within(dd as HTMLElement).findByRole('link')
  return dd as HTMLElement
}

beforeEach(() => {
  startAt('#/')
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── The Server profiles list ──────────────────────────────────────────────

test('22 L2 D4.37: the list Used by link is the name alone, and no id stands beside it when no other instance has that name', async () => {
  const table = await serverList()
  const row = rowOf(table, 'qwen-27b')
  const link = within(row).getByRole('link', { name: NAME })
  expect(link.getAttribute('href'), 'the link is the instance page').toBe(
    '#/instances/default',
  )
  expect(
    link.textContent?.trim(),
    'the words of the link are the name, the id is not inside it',
  ).toBe(NAME)
  expect(
    row.querySelector('.instance-id'),
    'one instance of this name: the id would only repeat what the name says (D4.37)',
  ).toBeNull()
})

test('22 L2 D4.37: two instances of the same name: each Used by keeps the id of its own instance outside the name-only link', async () => {
  const table = await serverList({
    instances: TWINS,
    serverProfiles: () =>
      reply(200, [usedBy('default'), usedBy('staging', 'gemma-12b')]),
  })
  for (const [profile, id] of [
    ['qwen-27b', 'default'],
    ['gemma-12b', 'staging'],
  ] as const) {
    const row = rowOf(table, profile)
    const link = within(row).getByRole('link', { name: NAME })
    expect(
      link.getAttribute('href'),
      `the row of ${profile} leads to ${id}`,
    ).toBe(`#/instances/${id}`)
    expect(
      link.textContent?.trim(),
      `the link says the name alone, so two rows do not read as one link (${profile})`,
    ).toBe(NAME)
    const span = row.querySelector('.instance-id')
    expect(
      span,
      `the shared name is told apart by the id (${profile})`,
    ).not.toBeNull()
    expect(span!.textContent, 'the id is the one of this row').toBe(
      en('instance-id-shown', id),
    )
    expect(link.contains(span!), 'the id is not inside the link').toBe(false)
  }
})

test('22 L2 D4.37: the Used by of an instance whose name is its id says it once: a link of the id, no id beside it', async () => {
  const table = await serverList({
    instances: AS_ID,
    serverProfiles: () => reply(200, [usedBy('embed')]),
  })
  const row = rowOf(table, 'qwen-27b')
  const link = within(row).getByRole('link', { name: 'embed' })
  expect(link.textContent?.trim(), 'the name is the id').toBe('embed')
  expect(
    row.querySelector('.instance-id'),
    'the id is not said twice',
  ).toBeNull()
})

// ── The server profile page ───────────────────────────────────────────────

test('22 L2 D4.37: the server profile page Used by is the name-only link, with no id beside it for a name no other instance has', async () => {
  const main = pageOf('qwen-27b', {
    serverProfile: () =>
      reply(200, serverProfileDoc({}, { instance: 'default' })),
  })
  const cell = await pageUsedBy(main)
  const link = within(cell).getByRole('link', { name: NAME })
  expect(link.textContent?.trim(), 'the link is the name alone').toBe(NAME)
  expect(
    cell.querySelector('.instance-id'),
    'the id follows the link only where names are shared (D4.37)',
  ).toBeNull()
})

test('22 L2 D4.37: the server profile page Used by keeps the id when the instance shares its name', async () => {
  const main = pageOf('qwen-27b', {
    instances: TWINS,
    serverProfile: () =>
      reply(200, serverProfileDoc({}, { instance: 'staging' })),
  })
  const cell = await pageUsedBy(main)
  const link = within(cell).getByRole('link', { name: NAME })
  expect(
    link.getAttribute('href'),
    'the link is the page of the instance that uses this profile',
  ).toBe('#/instances/staging')
  const span = cell.querySelector('.instance-id')
  expect(span, 'the shared name keeps its id on the page too').not.toBeNull()
  expect(span!.textContent).toBe(en('instance-id-shown', 'staging'))
})

// ── The guard: D4.37 is about the Used by links ───────────────────────────

test('22 L2 D4.37: the Overview tile of one instance of a name keeps its id line (16 17f.3 is not part of D4.37)', async () => {
  const main = app('#/')
  await within(main).findByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const tiles = (
    await within(main).findAllByRole('list', { hidden: true })
  ).filter((list) => list.closest('.tile') === null)
  expect(tiles.length, 'one list of tiles').toBe(1)
  const tile = within(tiles[0]!).getAllByRole('listitem')[0]!
  const id = tile.querySelector('p.instance-id')
  expect(
    id,
    'the tile still says which instance it is: 16 17f.3, which D4.37 leaves alone',
  ).not.toBeNull()
  expect(id!.textContent).toBe(en('instance-id-shown', 'default'))
})
