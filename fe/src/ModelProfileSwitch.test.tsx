// Brief 24, slice A2 (fe/briefs/24-ux-audit.md, section A2, as revised
// 2026-11-25 for 22D4): the model profile page carries its own on/off switch,
// in the state line under the summary line, so a page reached from a search or
// a link no longer sends the user back to the list. The page, its read, its
// save and its Save bar Delete are ModelProfilePage.test.tsx,
// ModelProfileSave*.test.tsx and ModelProfileDelete*.test.tsx; the switch's own
// rules on the list's rows are ModelProfilesSwitch.test.tsx, whose switch,
// route and texts this line reuses. The strings keys are pinned by `1a strings`
// in App.test.tsx: the three keys 23 A2.1 and A2.3 add are in the test's EN
// map, and the two sentences 23 A2.1 drops are in that test's removed list.
//
// Not provable in jsdom, so not tested here: the look of the line and of the
// switch (the designer's review), and the 320 px and 200 % text widths the UX
// check measures.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const ROUTE = `/api/profiles/model/${encodeURIComponent(ID)}`
const SWITCH_ROUTE = `${ROUTE}/enabled`
const ENGINE = 'llama.cpp'
/** A profile whose id is NOT spelled the same raw and encoded (the 22D review's
 *  pin, carried here for the switch's own route). */
const SPACED = 'qwen 3-27B#7'
const SPACED_HASH = `#/profiles/model/${encodeURIComponent(SPACED)}`
const SPACED_ROUTE = `/api/profiles/model/${encodeURIComponent(SPACED)}`
/** The switch route of that id: `PUT .../enabled` (api-contract.md), the route
 *  the Models list uses, so the encoded id is pinned as its path segment. */
const SPACED_SWITCH_ROUTE = `${SPACED_ROUTE}/enabled`
/** The two sentences 23 A2.1 drops, as literals: their keys leave the strings
 *  file with this slice, so `en(key)` throws for them. */
const DROPPED_DELETE = 'To delete it, switch it off first.'
const DROPPED_LIST =
  'To serve it, switch it on in the list of all model profiles.'
/** The Embeddings card's mode select: the page's own field, used to make the
 *  page hold an edit (as 22 D.3 does). */
const MODE = en('model-profile-embedding-mode')

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
})

