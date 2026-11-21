// The session answers of a model profile save (fe/briefs/02h-model-profile-save.md, sub-slice
// 2h.3): a 401 ends the session over the page (151), the sign-in in the dialog
// carries on for the same user and drops the edits for another (152), and a 403
// session-not-administrator revokes it (153). The tests of 154: 401 then same
// user, 401 then another user, revoked. The bar, the Embeddings selects, the
// PUT and the keyed failures are 135 to 149, in ModelProfileSave1 to 4, whose
// helpers this file repeats (a test file imports no other test file).
import {
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
  navigate,
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const GET = `/api/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const MODE = en('model-profile-embedding-mode')
const POOLING = en('model-profile-pooling')
/** What the first read gives, and what the read after a sign-in gives. */
const AS_LOADED = { embeddings: 'true', pooling: 'mean' }
const ELSEWHERE = { embeddings: 'false', pooling: 'cls' }
/** The session ended: 401, any body (the bff's key is session-required). */
const ENDED = () => reply(401, { key: 'session-required', args: [] })
/** 403, key session-not-administrator: the session is revoked. */
const REVOKED = () => reply(403, { key: 'session-not-administrator', args: [] })

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers (as ModelProfileSave2/3/4.test.tsx) ────────────────────────────────

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

/** The page, then dirty: Embedding mode picked off. */
async function dirty(
  modelProfileSave: BffOptions['modelProfileSave'] = () => ENDED(),
) {
  const { bff, main } = await loaded({
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { revision: 'r1', values: AS_LOADED })),
    modelProfileSave,
  })
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

/** Focuses `el` and clicks it, as a pointer or Enter does. */
function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
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

/**
 * The save bar's status line. The GPUs card keeps its own status region
 * in the DOM from its first render (brief 14 GD1.9), so the bar's line is
 * picked inside the bar, not by counting the page's status regions (section
 * 6 of fe/briefs/14-gpu-distribution.md, ruling of 2026-11-21).
 */
function status(main: HTMLElement): HTMLElement {
  settingsH2(main)
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => s.closest('.save-bar') !== null)
  expect(all, 'the save bar has its status line').toHaveLength(1)
  return all[0]!
}

function settingsH2(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
}

function assetsCard(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
}

function heading1(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', { level: 1, name: ID })
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

function signInButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('session-sign-in') })
}

/** The sign-in dialog the app opened with showModal(). */
function signInDialog(): HTMLElement {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(d.tagName, 'a native <dialog>').toBe('DIALOG')
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  return d
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/**
 * Saves into a 401, opens the bar's Sign in and signs in as `session.user`
 * in the dialog. The model profile reads answer `first` at first and `again` from
 * the second read on.
 */
async function signInAs(
  session: { user: string },
  again: () => Response,
  modelProfileSave?: BffOptions['modelProfileSave'],
) {
  let reads = 0
  const { bff, main } = await loaded({
    modelProfile: () =>
      ++reads === 1
        ? reply(200, modelProfileDoc({}, { revision: 'r1', values: AS_LOADED }))
        : again(),
    modelProfileSave: modelProfileSave ?? (() => ENDED()),
    signIn: () => reply(200, session),
  })
  pick(main, MODE, 'false')
  await save(main, bff)
  fireEvent.click(
    await within(main).findByRole('button', { name: en('session-sign-in') }),
  )
  const d = signInDialog()
  type(
    within(d).getByRole('textbox', { name: en('signin-username') }),
    session.user,
  )
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  return { bff, main }
}

// ── 151: a 401 ends the session over the page ────────────────────────────

test('2h.151: a 401 on a model profile save shows session-ended with the bar Sign in after it; the edits stay dirty, the page, the banner and the nav stay, no Reload', async () => {
  const { bff, main } = await dirty()
  await save(main, bff)
  const ended = await within(main).findByText(en('session-ended'))
  expectAlert(main, en('session-ended'))
  expect(within(main).queryByText(en('session-revoked'))).toBeNull()
  expect(
    within(main).queryByText(en('save-failed')),
    'not the generic sentence',
  ).toBeNull()
  expect(selectOf(main, MODE).value, 'the edit is kept').toBe('false')
  expect(status(main).textContent, 'the form stays dirty').toBe(
    en('save-unsaved'),
  )
  expect(before(ended, signInButton(main)), 'Sign in after the alert').toBe(
    true,
  )
  expect(
    within(main).queryByRole('button', { name: en('save-reload') }),
    'no Reload on a 401',
  ).toBeNull()
  // The page stays as it was (01's 36a): settings, h1, banner, nav.
  expect(settingsH2(main), 'the settings are still here').toBeTruthy()
  expect(heading1(main), 'the page stays').toBeTruthy()
  expect(screen.getByRole('banner').textContent).toContain(
    en('session-signed-in-as', 'alice'),
  )
  expect(
    within(screen.getByRole('navigation', { name: en('nav-label') })).getByRole(
      'link',
      { name: 'coder' },
    ),
    'the nav stays',
  ).toBeTruthy()
  expect(bff.to('GET', GET), 'no read').toHaveLength(1)
  expect(bff.to('PUT', PUT), 'one write').toHaveLength(1)
})

test('2h.151: the bar Sign in opens the sign-in dialog over the page; Escape, then Cancel, close it with focus back on that button, edits kept, nothing sent', async () => {
  const { bff, main } = await dirty()
  await save(main, bff)
  fireEvent.click(
    await within(main).findByRole('button', {
      name: en('session-sign-in'),
    }),
  )
  const d = signInDialog()
  // The dialog sits over the page: the page's h1 is still the one h1.
  expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1)
  fireEvent.keyDown(
    within(d).getByRole('textbox', { name: en('signin-username') }),
    { key: 'Escape', code: 'Escape' },
  )
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(document.activeElement, 'focus back on the bar Sign in').toBe(
    signInButton(main),
  )
  expectAlert(main, en('session-ended'), 'the alert stays after Escape')
  // Cancel closes it the same way.
  fireEvent.click(signInButton(main))
  const d2 = signInDialog()
  fireEvent.click(within(d2).getByRole('button', { name: en('signin-cancel') }))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(document.activeElement, 'focus back on the bar Sign in').toBe(
    signInButton(main),
  )
  expect(selectOf(main, MODE).value, 'the edit is kept').toBe('false')
  expect(status(main).textContent, 'still dirty').toBe(en('save-unsaved'))
  expect(bff.to('PUT', PUT), 'nothing resent').toHaveLength(1)
  expect(bff.to('GET', GET), 'nothing read').toHaveLength(1)
  expect(bff.to('POST', '/api/session'), 'no sign-in sent').toHaveLength(0)
})

// ── 152: the sign-in in the dialog ───────────────────────────────────────

test('2h.152: signing in as the same user closes the dialog, ends the alert, focuses Save changes, keeps the edits and resends nothing', async () => {
  const { bff, main } = await signInAs(
    { user: 'alice' },
    () =>
      reply(200, modelProfileDoc({}, { revision: 'r1', values: AS_LOADED })),
    (_body, _id, n) =>
      n === 1 ? ENDED() : reply(200, { revision: 'r2', running: false }),
  )
  expect(bff.to('POST', '/api/session')[0]!.body).toEqual({
    username: 'alice',
    password: 'hunter2',
  })
  await waitFor(() =>
    expect(document.activeElement, 'focus on Save changes').toBe(
      saveButton(main),
    ),
  )
  expectNoAlert(main, 'after the sign-in')
  expect(within(main).queryByText(en('server-edits-dropped'))).toBeNull()
  expect(selectOf(main, MODE).value, 'the edit is intact').toBe('false')
  expect(selectOf(main, POOLING).value, 'the other value is intact').toBe(
    AS_LOADED.pooling,
  )
  expect(status(main).textContent, 'the form stays dirty').toBe(
    en('save-unsaved'),
  )
  expect(bff.to('PUT', PUT), 'the save is not resent').toHaveLength(1)
  expect(
    bff.to('GET', GET),
    'the model profile is not read again',
  ).toHaveLength(1)
  // Saving again is the user's choice, and sends the same revision.
  await save(main, bff, 2)
  expect(sentBody(bff, 2), 'the same revision').toMatchObject({
    revision: 'r1',
    values: { embeddings: null },
  })
})

test('2h.152: signing in as another user drops the edits, reads the model profile once, and shows server-edits-dropped after the settings h2 with focus on the h1; the alert stays until the page is left', async () => {
  const OTHER = 'gemma-3-27b'
  const { bff, main } = await signInAs({ user: 'carol' }, () =>
    reply(200, modelProfileDoc({}, { revision: 'r-9', values: ELSEWHERE })),
  )
  await waitFor(() => expect(bff.to('GET', GET)).toHaveLength(2))
  await waitFor(() =>
    expect(selectOf(main, POOLING).value).toBe(ELSEWHERE.pooling),
  )
  expect(selectOf(main, MODE).value, 'the read value, not the edit').toBe(
    ELSEWHERE.embeddings,
  )
  expect(status(main).textContent, 'the form is clean').toBe('')
  expect(
    saveButton(main).getAttribute('aria-disabled'),
    'Save is inert on a clean form',
  ).toBe('true')
  expect(within(main).queryByText(en('session-ended'))).toBeNull()
  // The dropped-edits alert (01's 36b): a role="alert" paragraph right after
  // the settings h2, above the first card.
  const dropped = within(main).getByText(en('server-edits-dropped'))
  expect(dropped.closest('[role="alert"]'), 'role="alert"').not.toBe(null)
  expect(dropped.tagName, 'a paragraph').toBe('P')
  expect(before(settingsH2(main), dropped), 'right after the settings h2').toBe(
    true,
  )
  expect(before(dropped, assetsCard(main)), 'above the first card').toBe(true)
  await waitFor(() =>
    expect(document.activeElement, 'focus on the h1').toBe(heading1(main)),
  )
  expect(bff.to('PUT', PUT), 'nothing sent but the failed save').toHaveLength(1)
  await waitFor(() =>
    expect(screen.getByRole('banner').textContent).toContain(
      en('session-signed-in-as', 'carol'),
    ),
  )
  // The alert stays until the page is left.
  navigate(`#/profiles/model/${OTHER}`)
  await within(main).findByRole('heading', { level: 1, name: OTHER })
  expect(
    within(main).queryByText(en('server-edits-dropped')),
    'the alert is gone with the page',
  ).toBeNull()
})

