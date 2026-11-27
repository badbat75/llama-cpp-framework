// Brief 22, slice 22D (fe/briefs/22-model-sources.md): deleting a model profile
// from its own page, with the Confirm in place of `fe/DESIGN.md` 5.2, on a
// profile that is switched off. The page, its read and its save are
// ModelProfilePage.test.tsx and ModelProfileSave*.test.tsx; the create on the
// list page is 22C. 22 D.8 (the strings file gains exactly 22D's keys) is
// `1a strings` in App.test.tsx, which compares the strings file against the
// test's EN map, where 22D's keys are listed. The tests are modelled on
// ServerProfilePage2.test.tsx (brief 16's 17b), which this slice follows; where
// 22D differs, the criterion says so (the button follows `enabled`, not an
// instance; the 409 names the instances the manager refuses for).
//
// Not provable in jsdom, so not tested here: the focus ring and the muted look
// of the line of 22 D.1 (the designer's review), and the 320 px and 200 % text
// widths, which the UX check's `model-profile-delete` state measures.
//
// Slice 22D2 (the same brief, section 22D2) moves the opener into the status
// row under the summary line, moves the save bar to the top of the settings
// section, and replaces the muted "switch it off first" line with the row's
// model-profile-on.
//
// Slice 22D4 (the same brief, section 22D4) moves the opener again, into the
// save bar as its first button, and takes the status row away: the switched-on
// state reads model-profile-on in a plain paragraph under the summary line, and
// a switched-on profile keeps the button, switched off with the list's reason.
// The answers of D.1 to D.7 stand; what changes here is the place of the opener
// and of the group, and where a refusal's text lands, so D.1, D.2 and D.4 name
// the new place. The − on the list rows, the bar's Delete and the page's
// paragraph are ModelProfileDelete3.test.tsx.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
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
  accName,
  before,
  type BffOptions,
  deferred,
  en,
  fakeBff,
  modelProfileDoc,
  navigate,
  reply,
  startAt,
  stubDialog,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const ROUTE = `/api/profiles/model/${encodeURIComponent(ID)}`
/** A second profile: the page a deleted one gives place to (17b review R2). */
const OTHER = 'gemma-3-12b-it'
const OTHER_HASH = `#/profiles/model/${OTHER}`
const OTHER_ROUTE = `/api/profiles/model/${encodeURIComponent(OTHER)}`
/** A profile whose id is NOT spelled the same raw and encoded (22 D.3, the 22D
 *  review): the raw id holds a space and a `#`, so the address of its route is
 *  a different string from the id. Every other id here survives a dropped
 *  `encodeURIComponent` unchanged, which is what this one pins. */
const SPACED = 'qwen 3-27B#7'
const SPACED_HASH = `#/profiles/model/${encodeURIComponent(SPACED)}`
const SPACED_ROUTE = `/api/profiles/model/${encodeURIComponent(SPACED)}`
/** The instances a 22 D.5 409 names. */
const TAKEN = 'coder, embed'
/** The Embeddings card's mode select: the page's own field, used to make the
 *  page hold an edit (22 D.3: the unsaved-changes question). */
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

/** Lets jsdom's queued tasks (a hashchange, a dialog closing) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The document of a switched-off profile, with the two Embeddings keys set so
 *  its select is editable (22 D.3 makes an edit). */
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

/** The page, with its document read as a switched-off profile. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfile: (id) => offDoc(id), ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
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

/** The save bar's button group: 22 D4.10 puts the page's Delete in it, and 22
 *  D4.30 moves it to the group's end, after Discard and Save changes. */
function barActions(main: HTMLElement): HTMLElement {
  const actions = main.querySelector('.save-actions')
  expect(actions, 'the bar’s button group of 2h.1').not.toBeNull()
  return actions as HTMLElement
}

/** The save bar (22 D.1: the row comes before it; 22D2 moved it to the top). */
function saveBar(main: HTMLElement): HTMLElement | null {
  return main.querySelector('.save-bar')
}

function deleteButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('model-profile-delete') })
}

/** The confirm group's accessible name: its question (22 D.2). */
const QUESTION = en('model-profile-delete-question', ID)

/** The paragraph the group is named by, the question it asks. */
function questionOf(group: HTMLElement): HTMLElement | null {
  const id = group.getAttribute('aria-labelledby')
  return id === null ? null : document.getElementById(id)
}

/** The confirm group, open or not. The page's fields are `fieldset`s (groups)
 *  as well, so the pin asks for the one its question names. */
