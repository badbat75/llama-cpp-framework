// Brief 16, slice 17b (fe/briefs/16-server-profiles-and-instances.md): deleting a
// server profile from its own page, while no instance runs it. The page, its read
// and its save are ServerProfilePage1.test.tsx; the create on the list page is
// ServerProfiles2.test.tsx; 17b.10 (the strings file gains exactly 17b's keys) is
// `1a strings` in App.test.tsx, which compares the strings file against the test's
// EN map, where 17b's keys are listed. Test names cite the brief first, `16 17b.6`,
// because brief 17's UX pass owns the bare `17b.n` names.
//
// Slice 22D2 (fe/briefs/22-model-sources.md, section 22D2) gives this page the
// status row of the model profile page: the delete becomes a minus icon button
// in a div.page-status under the h1, the row says whether an instance uses the
// profile, and the confirm and the refusal paragraph of 17b.9 sit under that
// row instead of after the Value list. The answers of 17b.6 to 17b.9 stand;
// what changes here is the place. The row, the minus and the strings are
// 22 D2.8 in ModelProfileDelete2.test.tsx.
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
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  navigate,
  reply,
  serverProfileDoc,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'qwen-27b'
const HASH = `#/profiles/server/${ID}`
const ROUTE = `/api/profiles/server/${ID}`
/** A second profile: the page a deleted one gives place to (17b review R2). */
const OTHER = 'edge-box'
const OTHER_ROUTE = `/api/profiles/server/${OTHER}`
/** The instance that takes the profile in 17b.9's 409. */
const TAKEN = 'coder'

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

/** The document of a profile no instance runs. */
function freeDoc() {
  return reply(200, serverProfileDoc({}, { instance: null }))
}

/** The document of a profile instance `instance` runs. */
function usedDoc(instance: string) {
  return reply(200, serverProfileDoc({}, { instance }))
}

/** The page, with its Server form read as a free profile. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ serverProfile: () => freeDoc(), ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The Server form's Port box: the page's own field, not the confirm's. */
function portBox(main: HTMLElement): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: en('server-port') })
  return (
    g
      ? within(g).getByRole('textbox', { name: en('server-port') })
      : within(main).getByRole('textbox', { name: en('server-port') })
  ) as HTMLInputElement
}

function deleteButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('server-profile-delete') })
}

/** The status row of 22 D2.8, the place of the delete since 22D2. */
function statusRow(main: HTMLElement): Element | null {
  return main.querySelector('div.page-status')
}

/** The words a node shows, whitespace folded. */
function words(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The confirm group's accessible name: its question (17b.7). */
const QUESTION = en('server-profile-delete-question', ID)

/** The paragraph the group is named by, the question it asks. */
function questionOf(group: HTMLElement): HTMLElement | null {
  const id = group.getAttribute('aria-labelledby')
  return id === null ? null : document.getElementById(id)
}

/** The confirm group, open or not. The Server form's fields are groups as
 *  well, so the pin asks for the one its question names. */
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

/** The group's status line (17b.8: the group holds one). */
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

/** The Value list's Used by cell. */
function usedByCell(main: HTMLElement): HTMLElement {
  const dl = main.querySelector('dl.assets')
  expect(dl, 'the Value list').not.toBeNull()
  const dd = dl!.querySelector('dd')
  expect(dd, 'its row').not.toBeNull()
  return dd as HTMLElement
}

/** Signs in as `user` in the open sign-in dialog. */
async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(d.tagName, 'a native <dialog>').toBe('DIALOG')
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), user)
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
}

/** Asks the browser whether leaving the page would be held. */
function asksBrowser(): boolean {
  const e = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(e)
  return e.defaultPrevented
}

// ── 17b.6: delete is offered while unused ───────────────────────────────

