// The keyed answers of a preset save (fe/briefs/02h-preset-save.md, sub-slice
// 2h.2): 409 preset-changed-elsewhere with Reload (145), 404 no-preset-named
// (146), 403 client-not-authorized and 503 client-unavailable (147), 504
// client-timeout and the fe's own 120 s limit of a PUT (148), and the guard's
// keys with presets-malformed (149). The bar, the editable Embeddings
// selects, the PUT, the 200 and the plain failure are 135 to 141, in
// PresetSave1.test.tsx and PresetSave2.test.tsx, and the 422 answers are
// 142 to 144 and 150, in PresetSave3.test.tsx, whose helpers this file
// repeats (a test file imports no other test file). The session (2h.3) is
// not here.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  en,
  fakeBff,
  held,
  presetDoc,
  navigate,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const GET = `/api/presets/${ID}`
const PUT = `/api/presets/${ID}`
const MODE = en('preset-embedding-mode')
const POOLING = en('preset-pooling')
/** What the first read gives, and what the read after a Reload gives. */
const AS_LOADED = { embeddings: 'true', pooling: 'mean' }
const ELSEWHERE = { embeddings: 'false', pooling: 'cls' }

/** 409: the section changed since the read (api-contract.md). */
const CONFLICT = () => reply(409, { key: 'preset-changed-elsewhere', args: [] })

/** [name, status] of the keys that keep the place's generic failure (149). */
const GUARD_KEYS: [string, number][] = [
  ['request-cross-origin', 403],
  ['request-bad-host', 403],
  ['payload-too-large', 413],
  ['presets-malformed', 400],
]

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers (as PresetSave2.test.tsx and PresetSave3.test.tsx) ──────────

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
    name: en('preset-assets'),
  })
  return { bff, main }
}

/** The page read as `AS_LOADED` with revision `r1`, the save answered `answer`. */
function form(answer?: BffOptions['presetSave']) {
  return loaded({
    preset: () =>
      reply(200, presetDoc({}, { revision: 'r1', values: AS_LOADED })),
    presetSave: answer,
  })
}

/** The page, then dirty: Embedding mode picked off, loaded on. */
async function dirty(answer?: BffOptions['presetSave']) {
  const { bff, main } = await form(answer)
  pick(main, MODE, 'false')
  return { bff, main }
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

function reloadButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-reload') })
}

/** The bar's answer buttons: the buttons that share the parent of a
 * role="alert" paragraph (Reload, Sign in). */
function barButtons(main: HTMLElement): HTMLElement[] {
  const parents = new Set(
    within(main)
      .queryAllByRole('alert')
      .map((a) => a.parentElement),
  )
  return [...parents].flatMap(
    (p) => [...(p?.querySelectorAll('button') ?? [])] as HTMLElement[],
  )
}

/** Clicks Save changes and waits for the n-th PUT of this preset. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(n))
}

/** The whole body of the n-th recorded PUT of this preset. */
function sentBody(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  return bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
}

/** The save bar's status line: the only role="status" below the h2 of the
 * settings (the page's loading status is above it). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('preset-settings-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line once the page is loaded').toHaveLength(1)
  return all[0]!
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

function heading1(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', { level: 1, name: ID })
}

// ── Fake timers (the conventions of Timeout.test.tsx) ───────────────────

/** Fake timers and Date only: the microtask queue stays real. */
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
    for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

/** Moves the clock by `ms` in one step, then flushes. */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** The preset page under a fake clock, its read answered by the stub. */
async function loadedOnFakeClock(options: BffOptions = {}) {
  fakeClock()
  startAt(HASH)
  const bff = fakeBff(options)
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  expect(
    within(main).queryByRole('heading', {
      level: 3,
      name: en('preset-assets'),
    }),
    'the preset page is loaded',
  ).not.toBeNull()
  return { bff, main }
}

// ── 145: 409 preset-changed-elsewhere ───────────────────────────────────