afterEach(async () => {
  // A link click queues its hashchange as a task: let it land on this test's
  // page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a reply landing) run. */
async function flushTasks(n = 3) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The document of a switched-off profile, with the two Embeddings keys set so
 *  its select is editable (23 A2.3 makes an edit). */
function offDoc(id = ID) {
  return reply(
    200,
    modelProfileDoc(
      { embeddings: 'true', pooling: 'mean' },
      { id, enabled: false },
    ),
  )
}

/** The document of a switched-on profile. */
function onDoc(id = ID) {
  return reply(
    200,
    modelProfileDoc(
      { embeddings: 'true', pooling: 'mean' },
      { id, enabled: true },
    ),
  )
}

/** The page, its document read as a switched-off profile, its cards drawn. */
async function loaded(options: BffOptions = {}, name = ID) {
  const bff = fakeBff({ modelProfile: (id) => offDoc(id), ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

/** Changes the Embeddings card's mode select: the page holds an edit. */
function makeDirty(main: HTMLElement) {
  const select = within(main).getByRole('combobox', { name: MODE })
  fireEvent.change(select, { target: { value: 'false' } })
}

/** True when the control is switched off by `aria-disabled` alone. */
function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

/** The page's own switch of 23 A2.1: the one `role="switch"` of the page. */
function switchOf(main: HTMLElement): HTMLInputElement {
  const all = within(main).queryAllByRole('switch') as HTMLInputElement[]
  expect(all, 'the page has exactly one switch').toHaveLength(1)
  return all[0]!
}

/** aria-labelledby, then a <label>, then the text. */
function nameOf(el: HTMLElement): string {
  const by = el.getAttribute('aria-labelledby')
  const text = by
    ? by
        .split(/\s+/)
        .map((id) => document.getElementById(id)?.textContent ?? '')
        .join(' ')
    : el instanceof HTMLInputElement && (el.labels?.length ?? 0) > 0
      ? [...el.labels!].map((l) => l.textContent).join(' ')
      : el.textContent
  return (text ?? '').replace(/\s+/g, ' ').trim()
}

/** The text `aria-describedby` answers, joined. */
function describedBy(el: HTMLElement): string {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .map((id) => document.getElementById(id)?.textContent ?? '')
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim()
}

/** The state line of 23 A2.1: the switch's ancestor that sits beside the
 *  summary line and the settings section, as the page's own children do. The
 *  brief names its place and its parts, never its tag. */
function stateLine(main: HTMLElement): HTMLElement {
  const summary = main.querySelector('p.model-profile-summary')
  expect(summary, 'the summary line is there').not.toBeNull()
  const page = summary!.parentElement
  expect(page, 'the page holds the summary line').not.toBeNull()
  let el: HTMLElement | null = switchOf(main)
  while (el !== null && el.parentElement !== page) el = el.parentElement
  expect(
    el,
    'the switch is in a line of the page, beside the summary line',
  ).not.toBeNull()
  return el!
}

/** The state line's own status place. */
function statusOf(main: HTMLElement): HTMLElement {
  const all = [
    ...stateLine(main).querySelectorAll<HTMLElement>('[role="status"]'),
  ]
  expect(all, 'the line has one status place').toHaveLength(1)
  return all[0]!
}

/** The state line's own alert places. */
function lineAlerts(main: HTMLElement): HTMLElement[] {
  return [...stateLine(main).querySelectorAll<HTMLElement>('[role="alert"]')]
}

function settingsSection(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  const section = h2.closest('section')
  expect(section, 'the settings section').not.toBeNull()
  return section as HTMLElement
}

function saveBar(main: HTMLElement): HTMLElement | null {
  return main.querySelector('.save-bar')
}

function deleteButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('model-profile-delete') })
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** The confirm group's accessible name: its question (22 D.2). */
const QUESTION = en('model-profile-delete-question', ID)

function queryConfirm(main: HTMLElement): HTMLElement | null {
  return within(main).queryByRole('group', { name: QUESTION })
}

async function confirmGroup(main: HTMLElement): Promise<HTMLElement> {
  press(deleteButton(main))
  return within(main).findByRole('group', { name: QUESTION })
}

function confirmIn(group: HTMLElement, key: string): HTMLElement {
  return within(group).getByRole('button', { name: en(key) })
}

/** The switch PUTs the page sent, each with the path it went to. */
function writes(bff: ReturnType<typeof fakeBff>): [string, unknown][] {
  return bff.calls
    .filter((c) => c.method === 'PUT' && c.path.endsWith('/enabled'))
    .map((c) => [c.path, c.body])
}

/** The document reads the page sent. */
function reads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', ROUTE).length
}

// ── 23 A2.1: the state line, its switch, its texts, its place ───────────

test('24 A2.1: the state line under the summary line holds the page’s own switch, named by the profile, the short text of its state and its own status place, outside the settings section and above the Save bar; the two sentences of 22 D4.9 are gone', async () => {
  const { main } = await loaded()
  const summary = main.querySelector('p.model-profile-summary')
  const sw = switchOf(main)
  const line = stateLine(main)
  // The switch is the list's row's: a native checkbox with role="switch", its
  // value from `checked`, never aria-checked, named by the profile it switches.
  expect(sw.getAttribute('type'), 'a native checkbox').toBe('checkbox')
  expect(sw.getAttribute('role')).toBe('switch')
  expect(sw.hasAttribute('aria-checked'), 'no aria-checked').toBe(false)
  expect(sw.checked, 'a switched-off profile').toBe(false)
  expect(sw.getAttribute('aria-label'), 'named by an element').toBeNull()
  expect(nameOf(sw), 'named by the profile the page shows').toBe(ID)
  // Its short text, and no trace of the two sentences that pointed elsewhere.
  expect(
    (within(line).getByText(en('model-profile-off-short')).textContent ?? '')
      .replace(/\s+/g, ' ')
      .trim(),
    'the short off text',
  ).toBe(en('model-profile-off-short'))
  const shown = main.textContent ?? ''
  expect(shown, 'the delete sentence is gone').not.toContain(DROPPED_DELETE)
  expect(shown, 'the list sentence is gone').not.toContain(DROPPED_LIST)
  expect(shown, 'the long paragraph of 22 D4.9 is gone').not.toContain(
    'This model profile is switched off',
  )
  // Its own feedback places: a status, empty, and no alert until there is one.
  expect(statusOf(main).textContent, 'the status place is empty').toBe('')
  expect(lineAlerts(main), 'no alert at rest').toHaveLength(0)
  // Its place: outside the settings section, under the summary line, above the
  // Save bar (status beside its control, principle 10).
  const section = settingsSection(main)
  expect(line.closest('section'), 'outside the settings section').toBeNull()
  expect(before(summary!, line), 'under the summary line').toBe(true)
  expect(before(line, section), 'above the settings section').toBe(true)
  const bar = saveBar(main)
  expect(bar, 'the Save bar is there').not.toBeNull()
  expect(before(line, bar!), 'above the Save bar').toBe(true)
  cleanup()
  vi.unstubAllGlobals()
  // A switched-on profile: the same line, its own text.
  const on = await loaded({ modelProfile: () => onDoc() })
  expect(switchOf(on.main).checked, 'a switched-on profile').toBe(true)
  expect(
    (
      within(stateLine(on.main)).getByText(en('model-profile-on-short'))
        .textContent ?? ''
    )
      .replace(/\s+/g, ' ')
      .trim(),
    'the short on text',
  ).toBe(en('model-profile-on-short'))
  expect(
    within(on.main).queryByText(en('model-profile-off-short')),
    'the off text is not shown',
  ).toBeNull()
  const onShown = on.main.textContent ?? ''
  expect(onShown, 'the delete sentence is gone').not.toContain(DROPPED_DELETE)
  expect(onShown, 'the long paragraph is gone').not.toContain(
    'This model profile is switched on',
  )
})

test('24 A2.1: the switch sends PUT {enabled} to the id percent-encoded as a path segment, and its line’s status says the list’s texts, with the restart note when the engine runs', async () => {
  const { bff, main } = await loaded({
    modelProfileSwitch: () => reply(200, { enabled: true, running: false }),
  })
  const sw = switchOf(main)
  press(sw)
  await flushTasks()
  expect(writes(bff), 'one write, to the encoded route').toEqual([
    [SWITCH_ROUTE, { enabled: true }],
  ])
  expect(sw.checked, 'the reply’s value').toBe(true)
  expect(statusOf(main).textContent, 'the list’s text').toBe(
    en('model-profiles-switched-on', ID, ENGINE),
  )
  expect(lineAlerts(main), 'no alert').toHaveLength(0)
  expect(document.activeElement, 'the focus stays on the switch').toBe(sw)
  expect(reads(bff), 'the document is read again after the switch').toBe(2)
  cleanup()
  vi.unstubAllGlobals()
  // The restart note: the engine is running.
  const on = await loaded({
    modelProfile: () => onDoc(),
    modelProfileSwitch: () => reply(200, { enabled: false, running: true }),
  })
  press(switchOf(on.main))
  await flushTasks()
  expect(statusOf(on.main).textContent, 'the restart note').toBe(
    en('model-profiles-switched-off-restart', ID, ENGINE),
  )
  cleanup()
  vi.unstubAllGlobals()
  // An id that is not spelled the same raw and encoded.
  startAt(SPACED_HASH)
  const spaced = await loaded({}, SPACED)
  press(switchOf(spaced.main))
  await flushTasks()
  expect(writes(spaced.bff), 'the path is the encoded id').toEqual([
    [SPACED_SWITCH_ROUTE, { enabled: true }],
  ])
})

// ── 23 A2.2: the switch drives the Save bar's Delete ────────────────────

test('24 A2.2: switching on makes the Save bar’s Delete aria-disabled and shows model-profiles-delete-needs-off; switching off makes it active and removes that line; the focus stays on the switch', async () => {
  const { main } = await loaded({
    modelProfileSwitch: () => reply(200, { enabled: true, running: false }),
  })
  const del = deleteButton(main)
  expect(inactive(del), 'a switched-off profile: the Delete can act').toBe(
    false,
  )
  expect(
    within(main).queryByText(en('model-profiles-delete-needs-off')),
    'and the bar shows no reason line',
  ).toBeNull()
  const sw = switchOf(main)
  press(sw)
  await flushTasks()
  const after = deleteButton(main)
  expect(inactive(after), 'switched on: the Delete cannot act').toBe(true)
  const reason = within(main).getByText(en('model-profiles-delete-needs-off'))
  expect(reason.id, 'the reason line has an id').not.toBe('')
  expect(after.getAttribute('aria-describedby'), 'the Delete names it').toBe(
    reason.id,
  )
  expect(document.activeElement, 'the focus stayed on the switch').toBe(sw)
  cleanup()
  vi.unstubAllGlobals()
  // The way back: a switched-on profile starts with the reason line, and
  // switching off takes it away.
  const on = await loaded({
    modelProfile: () => onDoc(),
    modelProfileSwitch: () => reply(200, { enabled: false, running: false }),
  })
  expect(
    inactive(deleteButton(on.main)),
    'a switched-on profile: the Delete cannot act',
  ).toBe(true)
  expect(within(on.main).getByText(en('model-profiles-delete-needs-off')))
  const sw2 = switchOf(on.main)
  press(sw2)
  await flushTasks()
  expect(
    inactive(deleteButton(on.main)),
    'switched off: the Delete acts again',
  ).toBe(false)
  expect(
    within(on.main).queryByText(en('model-profiles-delete-needs-off')),
    'the reason line is removed',
  ).toBeNull()
  expect(document.activeElement, 'the focus stayed on the switch').toBe(sw2)
})

test('24 A2.2: a switch-on with the confirm open closes the confirm first, and the focus stays on the switch', async () => {
  const { bff, main } = await loaded({
    modelProfileSwitch: () => reply(200, { enabled: true, running: false }),
  })
  const group = await confirmGroup(main)
  expect(group.isConnected, 'the confirm is open').toBe(true)
  const sw = switchOf(main)
  press(sw)
  await flushTasks()
  expect(queryConfirm(main), 'the confirm closed').toBeNull()
  expect(inactive(deleteButton(main)), 'the Delete cannot act').toBe(true)
  expect(document.activeElement, 'the focus stayed on the switch').toBe(sw)
  expect(writes(bff), 'one write').toHaveLength(1)
})

// ── 23 A2.3: the dirty rule ─────────────────────────────────────────────

test('24 A2.3: while the page is dirty the switch is aria-disabled and described by model-profile-switch-dirty, shown as a line in the state line; a press sends nothing; Discard makes it active again', async () => {
  const { bff, main } = await loaded()
  makeDirty(main)
  const sw = switchOf(main)
  expect(inactive(sw), 'dirty: the switch cannot act').toBe(true)
  const line = stateLine(main)
  expect(
    (within(line).getByText(en('model-profile-switch-dirty')).textContent ?? '')
      .replace(/\s+/g, ' ')
      .trim(),
    'the line says why, in the state line',
  ).toBe(en('model-profile-switch-dirty'))
  expect(describedBy(sw), 'the switch is described by that line').toContain(
    en('model-profile-switch-dirty'),
  )
  press(sw)
  await flushTasks()
  expect(writes(bff), 'a press sends nothing').toHaveLength(0)
  expect(sw.checked, 'and changes nothing').toBe(false)
  press(discardButton(main))
  await flushTasks()
  expect(inactive(switchOf(main)), 'clean: the switch acts again').toBe(false)
  expect(
    within(main).queryByText(en('model-profile-switch-dirty')),
    'the line is gone',
  ).toBeNull()
})

test('24 A2.3: after a switch the page reads the document again, and the next Save carries the revision that read gave', async () => {
  let n = 0
  const { bff, main } = await loaded({
    modelProfile: (id) => {
      n++
      return reply(
        200,
        modelProfileDoc(
          { embeddings: 'true', pooling: 'mean' },
          { id, enabled: n === 1, revision: n === 1 ? 'r-1' : 'r-2' },
        ),
      )
    },
    modelProfileSwitch: () => reply(200, { enabled: false, running: false }),
  })
  const sw = switchOf(main)
  expect(sw.checked, 'the first read says on').toBe(true)
  press(sw)
  await flushTasks()
  expect(writes(bff), 'the switch wrote off').toEqual([
    [SWITCH_ROUTE, { enabled: false }],
  ])
  expect(reads(bff), 'the document is read again').toBe(2)
  expect(switchOf(main).checked, 'the switch shows the read’s value').toBe(
    false,
  )
  makeDirty(main)
  press(saveButton(main))
  await flushTasks()
  const saved = bff.to('PUT', ROUTE)
  expect(saved, 'one save').toHaveLength(1)
  expect(
    (saved[0]!.body as { revision?: string }).revision,
    'the save carries the revision of the read the switch brought',
  ).toBe('r-2')
})

// ── 23 A2.4: one write at a time ────────────────────────────────────────

test('24 A2.4: while the switch’s write runs the Save bar’s Delete and Save are aria-disabled, and a press of either sends nothing', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ modelProfileSwitch: () => gate.promise })
  const sw = switchOf(main)
  press(sw)
  await flushTasks()
  expect(writes(bff), 'the write is open').toHaveLength(1)
  expect(statusOf(main).textContent, 'the line says nothing yet').toBe('')
  const del = deleteButton(main)
  const save = saveButton(main)
  expect(inactive(del), 'Delete is inactive while the switch writes').toBe(true)
  expect(inactive(save), 'Save is inactive while the switch writes').toBe(true)
  press(del)
  await flushTasks()
  expect(queryConfirm(main), 'the inactive Delete opens nothing').toBeNull()
  press(save)
  await flushTasks()
  expect(bff.to('PUT', ROUTE), 'the inactive Save sends nothing').toHaveLength(
    0,
  )
  gate.resolve(reply(200, { enabled: true, running: false }))
  await flushTasks()
  expect(writes(bff), 'and nothing was sent twice').toHaveLength(1)
  expect(statusOf(main).textContent, 'the write answered').toBe(
    en('model-profiles-switched-on', ID, ENGINE),
  )
})

