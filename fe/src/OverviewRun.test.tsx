// Brief 24, section A1 (fe/briefs/24-ux-audit.md, criteria 23 A1.1 to 23 A1.4):
// the instance page's run controls on the Overview's tiles. The whole app
// renders at #/ against a fake bff (mocked fetch) on fake timers, so a write's
// chain (POST, fresh read of the shared store, render) lands on flush; "24 A1.n"
// names the brief's criterion. The Overview is not polled (1c.41): the tile's
// write settles on the shared store's fresh read, never a poll of its own.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  reply,
  shownText,
  startAt,
  type BffOptions,
  type Instance,
} from './test/bff.ts'

// ── Helpers (the RunControls.test.tsx pattern) ──────────────────────────

/**
 * Fake timers: a tile's write may schedule timers of its own; the clock stays
 * real-time-free so nothing waits on the wall. setImmediate and microtasks
 * stay real, so fetch's Response bodies are read as usual.
 */
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
    for (let i = 0; i < 6; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

/** Moves the clock by `ms` in one step, then flushes. */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

function list(state: string): Instance[] {
  return [
    { id: 'default', engine: 'llama.cpp', state },
    { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
  ]
}

/** The two instances stopped, coder with a display name (16 17f): the tile
 *  shows and says that name, so the refusal of 24 A1.4 D1 names it. */
function namedList(): Instance[] {
  return [
    { id: 'default', engine: 'llama.cpp', state: 'inactive' },
    {
      id: 'coder',
      engine: 'llama.cpp',
      state: 'inactive',
      label: 'Coding assistant',
    },
  ]
}

/** The app at `hash` (signed in as an administrator by default), loaded. */
async function open(options: BffOptions = {}, hash = '#/') {
  fakeClock()
  startAt(hash)
  const bff = fakeBff(options)
  render(<App />)
  await flush()
  return { bff, main: screen.getByRole('main') }
}

/** Leaves the app: unmount, drop the fake clock, so the next open is clean. */
function close() {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
}

/** The Overview's tiles list: the Instances section's list (no chips lists:
 *  the fixtures carry no tags). */
function instanceTiles(main: HTMLElement): HTMLElement {
  const heading = within(main).getByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const section = heading.closest('section') as HTMLElement
  expect(section, 'the Instances section').toBeTruthy()
  return within(section).getByRole('list')
}

/** The tile of one instance id, found by the address its link leads to. */
function tileOf(main: HTMLElement, id: string): HTMLElement {
  const link = instanceTiles(main).querySelector<HTMLAnchorElement>(
    `a.tile-id[href="#/instances/${encodeURIComponent(id)}"]`,
  )
  expect(link, `the tile link of ${id}`).not.toBeNull()
  const tile = link!.closest('li')
  expect(tile, `the tile of ${id} is a list item`).not.toBeNull()
  return tile as HTMLElement
}

/** The tile's verb for one action key, named with the instance (23 A1.3). */
function tileButton(
  tile: HTMLElement,
  key: string,
  id: string,
): HTMLButtonElement {
  return within(tile).getByRole('button', { name: `${en(key)} ${id}` })
}

function tileButtons(tile: HTMLElement, id: string): HTMLButtonElement[] {
  return ['run-start', 'run-stop', 'run-restart'].map((k) =>
    tileButton(tile, k, id),
  )
}

/** The row of the tile's run buttons: the parent of its Start. */
function runRow(tile: HTMLElement, id: string): HTMLElement {
  const [start, stop, restart] = tileButtons(tile, id)
  const row = start.parentElement as HTMLElement
  expect(row.contains(stop), 'the three verbs share one row').toBe(true)
  expect(row.contains(restart), 'the three verbs share one row').toBe(true)
  return row
}

/** The tile's own status line: the one role="status" inside the tile. */
function tileStatus(tile: HTMLElement): HTMLElement {
  const all = within(tile).queryAllByRole('status')
  expect(all, 'one status line in the tile').toHaveLength(1)
  return all[0]!
}

/** The texts of the tile's alerts. */
function tileAlerts(tile: HTMLElement): string[] {
  return within(tile)
    .queryAllByRole('alert')
    .map((a) => a.textContent ?? '')
}

function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

/** Which of Start, Stop, Restart are aria-disabled, as [start, stop, restart]. */
function inactiveSet(tile: HTMLElement, id: string): boolean[] {
  return tileButtons(tile, id).map(inactive)
}

/** Focuses `el` and clicks it, as a pointer or Enter does. */
function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function posts(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter((c) => c.method === 'POST' && c.path.startsWith('/api/instances'))
    .map((c) => c.path)
}

function reads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', '/api/instances').length
}

function badgeText(tile: HTMLElement): string {
  const badge = tile.querySelector('.badge')
  expect(badge, 'the tile carries its state badge').not.toBeNull()
  return badge!.textContent ?? ''
}

afterEach(() => {
  close()
  startAt('')
})

// ── The stylesheet, as text (jsdom lays nothing out) ────────────────────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

const TOP = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
  .filter((m) => {
    const at = m.index ?? 0
    return (
      (CSS.slice(0, at).match(/\{/g) ?? []).length ===
      (CSS.slice(0, at).match(/\}/g) ?? []).length
    )
  })
  .map((m) => ({
    compact: m[1].replace(/\s+/g, ''),
    body: m[2].replace(/\s+/g, ' '),
  }))

const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

// ── 23 A1.1: the buttons on the tile ────────────────────────────────────

const STATES: [string, boolean[]][] = [
  ['active', [true, false, false]],
  ['inactive', [false, true, true]],
  ['failed', [false, true, true]],
]

test.each(STATES)(
  '24 A1.1: each tile ends with the instance page run buttons for its state (%s): the row is the tile last element, secondary, aria-disabled by state and never disabled, with its empty status line after it',
  async (state, expected) => {
    const { main } = await open({ instances: list(state) })
    const tile = tileOf(main, 'default')
    const row = runRow(tile, 'default')
    expect(
      tile.lastElementChild,
      `${state}: the run row is the tile's last element`,
    ).toBe(row)
    expect(inactiveSet(tile, 'default'), state).toEqual(expected)
    for (const b of tileButtons(tile, 'default')) {
      expect(b.disabled, `${state}: never disabled`).toBe(false)
      expect(b.className, `${state}: secondary`).toContain('secondary')
    }
    const status = tileStatus(tile)
    expect(status.textContent, 'empty until an action').toBe('')
    expect(before(row, status), 'the status line is under the buttons').toBe(
      true,
    )
    expect(tileAlerts(tile), 'no alert until there is text').toEqual([])
  },
)

test('24 A1.1: the tile verbs carry the instance page icons (play, square, rotate-cw), are 40 px high (min-height: var(--control-h), 40px) and sit in a row that wraps inside the tile (flex-wrap: wrap)', async () => {
  const { main } = await open({ instances: list('active') })
  const tile = tileOf(main, 'default')
  const row = runRow(tile, 'default')

  // 40 px: the global button rule, the same the instance page verbs use.
  const buttonRules = TOP.filter((r) => r.compact === 'button')
  expect(buttonRules.length, 'one global button rule').toBe(1)
  expect(
    prop(buttonRules[0]!.body, 'min-height'),
    'the verbs are 40 px high',
  ).toBe('var(--control-h)')
  const roots = TOP.filter((r) => r.compact === ':root')
  expect(roots.length, 'one :root rule').toBe(1)
  expect(prop(roots[0]!.body, '--control-h')).toBe('40px')

  // Wrapping: the rule of the row's own class wraps.
  const classes = [...row.classList]
  expect(classes.length, 'the run row carries a class').toBeGreaterThan(0)
  const rowRules = TOP.filter((r) =>
    classes.some((c) => r.compact.includes(`.${c}`)),
  )
  expect(
    rowRules.some((r) => /(^|;)\s*flex-wrap:\s*wrap/.test(r.body)),
    'the row wraps inside the tile',
  ).toBe(true)

  // The same icons as the instance page: the same svg, verb by verb.
  const svgs: Record<string, string> = {}
  for (const key of ['run-start', 'run-stop', 'run-restart']) {
    const svg = tileButton(tile, key, 'default').querySelector('svg')
    expect(svg, `the tile's ${key} carries an icon`).not.toBeNull()
    svgs[key] = svg!.outerHTML
  }
  close()
  const page = await open({ instances: list('active') }, '#/instances/default')
  for (const key of ['run-start', 'run-stop', 'run-restart']) {
    const verb = within(page.main).getByRole('button', { name: en(key) })
    expect(
      verb.querySelector('svg')?.outerHTML,
      `${key}: the tile's icon is the instance page's`,
    ).toBe(svgs[key])
  }
})

// ── 23 A1.2: one write at a time, its answer under the buttons ──────────

test('24 A1.2: one write at a time per tile: while it runs the tile verbs are aria-disabled, its status under the buttons reads run-starting and focus stays on the pressed verb; the other tile is untouched and its own write goes', async () => {
  const gate = deferred()
  const { bff, main } = await open({
    instances: list('inactive'),
    run: () => gate.promise,
  })
  const tile = tileOf(main, 'default')
  const start = tileButton(tile, 'run-start', 'default')
  press(start)
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/start'])
  for (const b of tileButtons(tile, 'default'))
    expect(inactive(b), 'the tile verbs are aria-disabled while writing').toBe(
      true,
    )
  const row = runRow(tile, 'default')
  const status = tileStatus(tile)
  expect(status.textContent).toBe(en('run-starting'))
  expect(before(row, status), 'the answer is under the buttons').toBe(true)
  expect(document.activeElement, 'focus stays on the pressed verb').toBe(start)

  // The other tile is untouched: its Start is active and its write goes.
  const coder = tileOf(main, 'coder')
  const coderStart = tileButton(coder, 'run-start', 'coder')
  expect(inactive(coderStart), 'the other tile is not disabled').toBe(false)
  press(coderStart)
  await flush()
  expect(posts(bff), 'one write per tile, not one per page').toEqual([
    '/api/instances/default/start',
    '/api/instances/coder/start',
  ])
  expect(tileStatus(coder).textContent).toBe(en('run-starting'))
})

test('24 A1.2: Start settles on the shared store fresh read: one fresh GET, the badge redraws Running, the status reads run-started, the verbs follow the new state, focus stays on Start, and the Overview sends no poll', async () => {
  const { bff, main } = await open({ instances: list('inactive') })
  // The read the write asks for answers active (the manager started it).
  bff.setInstances(list('active'))
  const tile = tileOf(main, 'default')
  const start = tileButton(tile, 'run-start', 'default')
  press(start)
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/start'])
  expect(
    reads(bff),
    'the write asked the shared store for one fresh read',
  ).toBe(2)
  expect(badgeText(tile)).toContain(en('instance-state-active'))
  expect(tileStatus(tile).textContent).toBe(en('run-started'))
  expect(
    inactiveSet(tile, 'default'),
    'active: Start aria-disabled, Stop and Restart active',
  ).toEqual([true, false, false])
  expect(document.activeElement, 'focus stayed on the pressed verb').toBe(start)
  expect(tileAlerts(tile)).toEqual([])
  // The Overview is not polled (1c.41): the settling read is the last.
  await advance(10_000)
  expect(reads(bff), 'no poll of its own').toBe(2)
})

test('24 A1.2: a failed write answers in the tile alert under the buttons with the instance page text; the status stays empty, focus stays on the pressed verb, the verbs follow the last state', async () => {
  const { main } = await open({
    instances: list('inactive'),
    run: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  const tile = tileOf(main, 'default')
  const start = tileButton(tile, 'run-start', 'default')
  press(start)
  await flush()
  expect(tileAlerts(tile)).toEqual([en('api-client-unavailable')])
  const alert = within(tile).getByRole('alert')
  expect(
    before(runRow(tile, 'default'), alert),
    'the alert is under the buttons',
  ).toBe(true)
  expect(tileStatus(tile).textContent, 'the status is emptied').toBe('')
  expect(document.activeElement, 'focus stays on the pressed verb').toBe(start)
  expect(
    inactiveSet(tile, 'default'),
    'the verbs follow the last state (inactive)',
  ).toEqual([false, true, true])
})

test('24 A1.2: a 504 on a tile re-reads the instances at once and redraws the badge', async () => {
  // The 504 client-timeout reached the manager (16 17f.10): the write may
  // have happened, so the tile reads the list now, not at a later poll.
  const { bff, main } = await open({
    instances: list('inactive'),
    run: () => reply(504, { key: 'client-timeout', args: [] }),
  })
  // The next GET answers active: the manager did start it, the 504 lost only
  // the answer.
  bff.setInstances(list('active'))
  const tile = tileOf(main, 'default')
  const start = tileButton(tile, 'run-start', 'default')
  press(start)
  await flush()
  expect(tileAlerts(tile), 'the tile shows the not-confirmed text').toEqual([
    en('write-timeout'),
  ])
  expect(
    reads(bff),
    'exactly one fresh GET /api/instances follows right away',
  ).toBe(2)
  expect(badgeText(tile), 'the badge follows the manager').toContain(
    en('instance-state-active'),
  )
  expect(
    inactiveSet(tile, 'default'),
    'active: Start aria-disabled, Stop and Restart live',
  ).toEqual([true, false, false])
  // The fresh read is the 504's own, not a poll: the Overview is not polled
  // (1c.41), so the clock brings nothing more.
  await advance(10_000)
  expect(reads(bff), 'no poll of its own').toBe(2)
})

test('24 A1.2 D2: a Start the manager accepted, with the fresh read still saying Starting, leaves the tile status reading run-requested with the run-open-page link, not empty', async () => {
  // The Overview is not polled (1c.41), so the write's one fresh read usually
  // finds the instance starting: `settledText` gives nothing and the line was
  // left empty (24 A1.2 D2). The line says the write went, and that the state
  // shown is as of now.
  const { bff, main } = await open({ instances: list('inactive') })
  bff.setInstances(list('activating'))
  const tile = tileOf(main, 'default')
  const start = tileButton(tile, 'run-start', 'default')
  press(start)
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/start'])
  expect(
    badgeText(tile),
    'the badge shows the state the fresh read gave',
  ).toContain(en('instance-state-activating'))
  const status = tileStatus(tile)
  expect(
    status.textContent,
    'the line says the write went: the state shown is as of now',
  ).toContain(en('run-requested'))
  const link = status.querySelector('a[href="#/instances/default"]')
  expect(link, 'the line carries the instance page link').not.toBeNull()
  expect(link!.textContent, 'the link says run-open-page').toBe(
    en('run-open-page'),
  )
  expect(tileAlerts(tile), 'an accepted write is not an alert').toEqual([])
  expect(document.activeElement, 'focus stays on the pressed verb').toBe(start)
})

// ── 23 A1.3: the names carry the instance ───────────────────────────────

test('24 A1.3: the tile verbs are named Start default, Stop default, Restart default: the visible word starts the name and the instance follows in a visually hidden span; the coder tile names its own', async () => {
  const { main } = await open({ instances: list('active') })
  const tile = tileOf(main, 'default')
  for (const key of ['run-start', 'run-stop', 'run-restart']) {
    const verb = tileButton(tile, key, 'default')
    expect(
      shownText(verb),
      `${key}: the visible word is the instance page's`,
    ).toBe(en(key))
    const hidden = verb.querySelector('.visually-hidden')
    expect(
      hidden,
      `${key}: the instance is in a visually hidden span`,
    ).not.toBeNull()
    expect(hidden!.textContent, `${key}: after the visible verb`).toBe(
      ' default',
    )
  }
  const coder = tileOf(main, 'coder')
  expect(
    shownText(tileButton(coder, 'run-start', 'coder')),
    'the coder tile shows the same word',
  ).toBe(en('run-start'))
  expect(
    tileButton(coder, 'run-start', 'coder').querySelector('.visually-hidden')
      ?.textContent,
    'and names its own instance',
  ).toBe(' coder')
})

// ── 23 A1.4: the 17c start refusal ──────────────────────────────────────

const NO_PROFILES = () =>
  reply(409, { key: 'instance-no-model-profiles', args: ['default'] })

test('24 A1.4: the 409 instance-no-model-profiles shows in the tile alert with the instance page text and a link to the instance page', async () => {
  // The instance page's text for the same refusal, read off the page.
  const page = await open(
    { instances: list('inactive'), run: NO_PROFILES },
    '#/instances/default',
  )
  press(within(page.main).getByRole('button', { name: en('run-start') }))
  await flush()
  const pageAlerts = within(page.main).queryAllByRole('alert')
  expect(pageAlerts, 'the page answers in its run alert').toHaveLength(1)
  const pageText = pageAlerts[0]!.textContent ?? ''
  expect(pageText, 'a text of its own').not.toBe('')
  close()

  // The tile: the same text, in its alert, with a link to the instance page.
  const { bff, main } = await open(
    { instances: list('inactive'), run: NO_PROFILES },
    '#/',
  )
  const tile = tileOf(main, 'default')
  const start = tileButton(tile, 'run-start', 'default')
  press(start)
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/start'])
  const alerts = within(tile).queryAllByRole('alert')
  expect(alerts, 'one alert in the tile').toHaveLength(1)
  expect(
    alerts[0]!.textContent,
    'the tile alert carries the instance page text',
  ).toContain(pageText)
  expect(
    alerts[0]!.querySelector('a[href="#/instances/default"]'),
    'the alert carries a link to the instance page',
  ).not.toBeNull()
  expect(
    before(runRow(tile, 'default'), alerts[0]!),
    'the alert is under the buttons',
  ).toBe(true)
  expect(document.activeElement, 'focus stays on the pressed verb').toBe(start)
})

test('24 A1.4 D1: the tile alert for the 409 instance-no-model-profiles is api-instance-no-model-profiles naming the instance by the name the tile shows, never the generic request-failed, and keeps the run-open-page link to the instance page', async () => {
  // "Try again" cannot help a start the manager refuses for want of a model
  // profile (24 A1.4 D1): the refusal has words of its own, naming the
  // instance as the tile names it.
  const { bff, main } = await open(
    { instances: namedList(), run: NO_PROFILES },
    '#/',
  )
  const tile = tileOf(main, 'coder')
  // The verb's accessible name carries the display name (23 A1.3).
  const start = tileButton(tile, 'run-start', 'Coding assistant')
  press(start)
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/coder/start'])
  const alerts = within(tile).queryAllByRole('alert')
  expect(alerts, 'one alert in the tile').toHaveLength(1)
  const text = alerts[0]!.textContent ?? ''
  expect(
    text,
    'the tile names the missing model profile, with the name the tile shows',
  ).toContain(en('api-instance-no-model-profiles', 'Coding assistant'))
  expect(text, 'not the generic text that says to try again').not.toContain(
    en('request-failed'),
  )
  const link = alerts[0]!.querySelector('a[href="#/instances/coder"]')
  expect(link, 'the link to the instance page stays').not.toBeNull()
  expect(link!.textContent, 'the link says run-open-page').toBe(
    en('run-open-page'),
  )
  expect(document.activeElement, 'focus stays on the pressed verb').toBe(start)
})