test('2h.145: a 409 preset-changed-elsewhere shows api-preset-changed-elsewhere with Reload; Keep editing does nothing and focus goes back to Reload', async () => {
  const { bff, main } = await dirty(CONFLICT)
  await save(main, bff)
  await within(main).findByText(en('api-preset-changed-elsewhere'))
  expectAlert(main, en('api-preset-changed-elsewhere'))
  expect(
    selectOf(main, MODE).value,
    'the edit is kept while the answer is up',
  ).toBe('false')
  expect(status(main).textContent, 'the form stays dirty').toBe(
    en('save-unsaved'),
  )
  expect(
    within(main).queryByRole('button', { name: en('session-sign-in') }),
    'no Sign in',
  ).toBeNull()
  const reload = reloadButton(main)
  // Reload asks first: Keep editing changes nothing and takes focus back.
  fireEvent.click(reload)
  const ask = await discardDialog()
  fireEvent.click(keepButton(ask))
  expect(screen.queryByRole('dialog'), 'the dialog is closed').toBeNull()
  expect(bff.to('GET', GET), 'no read while the answer stands').toHaveLength(1)
  expect(selectOf(main, MODE).value, 'the edit is kept').toBe('false')
  expect(document.activeElement, 'focus back on Reload').toBe(reload)
  expectAlert(main, en('api-preset-changed-elsewhere'), 'the alert stays')
})

test('2h.145: a confirmed Reload reads the preset again: preset-loading in place of the settings, then the read values, clean, no alert, focus on the h1', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    preset: () =>
      reply(
        200,
        presetDoc(
          {},
          ++reads === 1
            ? { revision: 'r1', values: AS_LOADED }
            : { revision: 'r-9', values: ELSEWHERE },
        ),
      ),
    presetSave: CONFLICT,
  })
  pick(main, MODE, 'false')
  await save(main, bff)
  fireEvent.click(reloadButton(main))
  const ask = await discardDialog()
  fireEvent.click(confirmButton(ask))
  // The settings are replaced by the page's loading status (2d), then the
  // read lands: its values, clean, with no alert.
  expect(
    within(main).queryByText(en('preset-loading')),
    'the settings are replaced by the loading status',
  ).not.toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 2,
      name: en('preset-settings-title'),
    }),
    'no settings while it reads',
  ).toBeNull()
  await waitFor(() => expect(bff.to('GET', GET)).toHaveLength(2))
  await waitFor(() =>
    expect(selectOf(main, POOLING).value).toBe(ELSEWHERE.pooling),
  )
  expect(selectOf(main, MODE).value, 'the read value, not the edit').toBe(
    ELSEWHERE.embeddings,
  )
  expectNoAlert(main, 'after the reload')
  expect(status(main).textContent, 'the form is clean').toBe('')
  expect(
    saveButton(main).getAttribute('aria-disabled'),
    'Save is inert on a clean form',
  ).toBe('true')
  expect(document.activeElement, 'focus on the page h1').toBe(heading1(main))
  // The next save carries the revision of the reload.
  pick(main, MODE, 'true')
  await save(main, bff, 2)
  expect(sentBody(bff, 2), 'the new revision').toMatchObject({
    revision: 'r-9',
    values: { embeddings: 'true' },
  })
  expect(reads, 'the second read is the reload').toBe(2)
})

test('2h.145: on a form the user made clean again, Reload asks nothing and reads at once', async () => {
  const { bff, main } = await dirty(CONFLICT)
  await save(main, bff)
  pick(main, MODE, AS_LOADED.embeddings)
  expect(status(main).textContent, 'the form is clean').toBe('')
  fireEvent.click(reloadButton(main))
  expect(screen.queryByRole('dialog'), 'no dialog when clean').toBeNull()
  expect(dialogs.opened, 'no dialog opened').toEqual([])
  await waitFor(() => expect(bff.to('GET', GET)).toHaveLength(2))
  expectNoAlert(main, 'the read of a clean form ends the alert')
  expect(selectOf(main, MODE).value, 'the read values').toBe(
    AS_LOADED.embeddings,
  )
  expect(document.activeElement, 'focus on the page h1').toBe(heading1(main))
})

/** [what, the read's answer, the message a FIRST visit shows for it] (2d.66,
 * 59): the same one the read after a confirmed Reload must show (145). */
const READ_FAILURES: [string, () => Response, string][] = [
  [
    '503 client-unavailable',
    () => reply(503, { key: 'client-unavailable', args: [] }),
    en('api-client-unavailable'),
  ],
  [
    '404 no-preset-named',
    () => reply(404, { key: 'no-preset-named', args: [] }),
    en('preset-not-found', ID),
  ],
  [
    '500 a key the fe does not know',
    () => reply(500, { key: 'secret-detail', args: [] }),
    en('preset-load-failed'),
  ],
]

/** The page's own paragraphs below the h1: not the back link above it, not the
 * bar's alert, not a loading status. */
