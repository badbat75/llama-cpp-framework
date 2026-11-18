// Saving a model profile (fe/briefs/02h-model-profile-save.md, sub-slice 2h.1, criteria 135
// to 141): the save bar of the Server page (D5.2) at the end of the settings,
// Discard, the PUT and its pending state, what a 200 leaves loaded and what it
// says, every answer that is not a shaped 200, leaving with unsaved edits
// through the app's discard dialog, and the tab order. The editable fields,
// the revision, dirty and what a changed field sends are 130 to 134, in
// ModelProfileSave1.test.tsx, whose helpers this file repeats (a test file imports
// no other test file). The keyed answers (409, 422, 401) are 2h.2 and 2h.3.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, expect, it, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const MODE = en('model-profile-embedding-mode')
const POOLING = en('model-profile-pooling')

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
})

afterEach(async () => {
  // A link click queues its hashchange as a task: let it land on this test's
  // page, not on the next test's (as ServerSave.test.tsx).
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers (as ModelProfileSave1.test.tsx) ───────────────────────────────────

/** The app at the address in the location; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** Waits for the loaded page: the h1 of the model and the Assets card. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = open(options)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The loaded page with `values` holding exactly `values`. */
function page(values: Record<string, string>) {
  return loaded({
    modelProfile: () => reply(200, modelProfileDoc({}, { values })),
  })
}

/** An element's text, with the page's runs of whitespace folded. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

/** The written-as line of `key` (2e's 124). */
function writtenOf(main: HTMLElement, key: string): HTMLElement | null {
  return main.querySelector(`#model-profile-written-${key}`)
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** True when the button cannot be used: disabled, or aria-disabled. */
function inactive(el: HTMLElement): boolean {
  return (
    (el as HTMLButtonElement).disabled ||
    el.getAttribute('aria-disabled') === 'true'
  )
}

/** The save bar's status line: the only role="status" below the h2 of the
 * settings (the page's loading status is above it). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line once the page is loaded').toHaveLength(1)
  return all[0]!
}

/** Clicks Save changes and waits for the n-th PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(n))
}

/** The whole body of the n-th recorded PUT of this model profile. */
function sentBody(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  return bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
}
// ── Helpers for the bar, the answers and the leave guard (135 to 141) ───

/** The save bar of the settings section (135). */
function bar(main: HTMLElement): HTMLElement {
  const el = main.querySelector('.save-bar')
  expect(el, 'the save bar, the same one the Server page has').toBeTruthy()
  return el as HTMLElement
}

/** The bar's alert reads `text` (exact): a role="alert" of its own, after the
 * status line and before the buttons (135, 01's 30a). */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
  const line = status(main)
  expect(line.contains(alert), `${what}: not inside the status line`).toBe(
    false,
  )
  expect(before(line, alert!), `${what}: after the status line`).toBe(true)
  expect(
    before(alert!, discardButton(main)),
    `${what}: before the buttons`,
  ).toBe(true)
}

/** The bar's alert is not rendered (01's 30a: only while it has text). */
function expectNoAlert(main: HTMLElement, what: string) {
  expect(
    within(main)
      .queryAllByRole('alert')
      .map((a) => a.textContent),
    what,
  ).toEqual([])
}

/** The loaded page with revision `r1`, `values`, and `answer` for the save. */
function form(
  values: Record<string, string>,
  answer?: BffOptions['modelProfileSave'],
) {
  return loaded({
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { revision: 'r1', values })),
    modelProfileSave: answer,
  })
}

/** Makes the form dirty: Embedding mode picked to the option it does NOT show.
 * 132: a select's state is its selected option, and 2e's 93 gives the off / on
 * select `off` when the file has no `embeddings` key, so a fixed `off` is no
 * change on such a read (and no change after a save that sent `off`): the pick
 * must always be the other option. */
function makeDirty(main: HTMLElement) {
  const select = selectOf(main, MODE)
  pick(main, MODE, select.value === 'true' ? 'false' : 'true')
}