test('24 A2.4: while a Save runs, and while a Delete runs, the switch is aria-disabled and a press sends nothing and changes nothing', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ modelProfileSave: () => gate.promise })
  makeDirty(main)
  press(saveButton(main))
  await flushTasks()
  expect(bff.to('PUT', ROUTE), 'the save is open').toHaveLength(1)
  const sw = switchOf(main)
  expect(inactive(sw), 'the switch is inactive while the save runs').toBe(true)
  press(sw)
  await flushTasks()
  expect(writes(bff), 'a press sends nothing').toHaveLength(0)
  expect(sw.checked, 'and changes nothing').toBe(false)
  gate.resolve(reply(200, { revision: 'r-2', running: false }))
  await flushTasks()
  expect(inactive(switchOf(main)), 'active again once the save lands').toBe(
    false,
  )
  cleanup()
  vi.unstubAllGlobals()
  // The same while a Delete runs.
  const del = deferred()
  const second = await loaded({ modelProfileDelete: () => del.promise })
  const group = await confirmGroup(second.main)
  press(confirmIn(group, 'confirm-delete'))
  await flushTasks()
  expect(second.bff.to('DELETE', ROUTE), 'the delete is open').toHaveLength(1)
  const sw2 = switchOf(second.main)
  expect(inactive(sw2), 'the switch is inactive while the delete runs').toBe(
    true,
  )
  press(sw2)
  await flushTasks()
  expect(writes(second.bff), 'a press sends nothing').toHaveLength(0)
  expect(sw2.checked, 'and changes nothing').toBe(false)
  del.resolve(reply(204))
  await flushTasks()
  expect(within(second.main).getByText(en('model-profile-deleted', ID)))
})