function queryConfirm(main: HTMLElement): HTMLElement | null {
  return within(main).queryByRole('group', { name: QUESTION })
}

/** The confirm group, once the opener is pressed. */
async function confirmGroup(main: HTMLElement): Promise<HTMLElement> {
  press(deleteButton(main))
  return within(main).findByRole('group', { name: QUESTION })
}

function confirmIn(group: HTMLElement, key: string): HTMLElement {
  return within(group).getByRole('button', { name: en(key) })
}

/** The group's status line (22 D.3: the group holds one). */
function groupStatus(group: HTMLElement): HTMLElement {
  const all = [...group.querySelectorAll<HTMLElement>('[role="status"]')]
  expect(all, 'the group has one status line').toHaveLength(1)
  return all[0]!
}

/** The text of every `role="alert"` inside `root`. */
function alertTexts(root: HTMLElement): string[] {
  return [...root.querySelectorAll('[role="alert"]')].map(
    (a) => a.textContent ?? '',
  )
}

/** The document reads the page sent. */
function reads(bff: ReturnType<typeof fakeBff>, id = ID): number {
  return bff.to('GET', `/api/profiles/model/${encodeURIComponent(id)}`).length
}

/** Asks the browser whether leaving the page would be held. */
function asksBrowser(): boolean {
  const e = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(e)
  return e.defaultPrevented
}

/** Asks the page's own sign-in dialog to sign `user` in. */
async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(d.tagName, 'a native <dialog>').toBe('DIALOG')
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  fireEvent.change(
    within(d).getByRole('textbox', { name: en('signin-username') }),
    {
      target: { value: user },
    },
  )
  fireEvent.change(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
}

// ── 22 D.1: offered while switched off ──────────────────────────────────

test('22 D.1 (as 22 D4.10 moves its place and 22 D4.30 its order): a switched-off profile offers the model-profile-delete delete as the last button of the save bar’s group, after Discard and Save changes, and outside every fieldset; a switched-on profile keeps it, aria-disabled with the list’s reason; the status row of 22 D2.1 is gone', async () => {
  const { main } = await loaded()
  const summary = main.querySelector('p.model-profile-summary')
  expect(summary, 'the summary line is there').not.toBeNull()
  expect(
    main.querySelector('div.page-status'),
    '22 D4.9: the status row of 22 D2.1 is gone',
  ).toBeNull()
  const bar = saveBar(main)
  expect(bar, 'the save bar is there').not.toBeNull()
  const button = deleteButton(main)
  const actions = barActions(main)
  expect(
    button.closest('.save-actions'),
    'the delete is in the bar’s group',
  ).toBe(actions)
  expect(button.closest('fieldset'), 'outside the form').toBeNull()
  expect(
    [...actions.children].indexOf(button),
    '22 D4.30: it is the group’s last button, after Discard and Save changes',
  ).toBe(actions.children.length - 1)
  expect(
    within(actions)
      .getAllByRole('button')
      .map((b) => accName(b)),
    '22 D4.30: Discard, Save changes, then the Delete the bar ends with',
  ).toEqual([en('save-discard'), en('save-button'), en('model-profile-delete')])
  expect(
    button.getAttribute('aria-disabled'),
    'a switched-off profile: the delete can act',
  ).not.toBe('true')
  expect(
    within(main).queryByText(en('model-profile-on-short')),
    '24 A2.1: a switched-off profile says nothing about being on',
  ).toBeNull()
  cleanup()
  vi.unstubAllGlobals()
  // A switched-on profile: the same button, switched off with the reason.
  const on = await loaded({ modelProfile: () => onDoc() })
  const kept = within(on.main).getByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    kept.getAttribute('aria-disabled'),
    '22 D4.10: a switched-on profile cannot be deleted',
  ).toBe('true')
  expect(kept.closest('.save-actions'), 'and it stays in the bar').toBe(
    barActions(on.main),
  )
  expect(
    (within(on.main).getByText(en('model-profile-on-short')).textContent ?? '')
      .replace(/\s+/g, ' ')
      .trim(),
    '24 A2.1: the state line says the state in the short sentence, beside the page’s own switch',
  ).toBe(en('model-profile-on-short'))
})