// ── 152: a sign-in as another user with a CLEAN form: nothing to drop ─────
// The path: a 401 leaves the bar's alert and its Sign in (151), the user puts
// the edit back on its own (clean form, Save inert), then signs in in that
// same dialog as another user. The model profile is read again for the new session,
// and no edit is dropped: the notice server-edits-dropped must NOT show.

test('2h.152: a sign-in as another user on a clean form (the edit put back after the 401) re-reads the model profile and shows no server-edits-dropped', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    modelProfile: () =>
      ++reads === 1
        ? reply(200, modelProfileDoc({}, { revision: 'r1', values: AS_LOADED }))
        : reply(
            200,
            modelProfileDoc({}, { revision: 'r-9', values: ELSEWHERE }),
          ),
    modelProfileSave: () => ENDED(),
    signIn: () => reply(200, { user: 'carol' }),
  })
  pick(main, MODE, 'false')
  await save(main, bff)
  await within(main).findByText(en('session-ended'))
  const signIn = signInButton(main)
  // The user puts the edit back: the form is clean, nothing is left to lose.
  pick(main, MODE, AS_LOADED.embeddings)
  expect(status(main).textContent, 'the form is clean').toBe('')
  expect(
    saveButton(main).getAttribute('aria-disabled'),
    'Save is inert on a clean form',
  ).toBe('true')
  // The bar's alert and its Sign in stay (151): that is the reachable path.
  expectAlert(main, en('session-ended'), 'the alert stays on a clean form')
  press(signIn)
  const d = signInDialog()
  type(within(d).getByRole('textbox', { name: en('signin-username') }), 'carol')
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  // The model profile is read again for the new session.
  await waitFor(() => expect(bff.to('GET', GET)).toHaveLength(2))
  await waitFor(() =>
    expect(selectOf(main, POOLING).value).toBe(ELSEWHERE.pooling),
  )
  expect(selectOf(main, MODE).value, 'the read value').toBe(
    ELSEWHERE.embeddings,
  )
  expect(status(main).textContent, 'the new form is clean').toBe('')
  // Nothing was dropped: no server-edits-dropped, and no alert of any kind.
  expect(
    within(main).queryByText(en('server-edits-dropped')),
    'no dropped-edits notice',
  ).toBeNull()
  expect(
    within(main)
      .queryAllByRole('alert')
      .map((a) => a.textContent),
    'no alert after the sign-in',
  ).toEqual([])
  await waitFor(() =>
    expect(document.activeElement, 'focus on the h1').toBe(heading1(main)),
  )
  expect(bff.to('PUT', PUT), 'nothing sent but the failed save').toHaveLength(1)
})