function paragraphs(main: HTMLElement, h1: HTMLElement): string[] {
  return [...main.querySelectorAll('p')]
    .filter(
      (p) => before(h1, p) && !p.closest('[role="alert"], [role="status"]'),
    )
    .map((p) => p.textContent ?? '')
}

test('2h.145: a Reload whose read fails shows the message a first visit shows for that answer, by the read key, in place of the settings', async () => {
  for (const [what, failure, text] of READ_FAILURES) {
    let reads = 0
    const { bff, main } = await loaded({
      preset: () =>
        ++reads === 1
          ? reply(200, presetDoc({}, { revision: 'r1', values: AS_LOADED }))
          : failure(),
      presetSave: CONFLICT,
    })
    pick(main, MODE, 'false')
    await save(main, bff)
    fireEvent.click(reloadButton(main))
    fireEvent.click(confirmButton(await discardDialog()))
    await waitFor(() => expect(bff.to('GET', GET)).toHaveLength(2))
    expectNoAlert(main, `${what}: the 409 alert is gone`)
    expect(
      within(main).queryByRole('heading', {
        level: 2,
        name: en('preset-settings-title'),
      }),
      `${what}: the failed read replaces the settings`,
    ).toBeNull()
    const h1 = heading1(main)
    // The message of the read's key, as a plain paragraph below the h1 (2d.66,
    // 59), never the generic one for a key the fe knows.
    expect(
      paragraphs(main, h1),
      `${what}: the message of a first visit`,
    ).toEqual([text])
    for (const [, , other] of READ_FAILURES) {
      if (other !== text) {
        expect(
          within(main).queryByText(other),
          `${what}: not ${other}`,
        ).toBeNull()
      }
    }
    expect(
      document.body.textContent,
      `${what}: the answer body never reaches the page`,
    ).not.toContain('secret-detail')
    expect(h1.getAttribute('tabindex'), `${what}: the h1 is focusable`).toBe(
      '-1',
    )
    await waitFor(() =>
      expect(document.activeElement, `${what}: focus on the h1`).toBe(h1),
    )
    cleanup()
    vi.unstubAllGlobals()
  }
})

/** The preset the page moves to after a Reload, as a hash change does. */
const OTHER = 'gemma-3-27b'
const OTHER_HASH = `#/models/${OTHER}`
const OTHER_GET = `/api/presets/${OTHER}`

test('2h.145: a Reload confirmed on one preset leaves nothing behind for the next: its 404 read shows preset-not-found with the new id as a first visit does, focus on the h1', async () => {
  const { bff, main } = await loaded({
    preset: (id) =>
      id === ID
        ? reply(200, presetDoc({}, { revision: 'r1', values: AS_LOADED }))
        : reply(404, { key: 'no-preset-named', args: [] }),
    presetSave: CONFLICT,
  })
  pick(main, MODE, 'false')
  await save(main, bff)
  fireEvent.click(reloadButton(main))
  fireEvent.click(confirmButton(await discardDialog()))
  await waitFor(() => expect(bff.to('GET', GET)).toHaveLength(2))
  expectNoAlert(main, 'the reload ended the 409 alert')
  // The hash moves to another preset, as a click on a row does.
  navigate(OTHER_HASH)
  const shown = await within(main).findByText(en('preset-not-found', OTHER))
  expect(shown.tagName, 'the plain paragraph of a first visit').toBe('P')
  expect(shown.getAttribute('role'), 'not an alert').toBeNull()
  expect(within(main).queryAllByRole('alert'), 'no alert').toEqual([])
  expect(
    within(main).queryByText(en('preset-load-failed')),
    'not the generic load failure a Reload leaves behind',
  ).toBeNull()
  const h1 = await within(main).findByRole('heading', { level: 1, name: OTHER })
  expect(before(h1, shown), 'the sentence below the h1').toBe(true)
  expect(
    bff.to('GET', OTHER_GET),
    'the new page read its own preset once',
  ).toHaveLength(1)
  await waitFor(() => expect(document.activeElement).toBe(h1))
})