test('22 D.1 (as 22 D4.10 amends it): the offer follows the document read: nothing while the read is open, an active delete when a read answers switched off, a delete switched off with the reason when it answers switched on', async () => {
  const gate = deferred()
  fakeBff({ modelProfile: () => gate.promise })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  expect(
    within(main).queryByRole('button', { name: en('model-profile-delete') }),
    'nothing while the document is open',
  ).toBeNull()
  expect(
    within(main).queryByText(en('model-profile-on-short')),
    'nor the state line that replaces it',
  ).toBeNull()
  gate.resolve(offDoc())
  const off = await within(main).findByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    off.getAttribute('aria-disabled'),
    '22 D4.10: switched off, so the delete can act',
  ).not.toBe('true')
  // The next read of the same page answers switched on: the delete stays,
  // switched off (22 D4.10: a Delete that cannot act is never hidden).
  cleanup()
  vi.unstubAllGlobals()
  const gate2 = deferred()
  fakeBff({ modelProfile: () => gate2.promise })
  render(<App />)
  const m = screen.getByRole('main')
  await within(m).findByRole('heading', { level: 1, name: ID })
  gate2.resolve(onDoc())
  const on = await within(m).findByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    on.getAttribute('aria-disabled'),
    'the delete follows the switch: it stays, switched off',
  ).toBe('true')
  expect(
    within(m).getByText(en('model-profile-on-short')),
    'and the state reads on the page',
  ).toBeTruthy()
})

// ── 22 D.2: confirm in place ────────────────────────────────────────────

test('22 D.2 (as 22 D4.11 moves its place): pressing it opens a div role="group" inside the save bar, under its buttons, named by its paragraph model-profile-delete-question with the id, then confirm-delete then confirm-cancel, one empty status line and no alert, with the focus on confirm-cancel; Cancel, and Escape, close it on the opener; nothing is sent', async () => {
  const { bff, main } = await loaded()
  const opener = deleteButton(main)
  const group = await confirmGroup(main)
  expect(group.tagName, 'a div of role="group"').toBe('DIV')
  const bar = saveBar(main)
  expect(bar, 'the save bar of 22 D2.5').not.toBeNull()
  expect(
    group.closest('.save-bar'),
    '22 D4.11: the group is in the bar, where the bar grows for it',
  ).toBe(bar)
  const actions = barActions(main)
  expect(
    before(actions, group),
    '22 D4.11: and under the bar’s buttons, not above them',
  ).toBe(true)
  expect(
    group.closest('.save-actions'),
    '22 D4.11: not inside the button group, which is a row of buttons',
  ).toBeNull()
  const label = group.getAttribute('aria-labelledby')
  expect(label, 'the group is named by an element').toBeTruthy()
  const question = document.getElementById(label!)
  expect(question, 'the named element is its paragraph').not.toBeNull()
  expect(question!.tagName, 'a paragraph').toBe('P')
  expect(
    (question!.textContent ?? '').replace(/\s+/g, ' ').trim(),
    'the question names the profile, says the file stays and cannot be undone',
  ).toBe(en('model-profile-delete-question', ID))
  expect(
    question!.id,
    'the paragraph carries the id the group names',
  ).toBeTruthy()
  expect(group.contains(question), 'the question is in the group').toBe(true)
  const buttons = within(group).getAllByRole('button')
  expect(
    buttons.map((b) => (b.textContent ?? '').trim()),
    'the verb then Cancel',
  ).toEqual([en('confirm-delete'), en('confirm-cancel')])
  expect(groupStatus(group).textContent, 'the status line is empty').toBe('')
  expect(alertTexts(group), 'no alert: the place is empty').toEqual([])
  const cancel = confirmIn(group, 'confirm-cancel')
  expect(document.activeElement, 'the focus is on Cancel').toBe(cancel)
  expect(
    bff.to('DELETE', ROUTE),
    'nothing is sent before the verb',
  ).toHaveLength(0)
  // Cancel.
  press(cancel)
  expect(queryConfirm(main), 'the group is gone').toBeNull()
  expect(document.activeElement, 'the focus is on the opener').toBe(opener)
  // Escape.
  const again = await confirmGroup(main)
  expect(document.activeElement, 'Cancel again').toBe(
    confirmIn(again, 'confirm-cancel'),
  )
  fireEvent.keyDown(document.activeElement as Element, { key: 'Escape' })
  expect(queryConfirm(main), 'Escape closes it').toBeNull()
  expect(document.activeElement, 'on the opener').toBe(opener)
  expect(bff.to('DELETE', ROUTE), 'still nothing sent').toHaveLength(0)
})

// ── 22 D.3: the delete ──────────────────────────────────────────────────