// ── 153: a revoked session (403 session-not-administrator) ──────────────

test('2h.153: a 403 session-not-administrator on a model profile save shows session-revoked in place of session-ended: edits kept dirty, the bar Sign in after the alert, focus stays on Save, banner and nav stay', async () => {
  const { bff, main } = await dirty(() => REVOKED())
  // press, not a bare click: a real click focuses Save, and the code must
  // not move focus away from it.
  press(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
  const revoked = await within(main).findByText(en('session-revoked'))
  expectAlert(main, en('session-revoked'))
  expect(within(main).queryByText(en('session-ended'))).toBeNull()
  expect(selectOf(main, MODE).value, 'the edit is kept').toBe('false')
  expect(status(main).textContent, 'the form stays dirty').toBe(
    en('save-unsaved'),
  )
  expect(document.activeElement, 'focus stays on Save changes').toBe(
    saveButton(main),
  )
  const signIn = within(main).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(before(revoked, signIn), 'the bar Sign in after the alert').toBe(true)
  // 36a: the banner and the nav stay as they were, the page is not replaced.
  expect(screen.getByRole('banner').textContent).toContain(
    en('session-signed-in-as', 'alice'),
  )
  expect(
    within(screen.getByRole('navigation', { name: en('nav-label') })).getByRole(
      'link',
      { name: 'coder' },
    ),
    'the nav stays',
  ).toBeTruthy()
  expect(settingsH2(main), 'the settings are still here').toBeTruthy()
  expect(bff.to('GET', GET), 'no read').toHaveLength(1)
})