// ── A2 review defects (fe/briefs/24-ux-audit.md, "A2 review") ───────────

test('24 A2.4 D1: Save is held while the switch writes', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ modelProfileSwitch: () => gate.promise })
  const sw = switchOf(main)
  press(sw)
  await flushTasks()
  expect(writes(bff), 'the switch write is open').toHaveLength(1)
  // The review: an edit lands during the write, and Save turns active.
  makeDirty(main)
  const save = saveButton(main)
  expect(
    inactive(save),
    'an edit during the switch write: Save stays inactive',
  ).toBe(true)
  press(save)
  await flushTasks()
  expect(
    bff.to('PUT', ROUTE),
    'a press of Save sends no save while the switch writes',
  ).toHaveLength(0)
  gate.resolve(reply(200, { enabled: true, running: false }))
  await flushTasks()
  const after = saveButton(main)
  expect(inactive(after), 'the answer lets Save act').toBe(false)
  press(after)
  await flushTasks()
  expect(
    bff.to('PUT', ROUTE),
    'the save goes out once the switch write has answered',
  ).toHaveLength(1)
})

test('24 A2.4 D2: Delete’s reason is not shown for a profile that is off', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    modelProfile: () => onDoc(),
    modelProfileSwitch: () => gate.promise,
  })
  expect(
    within(main).getByText(en('model-profiles-delete-needs-off')),
    'a switched-on profile: the reason line is there',
  )
  press(switchOf(main))
  await flushTasks()
  expect(writes(bff), 'the switch-off write is open').toHaveLength(1)
  const del = deleteButton(main)
  expect(inactive(del), 'Delete stays held during the write').toBe(true)
  expect(
    within(main).queryByText(en('model-profiles-delete-needs-off')),
    'the switch shows off: the reason line is not shown',
  ).toBeNull()
  expect(
    del.getAttribute('aria-describedby'),
    'and the held Delete carries no description',
  ).toBeNull()
  gate.resolve(reply(200, { enabled: false, running: false }))
  await flushTasks()
  expect(
    within(main).queryByText(en('model-profiles-delete-needs-off')),
    'the reason stays away once the write answered',
  ).toBeNull()
})