test('22 D.3: confirm-delete sends DELETE /api/profiles/model/<id> once, the group’s status reads confirm-deleting with both buttons aria-disabled, and 204 replaces everything after the h1 by the focused paragraph model-profile-deleted and a model-profile-back link, with the unsaved-changes question off', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    modelProfileDelete: () => gate.promise,
  })
  // An edit first: the page holds it, and asks about it, until the file is gone.
  makeDirty(main)
  expect(asksBrowser(), 'the page holds the edit').toBe(true)
  const group = await confirmGroup(main)
  const del = confirmIn(group, 'confirm-delete')
  const cancel = confirmIn(group, 'confirm-cancel')
  press(del)
  expect(
    (await within(group).findByText(en('confirm-deleting'))).closest(
      '[role="status"]',
    ),
    'the group’s status line says what is happening',
  ).not.toBeNull()
  expect(del.getAttribute('aria-disabled'), 'Delete is inactive').toBe('true')
  expect(cancel.getAttribute('aria-disabled'), 'Cancel is inactive').toBe(
    'true',
  )
  press(del)
  press(cancel)
  const calls = bff.to('DELETE', ROUTE)
  expect(calls, 'a second press sends nothing').toHaveLength(1)
  expect(calls[0]!.path, 'the id is percent-encoded in the address').toBe(
    `/api/profiles/model/${encodeURIComponent(ID)}`,
  )
  gate.resolve(reply(204))
  await flushTasks()
  const h1 = within(main).getByRole('heading', { level: 1, name: ID })
  const done = within(main).getByText(en('model-profile-deleted', ID))
  expect(done.tagName, 'one paragraph').toBe('P')
  expect(done.getAttribute('tabindex'), 'it takes the focus').toBe('-1')
  expect(document.activeElement, 'and has it').toBe(done)
  expect(before(h1, done), 'it is the page’s content after the h1').toBe(true)
  const back = within(main).getByRole('link', {
    name: en('model-profile-back'),
  })
  expect(back.getAttribute('href'), 'a link to the list').toBe(
    '#/profiles/model',
  )
  expect(before(done, back), 'and the link follows it').toBe(true)
  expect(
    main.querySelector('p.model-profile-summary'),
    'the summary line is gone',
  ).toBeNull()
  expect(
    within(main).queryByRole('navigation', {
      name: en('model-profile-jump-label'),
    }),
    'the jump list is gone',
  ).toBeNull()
  expect(
    within(main).queryAllByRole('heading', { level: 3 }),
    'so are the cards',
  ).toHaveLength(0)
  expect(saveBar(main), 'the save bar is gone').toBeNull()
  expect(
    within(main).queryByRole('button', { name: en('model-profile-delete') }),
    'the delete is gone',
  ).toBeNull()
  expect(asksBrowser(), 'the unsaved-changes question no longer asks').toBe(
    false,
  )
})

test('22 D.3: while the DELETE runs Cancel is aria-disabled and a click on it closes nothing, the group, its question and its status line staying as they are; the 204 that lands afterwards is handled as 22 D.3 says (17b review R1)', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    modelProfileDelete: () => gate.promise,
  })
  const opener = deleteButton(main)
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  const cancel = confirmIn(group, 'confirm-cancel')
  await within(group).findByText(en('confirm-deleting'))
  expect(cancel.getAttribute('aria-disabled'), 'Cancel is inactive').toBe(
    'true',
  )
  // The click: an inactive control answers nothing (its aria-disabled is not a
  // shield, so the handler must be inert too: 22 D.2 closes an idle group).
  press(cancel)
  expect(queryConfirm(main), 'the group is still open').toBe(group)
  expect(
    groupStatus(group).textContent,
    'its status line still says what is happening',
  ).toBe(en('confirm-deleting'))
  expect(
    (questionOf(group)?.textContent ?? '').replace(/\s+/g, ' ').trim(),
    'its question is the same',
  ).toBe(en('model-profile-delete-question', ID))
  expect(
    document.activeElement,
    'the focus did not move to the opener',
  ).not.toBe(opener)
  expect(bff.to('DELETE', ROUTE), 'and the click sent nothing').toHaveLength(1)
  // The delete that was already sent is answered as 22 D.3.
  gate.resolve(reply(204))
  await flushTasks()
  const done = within(main).getByText(en('model-profile-deleted', ID))
  expect(document.activeElement, 'the focused paragraph of 22 D.3').toBe(done)
  expect(
    within(main)
      .getByRole('link', { name: en('model-profile-back') })
      .getAttribute('href'),
    'with the link to the list',
  ).toBe('#/profiles/model')
})