test('16 17b.6: a profile whose document says no instance runs offers the server-profile-delete delete in the status row under the h1, above the Value list and the Server section, the row saying server-profile-unused; one whose document names an instance offers nothing, the row saying server-profile-used-by; the button follows the document read', async () => {
  const { main } = await loaded()
  const h1 = within(main).getByRole('heading', { level: 1, name: ID })
  const row = statusRow(main)
  expect(row, 'the status row of 22 D2.8').not.toBeNull()
  expect(before(h1, row!), 'it is under the h1').toBe(true)
  const dl = main.querySelector('dl.assets')
  expect(dl, 'the Value list is there').not.toBeNull()
  expect(before(row!, dl!), 'and above the Value list').toBe(true)
  const button = deleteButton(main)
  expect(button.parentElement, 'the delete is in the row').toBe(row)
  expect(button.closest('section'), 'outside the Server section').toBeNull()
  expect(
    words(row!.querySelector('p')),
    '22 D2.8: an unused profile says no instance uses it',
  ).toBe(en('server-profile-unused'))
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
  expect(before(button, h2), 'and it sits before the Server section').toBe(true)
  cleanup()
  vi.unstubAllGlobals()
  // A profile an instance runs: nothing, and the row names the instance.
  const used = await loaded({ serverProfile: () => usedDoc(TAKEN) })
  expect(
    within(used.main).queryByRole('button', {
      name: en('server-profile-delete'),
    }),
    'no delete for a profile in use',
  ).toBeNull()
  expect(
    words(statusRow(used.main)!.querySelector('p')),
    '22 D2.8: the row names the instance that runs it',
  ).toBe(en('server-profile-used-by', TAKEN))
  cleanup()
  vi.unstubAllGlobals()
  // It follows the read.
  const gate = deferred()
  fakeBff({ serverProfile: () => gate.promise })
  render(<App />)
  const m = screen.getByRole('main')
  await within(m).findByRole('heading', { level: 1, name: ID })
  expect(
    within(m).queryByRole('button', { name: en('server-profile-delete') }),
    'nothing while the document is open',
  ).toBeNull()
  gate.resolve(freeDoc())
  await within(m).findByRole('button', { name: en('server-profile-delete') })
})

// ── 17b.7: confirm in place ─────────────────────────────────────────────

test('16 17b.7: pressing it shows right after it a div role="group" named by its paragraph server-profile-delete-question with the id, then confirm-delete then confirm-cancel, one empty status line and no alert, with the focus on confirm-cancel; Cancel, and Escape, close it on the opener; nothing is sent', async () => {
  const { bff, main } = await loaded()
  const opener = deleteButton(main)
  const group = await confirmGroup(main)
  expect(group.tagName, 'a div of role="group"').toBe('DIV')
  const row = statusRow(main)
  expect(row, 'the status row of 22 D2.8').not.toBeNull()
  expect(
    row!.nextElementSibling,
    '22 D2.8: the group is right after the row, not after the opener',
  ).toBe(group)
  const label = group.getAttribute('aria-labelledby')
  expect(label, 'the group is named by an element').toBeTruthy()
  const question = document.getElementById(label!)
  expect(question, 'the named element is its paragraph').not.toBeNull()
  expect(question!.tagName, 'a paragraph').toBe('P')
  expect(
    (question!.textContent ?? '').replace(/\s+/g, ' ').trim(),
    'the question names the profile and says it cannot be undone',
  ).toBe(en('server-profile-delete-question', ID))
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
  expect(
    within(group).queryByRole('button', { name: en('confirm-cancel') }),
    'inside the group',
  ).not.toBeNull()
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

// ── 17b.8: the delete ───────────────────────────────────────────────────

test('16 17b.8: confirm-delete sends DELETE /api/profiles/server/<id> once, the group’s status reads confirm-deleting with both buttons aria-disabled, and 204 replaces everything after the h1 by the focused paragraph server-profile-deleted and a server-profile-back link, with the unsaved-changes question off', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    serverProfileDelete: () => gate.promise,
  })
  // An edit first: the page holds it, and asks about it, until the file is gone.
  type(portBox(main), '9999')
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
  gate.resolve(reply(204))
  await flushTasks()
  const h1 = within(main).getByRole('heading', { level: 1, name: ID })
  const done = within(main).getByText(en('server-profile-deleted', ID))
  expect(done.tagName, 'one paragraph').toBe('P')
  expect(done.getAttribute('tabindex'), 'it takes the focus').toBe('-1')
  expect(document.activeElement, 'and has it').toBe(done)
  expect(before(h1, done), 'it is the page’s content after the h1').toBe(true)
  const back = within(main).getByRole('link', {
    name: en('server-profile-back'),
  })
  expect(back.getAttribute('href'), 'a link to the list').toBe(
    '#/profiles/server',
  )
  expect(before(done, back), 'and the link follows it').toBe(true)
  expect(main.querySelector('dl.assets'), 'the Value list is gone').toBeNull()
  expect(
    within(main).queryByRole('heading', { level: 2, name: en('server-title') }),
    'the Server section is gone',
  ).toBeNull()
  expect(
    within(main).queryAllByRole('heading', { level: 3 }),
    'so are its fields',
  ).toHaveLength(0)
  expect(
    within(main).queryByRole('button', { name: en('save-button') }),
    'the save bar is gone',
  ).toBeNull()
  expect(
    within(main).queryByRole('button', { name: en('server-profile-delete') }),
    'the delete is gone',
  ).toBeNull()
  expect(asksBrowser(), 'the unsaved-changes question no longer asks').toBe(
    false,
  )
})