test('24 A2.1 D3: a failed switch write adopts the re-read revision', async () => {
  let n = 0
  const { bff, main } = await loaded({
    modelProfile: (id) => {
      n++
      return reply(
        200,
        modelProfileDoc(
          { embeddings: 'true', pooling: 'mean' },
          { id, enabled: n === 1, revision: n === 1 ? 'r-1' : 'r-2' },
        ),
      )
    },
    modelProfileSwitch: () => reply(504, { key: 'client-timeout', args: [] }),
  })
  const sw = switchOf(main)
  expect(sw.checked, 'the first read says on, revision r-1').toBe(true)
  press(sw)
  await flushTasks()
  expect(writes(bff), 'the switch wrote off').toEqual([
    [SWITCH_ROUTE, { enabled: false }],
  ])
  expect(reads(bff), 'the page reads the document again').toBe(2)
  expect(switchOf(main).checked, 'the re-read says the write landed').toBe(
    false,
  )
  makeDirty(main)
  press(saveButton(main))
  await flushTasks()
  const saved = bff.to('PUT', ROUTE)
  expect(saved, 'one save').toHaveLength(1)
  expect(
    (saved[0]!.body as { revision?: string }).revision,
    'the save carries the revision of the re-read, not the old one',
  ).toBe('r-2')
})