test('22 D.3: after a delete of one profile the address of another is read and shown as that profile’s own page, and the paragraph saying the first was deleted is nowhere (17b review R2)', async () => {
  const { bff, main } = await loaded({
    modelProfileDelete: () => reply(204),
  })
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  const done = await within(main).findByText(en('model-profile-deleted', ID))
  expect(
    document.activeElement,
    'the page of the delete is the paragraph',
  ).toBe(done)
  // A new address: the page of another profile, read from its own file.
  navigate(OTHER_HASH)
  await flushTasks()
  const h1 = await within(main).findByRole('heading', { level: 1, name: OTHER })
  expect(document.activeElement, 'its h1 takes the focus').toBe(h1)
  expect(
    within(main).queryByText(en('model-profile-deleted', ID)),
    'the deleted paragraph of the first id is gone',
  ).toBeNull()
  expect(
    within(main).queryByText(en('model-profile-deleted', OTHER)),
    'and the new page says nothing was deleted',
  ).toBeNull()
  expect(
    bff.to('GET', OTHER_ROUTE).length,
    'the new profile was read',
  ).toBeGreaterThan(0)
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  await within(main).findByRole('button', {
    name: en('model-profile-delete'),
  })
})

// ── 22 D.3: the id is percent-encoded in the address ────────────────────

test('22 D.3: a profile whose id needs encoding is deleted at a percent-encoded address: the raw DELETE path carries %20 and %23, no raw space and no raw #, and the 204 lands on the paragraph naming that id (22D review, the encoded path)', async () => {
  // The page is opened at the encoded address, as a link to this profile
  // writes it (route.ts modelProfileHref); the read answers the id as the
  // manager spells it, so the page shows the raw one.
  startAt(SPACED_HASH)
  const bff = fakeBff({ modelProfile: (got) => offDoc(got) })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: SPACED })
  // The read lands: the offer only exists once the document is there (22 D.1).
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  press(within(main).getByRole('button', { name: en('model-profile-delete') }))
  const group = await within(main).findByRole('group', {
    name: en('model-profile-delete-question', SPACED),
  })
  press(within(group).getByRole('button', { name: en('confirm-delete') }))
  const dels = bff.calls.filter((c) => c.method === 'DELETE')
  expect(dels, 'one delete was sent').toHaveLength(1)
  expect(dels[0]!.path, 'the raw path is the percent-encoded one').toBe(
    SPACED_ROUTE,
  )
  expect(dels[0]!.path, 'no raw space in the address').not.toContain(' ')
  expect(dels[0]!.path, 'no raw # in the address').not.toContain('#')
  expect(
    bff.to('GET', SPACED_ROUTE).length,
    'guard, green today: the page read this profile at the encoded address too',
  ).toBeGreaterThan(0)
  await flushTasks()
  const done = await within(main).findByText(
    en('model-profile-deleted', SPACED),
  )
  expect(done.getAttribute('tabindex'), 'the paragraph of 22 D.3').toBe('-1')
  expect(document.activeElement, 'and it has the focus').toBe(done)
  expect(
    within(main)
      .getByRole('link', { name: en('model-profile-back') })
      .getAttribute('href'),
    'with the link to the list',
  ).toBe('#/profiles/model')
})

// ── 22 D.4: switched on meanwhile ───────────────────────────────────────

test('22 D.4 (as 22 D4.11 amends it): a 409 model-profile-enabled reads the document again: the group goes, the delete stays switched off, its text lands in the bar’s alert with the focus on the delete, and the state line says model-profile-on-short', async () => {
  let reads_ = 0
  const { bff, main } = await loaded({
    modelProfile: (id) => (++reads_ === 1 ? offDoc(id) : onDoc(id)),
    modelProfileDelete: () =>
      reply(409, { key: 'model-profile-enabled', args: [ID] }),
  })
  const text = en('api-model-profile-enabled', ID)
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  const alert = await within(main).findByText(text)
  expect(
    alert.closest('[role="alert"]'),
    'the text is in an alert',
  ).not.toBeNull()
  // The read again lands: the switch shows, the group goes, and the text is
  // the bar's: the bar outlives the group.
  await within(main).findByText(en('model-profile-on-short'))
  expect(queryConfirm(main), 'the confirm is gone').toBeNull()
  expect(reads(bff), 'the document was read again').toBe(2)
  expect(bff.to('GET', ROUTE), 'one read, then the re-read').toHaveLength(2)
  const kept = within(main).getByText(text)
  expect(kept.tagName, 'the text is in a paragraph').toBe('P')
  expect(kept.getAttribute('role'), 'an alert').toBe('alert')
  expect(kept.getAttribute('tabindex'), 'a place the focus can take').toBe('-1')
  expect(
    kept.closest('.save-bar'),
    '22 D4.11: it is the bar’s alert, the place the bar keeps for a refusal',
  ).toBe(saveBar(main))
  const del = deleteButton(main)
  expect(
    del.getAttribute('aria-disabled'),
    '22 D4.11: the delete stays switched off, the page says why',
  ).toBe('true')
  expect(document.activeElement, '22 D4.11: the focus moves to Delete').toBe(
    del,
  )
  expect(document.activeElement, 'the focus did not drop to body').not.toBe(
    document.body,
  )
  // The next read of the page leaves it out.
  navigate(OTHER_HASH)
  await flushTasks()
  await within(main).findByRole('heading', { level: 1, name: OTHER })
  navigate(HASH)
  await flushTasks()
  await within(main).findByRole('heading', { level: 1, name: ID })
  expect(
    within(main).queryByText(text),
    'the next read leaves the refusal text out',
  ).toBeNull()
})