test('16 17b.8: while the DELETE runs Cancel is aria-disabled and a click on it closes nothing, the group, its question and its status line staying as they are; the 204 that lands afterwards is handled as 17b.8 says (17b review R1)', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    serverProfileDelete: () => gate.promise,
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
  // shield, so the handler must be inert too: 17b.7 closes an idle group).
  press(cancel)
  expect(queryConfirm(main), 'the group is still open').toBe(group)
  expect(
    groupStatus(group).textContent,
    'its status line still says what is happening',
  ).toBe(en('confirm-deleting'))
  expect(
    (questionOf(group)?.textContent ?? '').replace(/\s+/g, ' ').trim(),
    'its question is the same',
  ).toBe(en('server-profile-delete-question', ID))
  expect(
    document.activeElement,
    'the focus did not move to the opener',
  ).not.toBe(opener)
  expect(bff.to('DELETE', ROUTE), 'and the click sent nothing').toHaveLength(1)
  // The delete that was already sent is answered as 17b.8.
  gate.resolve(reply(204))
  await flushTasks()
  const done = within(main).getByText(en('server-profile-deleted', ID))
  expect(document.activeElement, 'the focused paragraph of 17b.8').toBe(done)
  expect(
    within(main)
      .getByRole('link', { name: en('server-profile-back') })
      .getAttribute('href'),
    'with the link to the list',
  ).toBe('#/profiles/server')
})

test('16 17b.8: after a delete of one profile the address of another is read and shown as that profile’s own page, and the paragraph saying the first was deleted is nowhere (17b review R2)', async () => {
  const { bff, main } = await loaded({
    serverProfileDelete: () => reply(204),
  })
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  const done = await within(main).findByText(en('server-profile-deleted', ID))
  expect(
    document.activeElement,
    'the page of the delete is the paragraph',
  ).toBe(done)
  // A new address: the page of another profile, read from its own file.
  navigate(`#/profiles/server/${OTHER}`)
  await flushTasks()
  const h1 = await within(main).findByRole('heading', { level: 1, name: OTHER })
  expect(document.activeElement, 'its h1 takes the focus').toBe(h1)
  expect(
    within(main).queryByText(en('server-profile-deleted', ID)),
    'the deleted paragraph of the first id is gone',
  ).toBeNull()
  expect(
    within(main).queryByText(en('server-profile-deleted', OTHER)),
    'and the new page says nothing was deleted',
  ).toBeNull()
  expect(
    bff.to('GET', OTHER_ROUTE).length,
    'the new profile was read',
  ).toBeGreaterThan(0)
  await within(main).findByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
  await within(main).findByRole('button', { name: en('server-profile-delete') })
  expect(usedByCell(main).textContent, 'its own Used by row').toBe(
    en('server-profiles-unused'),
  )
})