test('2h.145: a Reload confirmed on one preset leaves nothing behind for the next: its 200 read shows the other preset and its values, focus on the h1 as a plain navigation', async () => {
  const { bff, main } = await loaded({
    preset: (id) =>
      id === ID
        ? reply(200, presetDoc({}, { revision: 'r1', values: AS_LOADED }))
        : reply(
            200,
            presetDoc(
              { embeddings: 'true', pooling: 'cls' },
              { id: OTHER, revision: 'r-b' },
            ),
          ),
    presetSave: CONFLICT,
  })
  pick(main, MODE, 'false')
  await save(main, bff)
  fireEvent.click(reloadButton(main))
  fireEvent.click(confirmButton(await discardDialog()))
  await waitFor(() => expect(bff.to('GET', GET)).toHaveLength(2))
  navigate(OTHER_HASH)
  const h1 = await within(main).findByRole('heading', { level: 1, name: OTHER })
  await waitFor(() => expect(selectOf(main, POOLING).value).toBe('cls'))
  expect(selectOf(main, MODE).value, "the other preset's value").toBe('true')
  expect(status(main).textContent, 'the new page is clean').toBe('')
  expect(within(main).queryAllByRole('alert'), 'no alert').toEqual([])
  expect(
    bff.to('GET', OTHER_GET),
    'the new page read its own preset once',
  ).toHaveLength(1)
  await waitFor(() => expect(document.activeElement).toBe(h1))
})

// ── 146: 404 no-preset-named ────────────────────────────────────────────

test('2h.146: a 404 no-preset-named fills the id into api-no-preset-named; the edits stay dirty, there is no Reload and the page is not replaced', async () => {
  const { bff, main } = await dirty(() =>
    reply(404, { key: 'no-preset-named', args: [ID] }),
  )
  await save(main, bff)
  const text = en('api-no-preset-named', ID)
  await within(main).findByText(text)
  expectAlert(main, text)
  expect(
    within(main).queryByText(en('save-failed')),
    'not the generic sentence',
  ).toBeNull()
  expect(selectOf(main, MODE).value, 'the edit is kept').toBe('false')
  expect(status(main).textContent, 'the form stays dirty').toBe(
    en('save-unsaved'),
  )
  expect(
    within(main).queryByRole('button', { name: en('save-reload') }),
    'no Reload: the page is not replaced',
  ).toBeNull()
  expect(
    within(main).getByRole('heading', {
      level: 2,
      name: en('preset-settings-title'),
    }),
    'the settings are still here',
  ).toBeTruthy()
  expect(heading1(main), 'the page stays').toBeTruthy()
  expect(bff.to('GET', GET), 'no read of the preset').toHaveLength(1)
})