// ── 22 D.5: assigned ────────────────────────────────────────────────────

test('22 D.5: a 409 model-profile-assigned shows api-model-profile-assigned with the profile and the instances in the group’s alert; the group stays open with its buttons and the focus stays on confirm-delete, and nothing is read again', async () => {
  const { bff, main } = await loaded({
    modelProfileDelete: () =>
      reply(409, { key: 'model-profile-assigned', args: [ID, TAKEN] }),
  })
  const group = await confirmGroup(main)
  const del = confirmIn(group, 'confirm-delete')
  press(del)
  const alert = await within(group).findByText(
    en('api-model-profile-assigned', ID, TAKEN),
  )
  expect(
    alert.closest('[role="alert"]'),
    'the text is in a role="alert"',
  ).not.toBeNull()
  expect(group.contains(alert), 'the alert is the group’s').toBe(true)
  expect(
    alertTexts(group).join(' '),
    'both args are in it: the profile and the instances',
  ).toContain(TAKEN)
  expect(queryConfirm(main), 'the group stays open').toBe(group)
  expect(groupStatus(group).textContent, 'its status line is empty again').toBe(
    '',
  )
  expect(
    within(group)
      .getAllByRole('button')
      .map((b) => (b.textContent ?? '').trim()),
    'the group keeps its two buttons',
  ).toEqual([en('confirm-delete'), en('confirm-cancel')])
  expect(
    confirmIn(group, 'confirm-delete').getAttribute('aria-disabled'),
    'Delete is active again',
  ).not.toBe('true')
  expect(document.activeElement, 'the focus stays on the verb').toBe(del)
  expect(reads(bff), 'nothing is read again').toBe(1)
  expect(bff.to('DELETE', ROUTE), 'one delete sent').toHaveLength(1)
})

// ── 22 D.6: gone ────────────────────────────────────────────────────────

test('22 D.6: a 404 no-model-profile-named shows its text in the group’s alert, and the page reads again and shows the not-found state: the model-profile-not-found paragraph with tabindex="-1" and the focus', async () => {
  let reads_ = 0
  const { bff, main } = await loaded({
    modelProfile: (id) =>
      ++reads_ === 1
        ? offDoc(id)
        : reply(404, { key: 'no-model-profile-named', args: [id] }),
    modelProfileDelete: () =>
      reply(404, { key: 'no-model-profile-named', args: [ID] }),
  })
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  const alert = await within(group).findByText(
    en('api-no-model-profile-named', ID),
  )
  expect(
    alert.closest('[role="alert"]'),
    'the text is in the group’s alert',
  ).not.toBeNull()
  const gone = await within(main).findByText(en('model-profile-not-found', ID))
  expect(gone.tagName, 'one paragraph').toBe('P')
  expect(gone.getAttribute('tabindex'), 'a place the focus can take').toBe('-1')
  // The focus is an effect after the render the finder resolves on (as D.7).
  await waitFor(() =>
    expect(document.activeElement, 'it has the focus').toBe(gone),
  )
  expect(document.activeElement, 'the focus did not drop to body').not.toBe(
    document.body,
  )
  expect(
    within(main).queryByRole('button', { name: en('model-profile-delete') }),
    'the delete is gone with the profile',
  ).toBeNull()
  expect(reads(bff), 'the document was read again').toBe(2)
})

// ── 22 D.7: the other failures ──────────────────────────────────────────