// ── 17b.9: delete refusals ──────────────────────────────────────────────

test('16 17b.9: a 409 client-server-profile-in-use shows its text in the group’s alert, and the document is read again: the Used by row names the instance and the button is gone', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    serverProfile: () => (++reads === 1 ? freeDoc() : usedDoc(TAKEN)),
    serverProfileDelete: () =>
      reply(409, {
        key: 'client-server-profile-in-use',
        args: [ID, TAKEN],
      }),
  })
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  const alert = await within(group).findByText(
    en('api-client-server-profile-in-use', ID, TAKEN),
  )
  expect(
    alert.closest('[role="alert"]'),
    'the text is in the group’s alert',
  ).not.toBeNull()
  expect(group.contains(alert), 'the alert is the group’s').toBe(true)
  await within(usedByCell(main)).findByRole('link', { name: TAKEN })
  expect(
    within(usedByCell(main))
      .getByRole('link', { name: TAKEN })
      .getAttribute('href'),
    'the row names the instance, at its page',
  ).toBe(`#/instances/${TAKEN}`)
  expect(reads, 'the document was read again').toBe(2)
  expect(bff.to('GET', ROUTE), 'one read, then the re-read').toHaveLength(2)
  expect(
    within(main).queryByRole('button', { name: en('server-profile-delete') }),
    'the button is gone',
  ).toBeNull()
  expect(queryConfirm(main), 'and so is the confirm').toBeNull()
})

test('16 17b.9: after the 409 client-server-profile-in-use and the read again, the in-use text stays in a p[role="alert"][tabindex="-1"] right after the status row of 22 D2.8, and that paragraph has the focus: the focus never ends on body (17b review D2)', async () => {
  let reads = 0
  const { main } = await loaded({
    serverProfile: () => (++reads === 1 ? freeDoc() : usedDoc(TAKEN)),
    serverProfileDelete: () =>
      reply(409, {
        key: 'client-server-profile-in-use',
        args: [ID, TAKEN],
      }),
  })
  const text = en('api-client-server-profile-in-use', ID, TAKEN)
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  await within(group).findByText(text)
  const dl = main.querySelector('dl.assets')
  expect(dl, 'the Value list').not.toBeNull()
  // The read again lands: the row names the instance, the delete and its group
  // go with it, and the alert must outlive them.
  await within(usedByCell(main)).findByRole('link', { name: TAKEN })
  expect(
    within(main).queryByRole('button', { name: en('server-profile-delete') }),
    'the button is gone',
  ).toBeNull()
  expect(queryConfirm(main), 'and so is the confirm').toBeNull()
  const alert = within(main).getByText(text)
  expect(alert.tagName, 'the text is in a paragraph').toBe('P')
  expect(alert.getAttribute('role'), 'an alert').toBe('alert')
  expect(alert.getAttribute('tabindex'), 'a place the focus can take').toBe(
    '-1',
  )
  const row = statusRow(main)
  expect(row, 'the status row of 22 D2.8').not.toBeNull()
  expect(
    row!.nextElementSibling,
    '22 D2.8: right after the row, where the group was',
  ).toBe(alert)
  expect(document.activeElement, 'and it has the focus').toBe(alert)
  expect(document.activeElement, 'the focus did not drop to body').not.toBe(
    document.body,
  )
})