test('2h.146: the 404 text is filled with the id of the page whatever the reply sends as args: no {0} is left, the reply id is not shown', async () => {
  // The bff sends EMPTY args for an id no preset can ever carry (a section it
  // refuses before it echoes anything), and an id of its own when it echoes.
  for (const args of [[], ['Qwen3-28B']] as string[][]) {
    const what = `args ${JSON.stringify(args)}`
    const { bff, main } = await dirty(() =>
      reply(404, { key: 'no-preset-named', args }),
    )
    await save(main, bff)
    const text = en('api-no-preset-named', ID)
    await within(main).findByText(text)
    expectAlert(main, text, what)
    const alert = within(main).getByText(text).closest('[role="alert"]')
    expect(
      alert?.textContent,
      `${what}: no placeholder left in the text`,
    ).not.toContain('{0}')
    expect(
      alert?.textContent,
      `${what}: the page's id, not the id of the reply`,
    ).not.toContain('Qwen3-28B')
    expect(bff.to('PUT', PUT), `${what}: one write`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 147: 403 client-not-authorized and 503 client-unavailable ───────────

test('2h.147: 403 client-not-authorized and 503 client-unavailable show their own text, keep the edits dirty, and offer no button', async () => {
  const answers: [number, string, string][] = [
    [403, 'client-not-authorized', 'api-client-not-authorized'],
    [503, 'client-unavailable', 'api-client-unavailable'],
  ]
  for (const [status_, key, enKey] of answers) {
    const what = `${status_} ${key}`
    const { bff, main } = await dirty(() => reply(status_, { key, args: [] }))
    await save(main, bff)
    await within(main).findByText(en(enKey))
    expectAlert(main, en(enKey), what)
    expect(
      within(main).queryByText(en('save-failed')),
      `${what}: not the generic sentence`,
    ).toBeNull()
    expect(selectOf(main, MODE).value, `${what}: the edit is kept`).toBe(
      'false',
    )
    expect(status(main).textContent, `${what}: still dirty`).toBe(
      en('save-unsaved'),
    )
    expect(barButtons(main), `${what}: no button in the alert`).toEqual([])
    expect(
      within(main).queryByRole('button', { name: en('save-reload') }),
      `${what}: no Reload`,
    ).toBeNull()
    expect(bff.to('GET', GET), `${what}: no read`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 148: 504 client-timeout, and the fe's own time limit ────────────────

test('2h.148: a 504 client-timeout reads as save-timeout with no button; the edits stay dirty and the next save sends the same revision', async () => {
  const { bff, main } = await dirty(() =>
    reply(504, { key: 'client-timeout', args: [] }),
  )
  await save(main, bff)
  await within(main).findByText(en('save-timeout'))
  expectAlert(main, en('save-timeout'), 'the bar reads save-timeout')
  expect(
    within(main).queryByText(en('save-failed')),
    'not the generic sentence',
  ).toBeNull()
  expect(barButtons(main), 'no button in the alert').toEqual([])
  for (const name of [en('save-reload'), en('session-sign-in')])
    expect(
      within(main).queryByRole('button', { name }),
      `no ${name} in the bar`,
    ).toBeNull()
  expect(selectOf(main, MODE).value, 'the edit is kept').toBe('false')
  expect(status(main).textContent, 'the form stays dirty').toBe(
    en('save-unsaved'),
  )
  // The revision the page holds did not move: the next Save sends it again.
  await save(main, bff, 2)
  // Embedding mode off is sent as null, the key removed (table 133).
  expect(sentBody(bff, 2), 'the same revision, the key removed').toMatchObject({
    revision: 'r1',
    values: { embeddings: null },
  })
})

test('2h.148: a save held to the fe 120 s limit shows save-timeout; the edits stay dirty and the next save sends the same revision', async () => {
  const h = held()
  const { bff, main } = await loadedOnFakeClock({
    preset: () =>
      reply(200, presetDoc({}, { revision: 'r1', values: AS_LOADED })),
    presetSave: (_body, _id, n, init) =>
      n === 1 ? h.answer(init) : reply(200, { revision: 'r2', running: false }),
  })
  pick(main, MODE, 'false')
  fireEvent.click(saveButton(main))
  await flush()
  expect(bff.to('PUT', PUT)).toHaveLength(1)
  await advance(119_999)
  expect(h.signals[0]?.aborted, 'not aborted at 119 999 ms').toBe(false)
  expect(
    within(main).queryByText(en('save-timeout')),
    'no timeout yet',
  ).toBeNull()
  await advance(1)
  expect(h.signals[0]?.aborted, 'aborted at 120 000 ms').toBe(true)
  const line = within(main).queryByText(en('save-timeout'))
  expect(line, 'the bar reads save-timeout').not.toBeNull()
  expectAlert(main, en('save-timeout'))
  expect(
    within(main).queryByText(en('save-failed')),
    'not the generic sentence',
  ).toBeNull()
  expect(selectOf(main, MODE).value, 'the edit is kept').toBe('false')
  expect(status(main).textContent, 'the form stays dirty').toBe(
    en('save-unsaved'),
  )
  fireEvent.click(saveButton(main))
  await flush()
  expect(bff.to('PUT', PUT)).toHaveLength(2)
  // Embedding mode off is sent as null, the key removed (table 133).
  expect(sentBody(bff, 2), 'the same revision, the key removed').toMatchObject({
    revision: 'r1',
    values: { embeddings: null },
  })
})

// ── 149: the guard keys and presets-malformed ───────────────────────────

test('2h.149: the guard keys and presets-malformed on a preset save show save-failed and keep the edits', async () => {
  for (const [key, status_] of GUARD_KEYS) {
    const what = `${status_} ${key}`
    const { bff, main } = await dirty(() => reply(status_, { key, args: [] }))
    await save(main, bff)
    await within(main).findByText(en('save-failed'))
    expectAlert(main, en('save-failed'), what)
    expect(selectOf(main, MODE).value, `${what}: the edit is kept`).toBe(
      'false',
    )
    expect(status(main).textContent, `${what}: still dirty`).toBe(
      en('save-unsaved'),
    )
    expect(
      within(main).queryByRole('button', { name: en('save-reload') }),
      `${what}: no Reload`,
    ).toBeNull()
    expect(
      within(main).queryByRole('button', { name: en('session-sign-in') }),
      `${what}: no Sign in`,
    ).toBeNull()
    expect(bff.to('PUT', PUT), `${what}: one write`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})