test('22 D.7: a 504 client-timeout shows delete-unknown in the group’s alert and never request-failed, then the document is read again: gone, the page is the not-found state with its paragraph focused; still there, the alert stays and the offer comes back', async () => {
  // Gone: the re-read answers 404.
  let reads_ = 0
  const { bff, main } = await loaded({
    modelProfile: (id) =>
      ++reads_ === 1
        ? offDoc(id)
        : reply(404, { key: 'no-model-profile-named', args: [id] }),
    modelProfileDelete: () => reply(504, { key: 'client-timeout', args: [] }),
  })
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  const alert = await within(group).findByText(en('delete-unknown'))
  expect(
    alert.closest('[role="alert"]'),
    'the not-confirmed text is in the group’s alert',
  ).not.toBeNull()
  expect(
    alertTexts(group).join(' '),
    'a 504 is never request-failed (17f.10)',
  ).not.toContain(en('request-failed'))
  const gone = await within(main).findByText(en('model-profile-not-found', ID))
  expect(gone.getAttribute('tabindex'), 'the not-found paragraph').toBe('-1')
  // The paragraph's focus arrives with the effect after the render the finder
  // resolves on: a synchronous activeElement read reds only under load.
  await waitFor(() =>
    expect(document.activeElement, 'and it has the focus').toBe(gone),
  )
  expect(reads(bff), 'the document was read again').toBe(2)
  cleanup()
  vi.unstubAllGlobals()
  // Still there: the re-read answers the same switched-off profile.
  const second = await loaded({
    modelProfileDelete: () => reply(504, { key: 'client-timeout', args: [] }),
  })
  const g2 = await confirmGroup(second.main)
  press(confirmIn(g2, 'confirm-delete'))
  const kept = await within(second.main).findByText(en('delete-unknown'))
  expect(
    kept.closest('[role="alert"]'),
    'the alert stays after the re-read',
  ).not.toBeNull()
  expect(
    within(second.main).queryByText(en('request-failed')),
    'and it is still not request-failed',
  ).toBeNull()
  expect(
    within(second.main).queryByText(en('model-profile-not-found', ID)),
    'the page is not the not-found state',
  ).toBeNull()
  await within(second.main).findByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(reads(second.bff), 'the document was read again').toBe(2)
})