test('16 17b.9: a 404 client-no-server-profile shows its text in the group’s alert, and the page reads again and shows server-profile-not-found', async () => {
  let reads = 0
  const { main } = await loaded({
    serverProfile: () =>
      ++reads === 1
        ? freeDoc()
        : reply(404, { key: 'client-no-server-profile', args: [ID] }),
    serverProfileDelete: () =>
      reply(404, { key: 'client-no-server-profile', args: [ID] }),
  })
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  const alert = await within(group).findByText(
    en('api-client-no-server-profile'),
  )
  expect(
    alert.closest('[role="alert"]'),
    'the text is in the group’s alert',
  ).not.toBeNull()
  const gone = await within(main).findByText(en('server-profile-not-found', ID))
  expect(gone, 'the page is the not-found page').toBeTruthy()
  expect(
    within(main).queryByRole('button', { name: en('server-profile-delete') }),
    'the delete is gone with the profile',
  ).toBeNull()
  expect(reads, 'the document was read again').toBe(2)
})

test('16 17b.9: after the 404 client-no-server-profile and the read again, the server-profile-not-found paragraph takes the focus: the focus never ends on body (17b review D2)', async () => {
  let reads = 0
  const { main } = await loaded({
    serverProfile: () =>
      ++reads === 1
        ? freeDoc()
        : reply(404, { key: 'client-no-server-profile', args: [ID] }),
    serverProfileDelete: () =>
      reply(404, { key: 'client-no-server-profile', args: [ID] }),
  })
  const group = await confirmGroup(main)
  press(confirmIn(group, 'confirm-delete'))
  await within(group).findByText(en('api-client-no-server-profile'))
  const gone = await within(main).findByText(en('server-profile-not-found', ID))
  expect(gone.tagName, 'one paragraph').toBe('P')
  expect(gone.getAttribute('tabindex'), 'a place the focus can take').toBe('-1')
  expect(document.activeElement, 'it has the focus').toBe(gone)
  expect(document.activeElement, 'the focus did not drop to body').not.toBe(
    document.body,
  )
})

test('16 17b.9: 503, 502 and a 403 session-not-administrator each show their text in the group’s alert, keep the group open with its buttons, leave its status line empty and read nothing again', async () => {
  const cases: [string, () => Response, string][] = [
    [
      '503 client-unavailable',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
    [
      '502 client-other',
      () => reply(502, { key: 'client-other', args: [] }),
      en('request-failed'),
    ],
    [
      '403 session-not-administrator',
      () => reply(403, { key: 'session-not-administrator', args: [] }),
      en('session-revoked'),
    ],
  ]
  for (const [what, answer, text] of cases) {
    const { bff, main } = await loaded({ serverProfileDelete: answer })
    const group = await confirmGroup(main)
    press(confirmIn(group, 'confirm-delete'))
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
    expect(bff.to('GET', ROUTE), `${what}: nothing is read again`).toHaveLength(
      1,
    )
    expect(bff.to('DELETE', ROUTE), `${what}: one delete sent`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('16 17b.9: a 401 shows session-ended-retry with a Sign in after it in the group; the Sign in opens the sign-in Dialog, and after a sign-in the focus is back on confirm-delete', async () => {
  const { bff, main } = await loaded({
    serverProfileDelete: () =>
      reply(401, { key: 'session-required', args: [] }),
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

// ── 22D review (the designer, 2026-11-23): D2, the sheet, this page ────────

// The stylesheet reader (the way `Reflow.test.ts` reads `index.css`, a small
// local copy: a test file imports no other test file). The review’s fix is a
// declaration, and jsdom lays nothing out, so the sheet is what pins it; the
// measured gap is the designer’s re-check.

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

// Top-level rules only: the gap applies at every width, not in a media block.
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

test('16 17b.7 (22D review D2): the confirm group keeps margin-top: var(--space-3) from its opener (the sheet)', async () => {
  // The review’s D2 asks for the same space on both pages, "so the two pages
  // stay alike": the group of this page’s ConfirmDelete (ServerProfilePage.tsx
  // ~507) starts at the opener’s bottom edge (gap 0, the question p has no
  // margin). The class is the coder’s (the confirm namespace, allowed by
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