const REFUSALS: [string, () => Response | Promise<Response>, string][] = [
  [
    '503 client-unavailable',
    () => reply(503, { key: 'client-unavailable', args: [] }),
    en('api-client-unavailable'),
  ],
  [
    '401 session-required',
    () => reply(401, { key: 'session-required', args: [] }),
    en('session-ended-retry'),
  ],
  [
    '403 session-not-administrator',
    () => reply(403, { key: 'session-not-administrator', args: [] }),
    en('session-revoked'),
  ],
  [
    '404 no-model-profile-named',
    () => reply(404, { key: 'no-model-profile-named', args: [ID] }),
    en('api-no-model-profile-named', ID),
  ],
  [
    '422 a key the fe does not know',
    () =>
      reply(422, { key: 'model-profile-no-model', args: ['secret-detail'] }),
    en('api-other', 'model-profile-no-model'),
  ],
  ['500 not json', () => reply(500, 'secret-detail'), en('request-failed')],
  [
    'network',
    () => Promise.reject(new TypeError('secret-detail')),
    en('request-failed'),
  ],
]

test.each(REFUSALS)(
  '24 A2.4: a refusal of the switch goes in the state line’s alert, with the text the list’s row maps for the same route, and the switch keeps its value: %s',
  async (what, write, text) => {
    const { bff, main } = await loaded({ modelProfileSwitch: write })
    const sw = switchOf(main)
    press(sw)
    await flushTasks()
    expect(writes(bff), what).toHaveLength(1)
    const alerts = lineAlerts(main)
    expect(alerts, `${what}: one alert, in the state line`).toHaveLength(1)
    expect(
      (alerts[0]!.textContent ?? '').replace(/\s+/g, ' ').trim(),
      what,
    ).toBe(text)
    expect(statusOf(main).textContent, `${what}: the status says nothing`).toBe(
      '',
    )
    expect(sw.checked, `${what}: the value before`).toBe(false)
    expect(inactive(sw), `${what}: active again`).toBe(false)
    expect(document.activeElement, `${what}: the focus stays`).toBe(sw)
    expect(document.body.textContent, what).not.toContain('secret-detail')
  },
)