test('22 D.7: 503, a 403 session-not-administrator, a 422 with a key the page does not know and a 500 each show their text in the group’s alert with the focus on confirm-delete, keep the group open with its buttons, leave its status line empty and read nothing again', async () => {
  const cases: [string, () => Response, string][] = [
    [
      '503 client-unavailable',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
    [
      '403 session-not-administrator',
      () => reply(403, { key: 'session-not-administrator', args: [] }),
      en('session-revoked'),
    ],
    [
      '422 with a key the page does not know',
      () =>
        reply(422, {
          key: 'model-profile-bad-source',
          field: 'values',
          args: [ID],
        }),
      en('api-other', 'model-profile-bad-source'),
    ],
    [
      '500',
      () => reply(500, { key: 'client-write-failed', args: [] }),
      en('request-failed'),
    ],
  ]
  for (const [what, answer, text] of cases) {
    const { bff, main } = await loaded({ modelProfileDelete: answer })
    const group = await confirmGroup(main)
    const del = confirmIn(group, 'confirm-delete')
    press(del)
    const alert = await within(group).findByText(text)
    expect(
      alert.closest('[role="alert"]'),
      `${what}: the text is in a role="alert"`,
    ).not.toBeNull()
    expect(group.contains(alert), `${what}: the alert is the group’s`).toBe(
      true,
    )
    expect(
      groupStatus(group).textContent,
      `${what}: the status line is empty`,
    ).toBe('')
    expect(
      within(group)
        .getAllByRole('button')
        .map((b) => (b.textContent ?? '').trim()),
      `${what}: the group keeps its two buttons`,
    ).toEqual([en('confirm-delete'), en('confirm-cancel')])
    expect(
      confirmIn(group, 'confirm-delete').getAttribute('aria-disabled'),
      `${what}: Delete is active again`,
    ).not.toBe('true')
    // The reply's focus to the verb is an effect after the render the alert
    // finder resolves on: poll it, a synchronous read reds only under load.
    await waitFor(() =>
      expect(document.activeElement, `${what}: the focus is on the verb`).toBe(
        del,
      ),
    )
    expect(reads(bff), `${what}: nothing is read again`).toBe(1)
    expect(bff.to('DELETE', ROUTE), `${what}: one delete sent`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('22 D.7: a 401 shows session-ended-retry with a Sign in after it in the group; the Sign in opens the sign-in Dialog, and after a sign-in the focus is back on confirm-delete', async () => {
  const { bff, main } = await loaded({
    modelProfileDelete: () => reply(401, { key: 'session-required', args: [] }),
  })
  const group = await confirmGroup(main)
  const del = confirmIn(group, 'confirm-delete')
  press(del)
  const alert = await within(group).findByText(en('session-ended-retry'))
  expect(alert.closest('[role="alert"]'), 'the group’s alert').not.toBeNull()
  const signIns = within(group).getAllByRole('button', {
    name: en('session-sign-in'),
  })
  expect(signIns, 'one Sign in in the group').toHaveLength(1)
  expect(before(alert, signIns[0]!), 'the Sign in is after the alert').toBe(
    true,
  )
  press(signIns[0]!)
  await screen.findByRole('dialog', { name: en('signin-title') })
  await signInInDialog('alice')
  expect(document.activeElement, 'the focus is back on the verb').toBe(del)
  expect(queryConfirm(main), 'the group is still open').not.toBeNull()
  expect(bff.to('DELETE', ROUTE), 'nothing is sent again').toHaveLength(1)
})

// ── 22D review (the designer, 2026-11-23): D1 and D2, the sheet ───────────

// The stylesheet reader (the way `Reflow.test.ts` reads `index.css`, a small
// local copy: a test file imports no other test file). The review's two fixes
// are declarations, and jsdom lays nothing out, so the sheet is what pins them;
// the measured width and gap are the designer's re-check (`build/ux-review`).

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

// Top-level rules only: both fixes apply at every width, not in a media block.
const TOP = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
  .filter((m) => opened(m.index ?? 0) === 0)
  .map((m) => ({ compact: m[1].replace(/\s+/g, ''), body: m[2] }))

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

/** The declarations that reach `el` from the top-level rules: the rules whose
 *  selector is the element's tag, one of its classes, a compound of them, or
 *  its id, ordered by class count then sheet order, merged. A selector with a
 *  descendant, a pseudo-class, an attribute or a combinator is skipped: it
 *  needs a tree jsdom does not lay out. The tag, classes and id are read off
 *  the page, so the pin follows the names the coder chose. */
function cascade(el: HTMLElement): Record<string, string> {
  const own = new Set([el.tagName.toLowerCase()])
  for (const c of el.classList) own.add(`.${c}`)
  if (el.id) own.add(`#${el.id}`)
  const hits: { body: string; spec: number; at: number }[] = []
  TOP.forEach((rule, at) => {
    let spec = -1
    for (const part of rule.compact.split(',')) {
      const tokens = part.match(/[#.]?[-a-z0-9_]+/g) ?? []
      if (tokens.length > 0 && tokens.every((t) => own.has(t)))
        spec = Math.max(spec, tokens.filter((t) => t.startsWith('.')).length)
    }
    if (spec >= 0) hits.push({ body: rule.body, spec, at })
  })
  hits.sort((a, b) => a.spec - b.spec || a.at - b.at)
  const out: Record<string, string> = {}
  for (const h of hits)
    for (const d of h.body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) out[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
  return out
}

// 22 D.1 (22D review D1), the opener not stretching as a grid item of the
// 1280 px jump grid, is retired here: 22 D2.1 moved the opener out of the
// settings section into the status row under the summary line, so it is no
// grid item at all. The row's own shape, the 40 px square the − shares with
// .ot-add and its flex: none (the width the review was about) are 22 D2.2 and
// 22 D2.9 of ModelProfileDelete2.test.tsx.

test('22 D.2 (22D review D2): the confirm group keeps margin-top: var(--space-3) from its opener (the sheet)', async () => {
  // The group starts at the button’s bottom edge (gap 0, the question p has no
  // margin), so the question touches the opener (the review, D2, at both
  // widths). The fix is the group’s own margin-top, one token, on this page
  // and on 17b’s. The class is the coder’s (the confirm namespace, allowed by
  // Reflow R1.8) and is read off the page, so the pin follows the name; a rule
  // scoped by a combinator (an adjacent sibling from the opener) reaches the
  // group in a browser but not through this cascade, so the rule must hang off
  // the group’s own class.
  const { main } = await loaded()
  const group = await confirmGroup(main)
  expect(
    norm(cascade(group)['margin-top'] ?? ''),
    'D2: the confirm group is separated from its opener by this: margin-top',
  ).toBe('var(--space-3)')
})