/** Lets jsdom's queued tasks (a hashchange, a history traversal) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** Types `hash` in the address bar: jsdom fires the hashchange in a task. */
async function typeHash(hash: string) {
  await act(async () => {
    window.location.hash = hash
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** The browser's Back button: jsdom traverses in a task, not synchronously. */
async function pressBack() {
  await act(async () => {
    window.history.back()
    await new Promise((r) => setTimeout(r, 0))
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** The browser's Forward button. */
async function pressForward() {
  await act(async () => {
    window.history.forward()
    await new Promise((r) => setTimeout(r, 0))
    await new Promise((r) => setTimeout(r, 0))
  })
}

function navLink(name: string): HTMLElement {
  return within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).getByRole('link', { name })
}

/** The nav with its instance links in (they come with the instances read). */
async function navReady(): Promise<HTMLElement> {
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  await within(nav).findByRole('link', { name: 'coder' })
  return nav
}

function signOutButton(): HTMLElement {
  return within(screen.getByRole('banner')).getByRole('button', {
    name: en('session-sign-out'),
  })
}

/** The product link in the banner (01's 38). */
function productLink(): HTMLElement {
  return within(screen.getByRole('banner')).getByRole('link', {
    name: en('app-name'),
  })
}

/** The discard dialog of 01's 38, opened by the app with showModal(). */
async function discardDialog(): Promise<HTMLElement> {
  const d = await screen.findByRole('dialog', { name: en('discard-title') })
  expect(d.tagName, 'a native <dialog>').toBe('DIALOG')
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  return d
}

function keepButton(d: HTMLElement): HTMLElement {
  return within(d).getByRole('button', { name: en('discard-keep') })
}

function confirmButton(d: HTMLElement): HTMLElement {
  return within(d).getByRole('button', { name: en('discard-confirm') })
}

/** True when a closing tab is blocked, so the beforeunload is armed. */
function unloadBlocked(): boolean {
  const e = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(e)
  return e.defaultPrevented
}

/** The model profile page reached by a real navigation, so Back has an entry. */
async function modelProfileAfterNavigation() {
  startAt('#/profiles/model')
  const bff = fakeBff()
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  await typeHash(HASH)
  await within(main).findByRole('heading', { level: 1, name: ID })
  return { bff, main }
}

// ── 135: the save bar ───────────────────────────────────────────────────

test("2h.135: the save bar is the settings section's last child, after the last card: status, no alert, Discard, Save changes, both aria-disabled while clean and inert to a click", async () => {
  const { bff, main } = await page({ embeddings: 'true', pooling: 'mean' })
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  const section = h2.parentElement!
  const b = bar(main)
  expect(section.contains(b), 'the bar is inside the settings section').toBe(
    true,
  )
  expect(section.lastElementChild, 'the last child of the section').toBe(b)
  const cards = [...section.querySelectorAll('.card')]
  expect(cards.length, 'the cards are before it').toBeGreaterThan(5)
  expect(before(cards[cards.length - 1]!, b), 'after the last card').toBe(true)
  const line = status(main)
  const save = saveButton(main)
  const discard = discardButton(main)
  expect(b.contains(line), 'the status line is in the bar').toBe(true)
  expect(line.textContent, 'clean: no status text').toBe('')
  expectNoAlert(main, 'clean: no alert')
  expect(
    b.contains(save) && b.contains(discard),
    'both buttons are in the bar',
  ).toBe(true)
  expect(before(line, discard), 'status, then Discard').toBe(true)
  expect(before(discard, save), 'Discard, then Save changes').toBe(true)
  for (const el of [save, discard]) {
    expect(el.getAttribute('aria-disabled'), text(el)).toBe('true')
    // aria-disabled, not disabled: focus never drops (D5.2).
    expect(
      (el as HTMLButtonElement).disabled,
      `${text(el)}: not disabled`,
    ).toBe(false)
  }
  fireEvent.click(save)
  fireEvent.click(discard)
  await act(async () => {})
  expect(
    bff.to('PUT', PUT),
    'a click on a clean bar sends nothing',
  ).toHaveLength(0)
  expect(status(main).textContent, 'and says nothing').toBe('')
})

test("2h.135: the bar keeps the Server page's sticky CSS: sticky, with the page's scroll padding clear of it (D4)", () => {
  const css = readFileSync(
    join(import.meta.dirname, 'index.css'),
    'utf8',
  ).replace(/\/\*[\s\S]*?\*\//g, '')
  const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((m) =>
    m[1].includes('.save-bar'),
  )
  expect(rules.length, 'a rule for the save bar').toBeGreaterThan(0)
  expect(rules.map((m) => m[2]).join(' ')).toMatch(/position:\s*sticky/)
  expect(
    css,
    'a field focused near the bottom is not hidden behind the bar',
  ).toMatch(/html:has\(\.save-bar\)\s*\{[^{}]*scroll-padding-bottom:\s*calc\(/)
})

// ── 136: Discard ────────────────────────────────────────────────────────

test('2h.136: Discard puts both selects back to what the read gave, ends the bar alert, empties the status, keeps the focus on itself and sends no request', async () => {
  const { bff, main } = await form(
    { embeddings: 'true', pooling: 'mean' },
    () => reply(500, { key: 'secret-detail', args: [] }),
  )
  pick(main, MODE, 'false')
  pick(main, POOLING, 'rank')
  await save(main, bff)
  await within(main).findByText(en('save-failed'))
  expectAlert(main, en('save-failed'), 'the alert Discard must end')
  const discard = discardButton(main)
  discard.focus()
  fireEvent.click(discard)
  expect(
    selectOf(main, MODE).value,
    'Embedding mode back to the loaded on',
  ).toBe('true')
  expect(selectOf(main, POOLING).value, 'Pooling back to the loaded mean').toBe(
    'mean',
  )
  expectNoAlert(main, 'Discard ends the alert')
  expect(status(main).textContent, 'Discard empties the status').toBe('')
  expect(document.activeElement, 'focus stays on Discard').toBe(discard)
  expect(discard.getAttribute('aria-disabled'), 'clean again').toBe('true')
  // The only request that ever left is the failed PUT of the fixture.
  expect(
    bff.calls.filter((c) => c.method !== 'GET'),
    'Discard sends nothing',
  ).toHaveLength(1)
})

// ── 137: the save and its pending state ─────────────────────────────────

test('2h.137: Save sends PUT /api/profiles/model/<id> as JSON with the read revision and only the changed key', async () => {
  const { bff, main } = await form({ embeddings: 'true', pooling: 'mean' })
  pick(main, MODE, 'false')
  await save(main, bff)
  expect(sentBody(bff), 'the body of the contract').toEqual({
    revision: 'r1',
    values: { embeddings: null },
  })
  // JSON in (api-contract.md): the bff reads a body only as application/json.
  const init = bff.fetch.mock.calls.find(([, i]) => i?.method === 'PUT')![1]!
  expect(new Headers(init.headers).get('content-type') ?? '').toMatch(
    /^application\/json\b/i,
  )
})

test('2h.137: while the PUT runs Save reads save-saving, both buttons are aria-disabled, a second click and a Discard click send nothing, and the selects stay editable', async () => {
  const gate = deferred()
  const { bff, main } = await form(
    { embeddings: 'true', pooling: 'mean' },
    () => gate.promise,
  )
  makeDirty(main)
  const button = saveButton(main)
  const discard = discardButton(main)
  fireEvent.click(button)
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
  expect(button.textContent, 'the button reads it').toBe(en('save-saving'))
  expect(status(main).textContent, 'the status line reads it too').toBe(
    en('save-saving'),
  )
  for (const el of [button, discard]) {
    expect(el.getAttribute('aria-disabled'), text(el)).toBe('true')
    expect(
      (el as HTMLButtonElement).disabled,
      `${text(el)}: not disabled`,
    ).toBe(false)
  }
  expect(selectOf(main, MODE).disabled, 'the fields stay editable').toBe(false)
  fireEvent.click(button)
  fireEvent.click(discard)
  await act(async () => {})
  expect(bff.to('PUT', PUT), 'a second click sends nothing').toHaveLength(1)
  expect(selectOf(main, MODE).value, 'Discard cannot be used mid-save').toBe(
    'false',
  )
  await act(async () => {
    gate.resolve(reply(200, { revision: 'r2', running: false }))
  })
  await waitFor(() => expect(button.textContent).toBe(en('save-button')))
})

// ── 138: the 200 ────────────────────────────────────────────────────────

test("2h.138: a 200 makes the sent values and the reply's revision the loaded ones: the selects show what was sent, running picks the message, focus stays on Save, the written-as line follows the new value and nothing is restarted or read again", async () => {
  const { bff, main } = await form(
    { embeddings: '0', pooling: 'mean' },
    (_b, _id, n) =>
      n === 1
        ? reply(200, { revision: 'r2', running: true })
        : reply(200, { revision: `r${n + 1}`, running: false }),
  )
  const line = status(main)
  const button = saveButton(main)
  expect(text(writtenOf(main, 'embeddings')!), 'the file spelling').toBe(
    en('model-profile-written-as', '0'),
  )
  pick(main, MODE, 'true')
  pick(main, POOLING, 'rank')
  button.focus()
  await save(main, bff)
  await waitFor(() => expect(line.textContent).toBe(en('save-saved-restart')))
  expect(sentBody(bff), 'both changed keys, the loaded revision').toEqual({
    revision: 'r1',
    values: { embeddings: 'true', pooling: 'rank' },
  })
  expect(selectOf(main, MODE).value, 'the select shows what was sent').toBe(
    'true',
  )
  expect(selectOf(main, POOLING).value, 'and so does Pooling').toBe('rank')
  expect(
    isHidden(writtenOf(main, 'embeddings')),
    'the new loaded value is the canonical one: no written-as line',
  ).toBe(true)
  for (const el of [saveButton(main), discardButton(main)])
    expect(el.getAttribute('aria-disabled'), `${text(el)}: clean`).toBe('true')
  expect(document.activeElement, 'focus stays on Save changes').toBe(button)
  // The reply's revision is the loaded one now, and rank is the loaded pooling.
  pick(main, POOLING, 'mean')
  expect(
    line.textContent,
    'a new change is dirty against the saved values',
  ).toBe(en('save-unsaved'))
  await save(main, bff, 2)
  expect(sentBody(bff, 2), 'the new revision goes out').toEqual({
    revision: 'r2',
    values: { pooling: 'mean' },
  })
  await waitFor(() => expect(line.textContent).toBe(en('save-saved')))
  // A key sent as null is gone from the loaded values: default shows, clean.
  pick(main, POOLING, 'default')
  await save(main, bff, 3)
  expect(sentBody(bff, 3), 'null removes the key').toEqual({
    revision: 'r3',
    values: { pooling: null },
  })
  await waitFor(() => expect(line.textContent).toBe(en('save-saved')))
  expect(
    selectOf(main, POOLING).value,
    'the removed key reads as default',
  ).toBe('default')
  // A save never restarts the server and never reads the file again.
  expect(
    bff.calls.filter((c) => c.method === 'POST'),
    'no restart',
  ).toEqual([])
  expect(bff.to('GET', `/api/profiles/model/${ID}`), 'no re-read').toHaveLength(
    1,
  )
})

test('2h.138: an edit made while the PUT runs stays on screen and is dirty against the values just saved; what was sent is the clean state now', async () => {
  const gate = deferred()
  const { bff, main } = await form({ pooling: 'mean' }, (_b, _id, n) =>
    n === 1 ? gate.promise : reply(200, { revision: 'r2', running: false }),
  )
  pick(main, POOLING, 'rank')
  await save(main, bff)
  pick(main, POOLING, 'cls')
  await act(async () => {
    gate.resolve(reply(200, { revision: 'r2', running: false }))
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-unsaved')))
  expect(selectOf(main, POOLING).value, 'what the user picked stays').toBe(
    'cls',
  )
  pick(main, POOLING, 'rank')
  expect(
    status(main).textContent,
    'what was sent is the loaded value: clean',
  ).toBe('')
  pick(main, POOLING, 'cls')
  await save(main, bff, 2)
  expect(sentBody(bff, 2), 'the held revision and the new change').toEqual({
    revision: 'r2',
    values: { pooling: 'cls' },
  })
})

// ── 139: every answer that is not the 200 of 138 ───────────────────────

/** 139: one test per row, one mount each (as 2h.150 in
 * ModelProfileSave3.test.tsx: the cleanup between rows is the file's afterEach). */
const ANSWERS_139: [string, () => Response | Promise<Response>][] = [
  ['500 with a key', () => reply(500, { key: 'secret-detail', args: [] })],
  ['204', () => reply(204)],
  ['200 without running', () => reply(200, { revision: 'r2' })],
  ['200 without a revision', () => reply(200, { running: false })],
  [
    '200 with a revision that is not a string',
    () => reply(200, { revision: 2, running: false }),
  ],
  ['200 that is not JSON', () => reply(200, 'secret-detail')],
  ['a network failure', () => Promise.reject(new TypeError('Failed to fetch'))],
]

it.each(ANSWERS_139)(
  '2h.139: %s: any answer that is not a shaped 200 shows save-failed in the bar alert, puts the status back to save-unsaved and keeps the edits, dirty',
  async (what, answer) => {
    const { bff, main } = await form(
      { embeddings: 'true', pooling: 'mean' },
      answer,
    )
    makeDirty(main)
    await save(main, bff)
    await within(main).findByText(en('save-failed'))
    expectAlert(main, en('save-failed'), what)
    expect(document.body.textContent, what).not.toContain('secret-detail')
    expect(selectOf(main, MODE).value, `${what}: the edit stays`).toBe('false')
    expect(
      status(main).textContent,
      `${what}: the status is back to unsaved`,
    ).toBe(en('save-unsaved'))
    expect(
      inactive(saveButton(main)),
      `${what}: still dirty, Save is live`,
    ).toBe(false)
    expect(within(main).queryByText(en('save-saved')), what).toBeNull()
    expect(
      isHidden(writtenOf(main, 'embeddings')),
      `${what}: the field is still changed, so 134 keeps the written-as line hidden`,
    ).toBe(true)
  },
)

// ── 140: leaving while dirty ────────────────────────────────────────────

test('2h.140: model-profile-back and a nav link while the model profile form is dirty ask with the discard dialog: Keep editing keeps the page, the edit and the focus back on the link, Discard changes goes on and leaves nothing dirty', async () => {
  const { main } = await page({ embeddings: 'true', pooling: 'mean' })
  await navReady()
  const back = within(main).getByRole('link', {
    name: en('model-profile-back'),
  })
  back.focus()
  makeDirty(main)
  fireEvent.click(back)
  const d = await discardDialog()
  expect(within(d).getByText(en('discard-description'))).toBeTruthy()
  expect(
    before(keepButton(d), confirmButton(d)),
    'Keep editing, then Discard changes',
  ).toBe(true)
  expect(document.activeElement, 'Keep editing is focused').toBe(keepButton(d))
  fireEvent.click(keepButton(d))
  await flushTasks()
  expect(window.location.hash, 'the address is the model profile again').toBe(
    HASH,
  )
  expect(selectOf(main, MODE).value, 'the edit stays').toBe('false')
  expect(document.activeElement, 'focus back on the link').toBe(back)
  // A nav link asks the same; Discard changes goes on.
  const link = navLink(en('nav-model-profiles'))
  link.focus()
  fireEvent.click(link)
  const d2 = await discardDialog()
  expect(window.location.hash, 'the link is not followed yet').toBe(HASH)
  fireEvent.click(confirmButton(d2))
  await waitFor(() => expect(window.location.hash).toBe('#/profiles/model'))
  await within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  // The dropped edits leave nothing behind: leaving cleared the dirty state.
  fireEvent.click(navLink(en('nav-overview')))
  expect(screen.queryByRole('dialog'), 'the next link asks nothing').toBeNull()
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
})

test('2h.140: a typed hash and Back while the model profile form is dirty are put back and asked, the select and its edit kept; clean, they change the page and nothing is asked', async () => {
  const { main } = await modelProfileAfterNavigation()
  const select = selectOf(main, MODE)
  makeDirty(main)
  await typeHash('#/profiles/model')
  const d = await discardDialog()
  expect(
    window.location.hash,
    'the address is the model profile page again',
  ).toBe(HASH)
  expect(selectOf(main, MODE), 'the same form, not a remount').toBe(select)
  // The default fixture has no embeddings key: 2e.93 shows off, makeDirty
  // picks the other option, on.
  expect(select.value, 'the edit stays').toBe('true')
  expect(status(main).textContent, 'the bar status stays as it was').toBe(
    en('save-unsaved'),
  )
  expect(document.activeElement, 'Keep editing is focused').toBe(keepButton(d))
  fireEvent.click(keepButton(d))
  await flushTasks()
  expect(window.location.hash).toBe(HASH)
  expect(screen.queryByRole('dialog')).toBeNull()

  // Back while dirty, on a page of its own. The typed hash above cannot be
  // followed by a Back: the guard's put-back is a replaceState on the entry
  // the browser stands on, so the entry behind it holds the model profile hash too
  // and a traversal to it fires no hashchange in jsdom (the tests of
  // LeaveGuard.test.tsx press Back on a page reached by a navigation, which
  // leaves a Model profiles entry behind, as here).
  cleanup()
  vi.unstubAllGlobals()
  const second = await modelProfileAfterNavigation()
  const backSelect = selectOf(second.main, MODE)
  makeDirty(second.main)
  await pressBack()
  const d2 = await discardDialog()
  expect(window.location.hash, 'Back is put back too').toBe(HASH)
  expect(selectOf(second.main, MODE), 'the same form, not a remount').toBe(
    backSelect,
  )
  expect(selectOf(second.main, MODE).value, 'the edit stays').toBe('true')
  fireEvent.click(keepButton(d2))
  await flushTasks()
  expect(
    screen.queryByRole('dialog'),
    'Keep editing closes it, the model profile page stays',
  ).toBeNull()
  // Clean: the same two navigations go through, nobody asks.
  cleanup()
  vi.unstubAllGlobals()
  const third = await modelProfileAfterNavigation()
  await typeHash('#/profiles/model')
  expect(
    screen.queryByRole('dialog'),
    'clean: a typed hash asks nothing',
  ).toBeNull()
  await within(third.main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  await pressBack()
  expect(screen.queryByRole('dialog'), 'clean: Back asks nothing').toBeNull()
  await within(third.main).findByRole('heading', { level: 1, name: ID })
})

test('2h.140: a closing tab is blocked only while the model profile form is dirty, and Sign out asks first: Keep editing signs out nobody, Discard changes signs out', async () => {
  const { bff, main } = await form({ embeddings: 'true', pooling: 'mean' })
  expect(unloadBlocked(), 'clean on load').toBe(false)
  makeDirty(main)
  expect(unloadBlocked(), 'dirty').toBe(true)
  fireEvent.click(discardButton(main))
  expect(unloadBlocked(), 'after Discard').toBe(false)
  makeDirty(main)
  await save(main, bff)
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  expect(unloadBlocked(), 'after a save').toBe(false)
  // 01's 38 (1a.15): Sign out asks first.
  makeDirty(main)
  const signOut = signOutButton()
  signOut.focus()
  fireEvent.click(signOut)
  const d = await discardDialog()
  fireEvent.click(keepButton(d))
  await act(async () => {})
  expect(
    bff.to('DELETE', '/api/session'),
    'Keep signs out nobody',
  ).toHaveLength(0)
  expect(document.activeElement, 'focus back on Sign out').toBe(signOut)
  fireEvent.click(signOut)
  const d2 = await discardDialog()
  fireEvent.click(confirmButton(d2))
  await waitFor(() => expect(bff.to('DELETE', '/api/session')).toHaveLength(1))
})

// The report of 140 ends with the save: a page that has just saved has nothing
// to lose, so the app asks nobody on its way out. Sign out answers synchronously
// (the bar's buttons and the banner's), so this needs no navigation.
test('2h.140: a page that has just saved reports no unsaved changes: Sign out signs out at once, and a closing tab is not blocked', async () => {
  const { bff, main } = await form({ embeddings: 'true', pooling: 'mean' })
  makeDirty(main)
  await save(main, bff)
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  // The bar's text is the render's; the report to the app goes in a passive
  // effect: let it land, so this asks about the settled page, not a lag.
  await flushTasks()
  expect(unloadBlocked(), 'a saved page has nothing to lose').toBe(false)
  const signOut = signOutButton()
  signOut.focus()
  fireEvent.click(signOut)
  await act(async () => {})
  expect(screen.queryByRole('dialog'), 'a saved page asks nothing').toBeNull()
  expect(
    bff.to('DELETE', '/api/session'),
    'no dialog: straight out',
  ).toHaveLength(1)
})

// The product link (01's 38) and the browser's Forward.
test('2h.140: the product link while the model profile form is dirty asks, and Discard changes lands on the Overview', async () => {
  const { main } = await page({ embeddings: 'true', pooling: 'mean' })
  const link = productLink()
  link.focus()
  makeDirty(main)
  fireEvent.click(link)
  const d = await discardDialog()
  expect(window.location.hash, 'the link is not followed yet').toBe(HASH)
  expect(selectOf(main, MODE).value, 'the edit stays').toBe('false')
  fireEvent.click(confirmButton(d))
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
})

test('2h.140: the browser Forward while the model profile form is dirty is put back and asked, the edit kept; Keep editing keeps the page, a clean navigation leaves it', async () => {
  const { main } = await modelProfileAfterNavigation()
  // Forward needs an entry ahead of the model profile page, and the guard's put-back
  // overwrites the entry the browser stands on (replaceState): so the Model profiles
  // entry is pushed while the form is still clean, as in LeaveGuard.test.tsx,
  // then the page comes back by a traversal, and only then the edit.
  await typeHash('#/profiles/model')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  await pressBack()
  expect(screen.queryByRole('dialog'), 'clean: Back asks nothing').toBeNull()
  await within(main).findByRole('heading', { level: 1, name: ID })
  const select = selectOf(main, MODE)
  makeDirty(main)
  await pressForward()
  const d = await discardDialog()
  expect(window.location.hash, 'Forward is put back').toBe(HASH)
  expect(selectOf(main, MODE), 'the same form, not a remount').toBe(select)
  expect(selectOf(main, MODE).value, 'the edit stays').toBe('true')
  expect(status(main).textContent, 'the bar status stays as it was').toBe(
    en('save-unsaved'),
  )
  fireEvent.click(keepButton(d))
  await flushTasks()
  expect(
    window.location.hash,
    'Keep editing keeps the model profile page',
  ).toBe(HASH)
  expect(selectOf(main, MODE).value, 'the edit is kept').toBe('true')
  // After Keep editing the page is left for good the ordinary way: Discard
  // cleans the form, and the hash typed then moves.
  fireEvent.click(discardButton(main))
  await typeHash('#/profiles/model')
  expect(
    screen.queryByRole('dialog'),
    'clean: the navigation asks nothing',
  ).toBeNull()
  await within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
})

// ── 141: keyboard, screen reader, the strings of the save ───────────────

test("2h.141: the tab order takes each Embeddings About toggle and then its select, in the card order, and the bar's Discard and Save changes are the last controls of the page", async () => {
  const { main } = await page({ embeddings: 'true', pooling: 'mean' })
  const focusable = [
    ...main.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
  const at = (el: HTMLElement) => focusable.indexOf(el)
  const about = (label: string) =>
    within(main).getByRole('button', { name: `${en('field-about')} ${label}` })
  const mode = selectOf(main, MODE)
  const pool = selectOf(main, POOLING)
  for (const el of [about(MODE), mode, about(POOLING), pool])
    expect(at(el), `${el.id}: in the tab order`).toBeGreaterThan(-1)
  expect(
    at(about(MODE)),
    'the About toggle of a field comes before the field (91)',
  ).toBeLessThan(at(mode))
  expect(at(mode), 'Embedding mode, then Pooling').toBeLessThan(
    at(about(POOLING)),
  )
  expect(at(about(POOLING)), 'Pooling: About then select').toBeLessThan(
    at(pool),
  )
  expect(
    at(selectOf(main, en('model-profile-engine'))),
    'the Embeddings card sits after the Engine field',
  ).toBeLessThan(at(about(MODE)))
  expect(
    focusable.slice(-2).map((el) => el.textContent),
    'the bar ends the page',
  ).toEqual([en('save-discard'), en('save-button')])
})

test('2h.141: the bar status is one live region that is there before its text changes, and a failure is announced by the alert, not by the status line', async () => {
  const { bff, main } = await form(
    { embeddings: 'true', pooling: 'mean' },
    (_b, _id, n) =>
      n === 1
        ? reply(200, { revision: 'r2', running: true })
        : reply(500, { key: 'client-write-failed', args: [] }),
  )
  const line = status(main)
  expect(line.getAttribute('role'), 'a live region, polite').toBe('status')
  expect(line.textContent).toBe('')
  makeDirty(main)
  expect(status(main), 'the same element').toBe(line)
  expect(line.textContent).toBe(en('save-unsaved'))
  await save(main, bff)
  await waitFor(() => expect(line.textContent).toBe(en('save-saved-restart')))
  expect(status(main), 'the same element').toBe(line)
  // The failure goes to the alert (01's 30a).
  pick(main, POOLING, 'rank')
  await save(main, bff, 2)
  await within(main).findByText(en('save-failed'))
  expectAlert(main, en('save-failed'))
  expect(line.textContent, 'the status line never holds it').toBe(
    en('save-unsaved'),
  )
})

test("2h.141: the brief's script: Written as 1, off hiding it and sending null, on back clean, Pooling rank and default, the two 200 messages with r2 held, a 500 keeping the edits and Discard restoring", async () => {
  const { bff, main } = await form(
    { embeddings: '1', pooling: 'mean' },
    (_b, _id, n) =>
      n === 1
        ? reply(200, { revision: 'r2', running: true })
        : n === 2
          ? reply(200, { revision: 'r3', running: false })
          : reply(500, { key: 'client-write-failed', args: [] }),
  )
  const line = status(main)
  expect(text(writtenOf(main, 'embeddings')!), 'the file spelling 1').toBe(
    en('model-profile-written-as', '1'),
  )
  // off: dirty, the line hidden, and a save sends exactly the body of 133.
  pick(main, MODE, 'false')
  expect(isHidden(writtenOf(main, 'embeddings')), 'off hides the line').toBe(
    true,
  )
  expect(line.textContent, 'off is a change').toBe(en('save-unsaved'))
  // back on: the field is the loaded state again, so the line shows.
  pick(main, MODE, 'true')
  expect(line.textContent, 'on is clean').toBe('')
  expect(isHidden(writtenOf(main, 'embeddings')), 'and the line shows').toBe(
    false,
  )
  // Pooling: an option, then default.
  pick(main, POOLING, 'rank')
  await save(main, bff)
  expect(sentBody(bff)).toEqual({
    revision: 'r1',
    values: { pooling: 'rank' },
  })
  await waitFor(() => expect(line.textContent).toBe(en('save-saved-restart')))
  pick(main, POOLING, 'default')
  await save(main, bff, 2)
  expect(sentBody(bff, 2), "the 200's revision is held and sent").toEqual({
    revision: 'r2',
    values: { pooling: null },
  })
  await waitFor(() => expect(line.textContent).toBe(en('save-saved')))
  // A 500: save-failed, and the form is still dirty.
  pick(main, MODE, 'false')
  await save(main, bff, 3)
  await within(main).findByText(en('save-failed'))
  expect(line.textContent, 'still dirty').toBe(en('save-unsaved'))
  expect(selectOf(main, MODE).value).toBe('false')
  // Discard restores the values the last 200 saved.
  fireEvent.click(discardButton(main))
  expect(selectOf(main, MODE).value, 'Embedding mode back on').toBe('true')
  expect(selectOf(main, POOLING).value, 'Pooling back to default').toBe(
    'default',
  )
  expect(status(main).textContent, 'clean').toBe('')
})
