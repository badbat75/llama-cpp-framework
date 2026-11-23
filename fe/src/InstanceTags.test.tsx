// Brief 12, slice T1 (fe/briefs/12-instance-tags.md): what of it brief 15 left
// standing: the instance's port on its page (T1.4), and the strings keys of T1.1
// that survived the chips. The instance-page tags criteria T1.5 to T1.14 are
// replaced by brief 15 slice C1 and live in InstanceTagChips.test.tsx; the tile
// criteria (T1.2, T1.3, T1.4 of the tiles) are in InstanceTiles.test.tsx.
// App.test.tsx ("1a strings") checks the EN table of test/bff.ts against
// strings/en_US.json, so the keys are pinned from both sides.
import { act, cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  en,
  fakeBff,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

/** The brief's fixture: the item carries port, the union and its own tags. */
const TAG_ITEM = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 8080,
  tags: ['edge', 'v2', 'gpu'],
  instanceTags: ['v2', 'gpu'],
}

/** The keys of brief 12 section 2 that brief 15 keeps, as the EN table spells them. */
const KEYS = [
  'instance-port',
  'instance-tags',
  'instance-tags-own',
  'instance-tags-saving',
  'api-server-bad-tags',
]

/** Lists of items answered as bytes, so a test can answer any shape. */
function itemsAnswer(items: () => unknown[]): BffOptions['instances'] {
  return () => reply(200, JSON.stringify(items()))
}

beforeEach(() => {
  startAt('#/instances/default')
})

afterEach(async () => {
  // A hash set lands as a hashchange in a task of its own: let it finish on
  // this test's page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a history traversal) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The app at the instance page, with the brief's fixture, loaded. */
async function openPage(options: BffOptions = {}) {
  const bff = fakeBff({
    instances: [TAG_ITEM],
    boot: { read: () => reply(200, { enabled: true }) },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await screen.findByRole('heading', { level: 1, name: 'default' })
  await within(main).findByRole('group', { name: en('instance-tags-own') })
  return { bff, main }
}

// ── T1.1: the texts that stay

test('12 T1.1: en_US.json has the keys brief 15 keeps with exactly their text', () => {
  const table: Record<string, string> = strings
  const missing = KEYS.filter((key) => table[key] === undefined)
  expect(missing, 'keys missing from strings/en_US.json').toEqual([])
  const wrong = KEYS.filter((key) => table[key] !== en(key))
  expect(
    wrong,
    'keys whose en_US.json text is not the English of the brief',
  ).toEqual([])
})

// ── T1.4: the header line

test('12 T1.4: the paragraph Port 8080 follows the engine line; with port 0 there is no such paragraph', async () => {
  const { main } = await openPage()
  const engine = within(main).getByText(
    en('instance-engine').replace('{0}', 'llama.cpp'),
  )
  const port = within(main).getByText(en('instance-port', '8080'))
  expect(port.tagName, 'a paragraph').toBe('P')
  expect(before(engine, port), 'the port line follows the engine line').toBe(
    true,
  )
  cleanup()
  const second = await openPage({
    instances: itemsAnswer(() => [{ ...TAG_ITEM, port: 0 }]),
  })
  expect(
    within(second.main).queryByText(/^Port \d/),
    'port 0: no port paragraph',
  ).toBeNull()
})
