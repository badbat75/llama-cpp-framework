// Leaving the Server form with the browser's Back, Forward or a hash typed in
// the address bar while it holds unsaved edits (fe/briefs/01-server.md, slice
// G1, criteria 38b to 38h): the app puts the address back and asks with the
// discard dialog of 38. The whole app renders at #/instances/default against a
// fake bff (mocked fetch), except the Sign out tests, which start on the
// Overview; "1b.38b" is the brief's criterion 38b. The link case is 1b.38
// (ServerSave.test.tsx), the dialog's own behaviour 1b.38a.
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
  deferred,
  en,
  fakeBff,
  reply,
  startAt,
  stubDialog,
  heavyTest,
  type BffOptions,
} from './test/bff.ts'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/instances/default')
  dialogs = stubDialog()
})

afterEach(async () => {
  // A hash set lands as a hashchange in a task of its own: let it finish on
  // this test's page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

const PUT = '/api/instances/default/server'

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

/** The page, rendered at the hash beforeEach put. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

/**
 * The page reached by a real navigation from the Overview, so the browser has
 * an entry to go Back to: the tests that press Back start here.
 */
async function loadedAfterNavigation(options: BffOptions = {}) {
  startAt('#/')
  const bff = fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  await typeHash('#/instances/default')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

/**
 * The path that leaves go()'s pass-through token set: signed in on the
 * Overview, Sign out there (go('#/') sets the token, the hash is already '#/'
 * so no hashchange consumes it). 09 S1a.11: Sign out lands on the sign-in page
 * at '#/', so the sign-in is that page's form: it fires no hashchange, the
 * token state the Sign out left is untouched. The Overview shows, and the
 * instance page is reached by the Overview's link.
 */
async function loadedAfterSignOutOnOverview() {
  startAt('#/')
  const bff = fakeBff()
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  fireEvent.click(screen.getByRole('button', { name: en('session-sign-out') }))
  // 09 S1a.11: the sign-in page at '#/', no notice, the form to sign in again.
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  type(
    within(main).getByLabelText(en('signin-username'), { selector: 'input' }),
    'alice',
  )
  type(
    within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
    'pw',
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  fireEvent.click(
    await within(main).findByRole('link', {
      name: 'default',
    }),
  )
  await flushTasks()
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

/** The text input of a field: in its group, or labelled. */
function box(main: HTMLElement, label: string): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: label })
  return (
    g
      ? within(g).getByRole('textbox', { name: label })
      : within(main).getByRole('textbox', { name: label })
  ) as HTMLInputElement
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** A dirty form: Port changed from 8580 to 8581. */
function editPort(main: HTMLElement) {
  type(box(main, 'Port'), '8581')
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** Clicks Save and waits until the PUT was sent. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** The save bar's status line: the one live status of the Server section. */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line once the form is loaded').toHaveLength(1)
  return all[0]!
}

/** The texts of the elements the control's aria-describedby names. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** Asserts `el` is marked invalid with the error `text` named under it. */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
}

/**
 * The bar's alert reads `text` (exact): a role="alert" of its own, after the
 * status line (30a).
 */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
  expect(status(main).contains(alert), `${what}: not in the status line`).toBe(
    false,
  )
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

function pageH1(main: HTMLElement, name: string): HTMLElement {
  return within(main).getByRole('heading', { level: 1, name })
}

/** The discard dialog of 38, opened by the app with showModal(). */
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

/** Keep editing in the open dialog: click, and let the closing land. */
async function keep(d: HTMLElement) {
  fireEvent.click(keepButton(d))
  await flushTasks()
}

// ── 38b: the question ───────────────────────────────────────────────────

// Warm-up: the app's first render of the file costs many times a later one;
// this test absorbs that cost (a synchronous mount, no finder), so the real
// tests find a warm render and their finders' real-time deadlines hold.
heavyTest('warm-up: the app mounts and shows main', () => {
  fakeBff()
  render(<App />)
  expect(screen.getByRole('main')).toBeTruthy()
  cleanup()
})

test('1b.38b: a hash typed while dirty is put back, the page and its edits stay, the discard dialog opens with Keep editing focused', async () => {
  const { main } = await loaded()
  const port = box(main, 'Port')
  type(port, 'abc')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  const h1 = pageH1(main, 'default')
  await navReady()
  await typeHash('#/profiles/model')

  const d = await discardDialog()
  expect(window.location.hash, 'the address is the instance page again').toBe(
    '#/instances/default',
  )
  expect(box(main, 'Port'), 'the same form, not a remount').toBe(port)
  expect(port.value, 'the edit stays').toBe('abc')
  expectError(port, en('field-error-number', '1', '65535'), 'the field error')
  expect(status(main).textContent, 'the bar status stays as it was').toBe(
    en('save-unsaved'),
  )
  expectAlert(main, en('save-invalid'), 'the bar alert stays')
  expect(document.title, 'the title does not change').toBe(
    en('page-title', 'default'),
  )
  expect(navLink(en('nav-overview')).getAttribute('aria-current')).toBeNull()
  expect(navLink('default').getAttribute('aria-current')).toBe('page')
  expect(document.activeElement, 'focus does not move to the h1').not.toBe(h1)
  expect(document.activeElement, 'Keep editing is focused first').toBe(
    keepButton(d),
  )
})

test('1b.38b: Back while dirty is put back, the form and the bar alert stay, and the discard dialog opens', async () => {
  const { bff, main } = await loadedAfterNavigation({
    save: () => reply(409, { key: 'server-changed-elsewhere', args: [] }),
  })
  editPort(main)
  await save(main, bff)
  expectAlert(main, en('api-server-changed-elsewhere'))
  const port = box(main, 'Port')
  const h1 = pageH1(main, 'default')
  await navReady()
  await pressBack()

  const d = await discardDialog()
  expect(window.location.hash, 'the address is the instance page again').toBe(
    '#/instances/default',
  )
  expect(box(main, 'Port'), 'the same form, not a remount').toBe(port)
  expect(port.value, 'the edit stays').toBe('8581')
  expectAlert(main, en('api-server-changed-elsewhere'), 'the bar alert stays')
  expect(status(main).textContent).toBe(en('save-unsaved'))
  expect(document.title).toBe(en('page-title', 'default'))
  expect(navLink('coder').getAttribute('aria-current')).toBeNull()
  expect(navLink('default').getAttribute('aria-current')).toBe('page')
  expect(document.activeElement, 'focus does not move to the h1').not.toBe(h1)
  expect(document.activeElement).toBe(keepButton(d))
  expect(bff.to('GET', PUT), 'nothing is read again').toHaveLength(1)
})

// Sign out on the Overview calls go('#/') while the hash is already '#/': no
// hashchange fires, and that token must not pass a later change to '#/'
// through the guard (the sign-in through the #/sign-in route fires a real
// hashchange and consumes it, so it hides this).

test('1b.38b: after a Sign out on the Overview, a typed hash to that hash on the instance page asks: no token is left over', async () => {
  const { main } = await loadedAfterSignOutOnOverview()
  const port = box(main, 'Port')
  editPort(main)
  await typeHash('#/')

  const d = await discardDialog()
  expect(window.location.hash, 'the address is the instance page again').toBe(
    '#/instances/default',
  )
  expect(port.value, 'the edit is still there').toBe('8581')
  expect(document.activeElement, 'Keep editing is focused first').toBe(
    keepButton(d),
  )
})

test('1b.38b: after a Sign out on the Overview, Back to the Overview asks: the same token', async () => {
  const { main } = await loadedAfterSignOutOnOverview()
  const port = box(main, 'Port')
  editPort(main)
  await pressBack()

  await discardDialog()
  expect(window.location.hash, 'the address is the instance page again').toBe(
    '#/instances/default',
  )
  expect(port.value, 'the edit is still there').toBe('8581')
})

// ── 38c: Keep editing ───────────────────────────────────────────────────

test('1b.38c: Keep editing keeps the address and the edits, puts the focus back on the field, and sends nothing', async () => {
  const { bff, main } = await loaded()
  const port = box(main, 'Port')
  editPort(main)
  port.focus()
  await typeHash('#/profiles/model')

  const d = await discardDialog()
  await keep(d)
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
  await flushTasks()
  expect(window.location.hash, 'the address stays the instance page').toBe(
    '#/instances/default',
  )
  expect(port.value, 'the edit stays').toBe('8581')
  expect(document.activeElement, 'focus back on the field that had it').toBe(
    port,
  )
  expect(bff.to('PUT', PUT), 'nothing is sent').toHaveLength(0)
  expect(bff.to('GET', PUT), 'nothing is read again').toHaveLength(1)
})

test('1b.38c (38a): Escape in the guard dialog is Keep editing, focus back on the field', async () => {
  const { bff, main } = await loaded()
  const port = box(main, 'Port')
  editPort(main)
  port.focus()
  await typeHash('#/profiles/model')

  const d = await discardDialog()
  fireEvent.keyDown(keepButton(d), { key: 'Escape', code: 'Escape' })
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  await flushTasks()
  expect(window.location.hash).toBe('#/instances/default')
  expect(port.value).toBe('8581')
  expect(document.activeElement, 'focus back on the field').toBe(port)
  expect(bff.to('PUT', PUT), 'nothing is sent').toHaveLength(0)
})

test('1b.38c: Keep editing when the hashchange found nothing focused gives the focus to the page h1', async () => {
  const { main } = await loaded()
  editPort(main)
  // jsdom: a blur of the focused control leaves the focus on the body, as
  // when the user is typing in the browser's address bar.
  ;(document.activeElement as HTMLElement | null)?.blur()
  expect(document.activeElement, 'nothing focused').toBe(document.body)
  await typeHash('#/profiles/model')

  const d = await discardDialog()
  await keep(d)
  expect(document.activeElement, 'focus on the page h1').toBe(
    pageH1(main, 'default'),
  )
  expect(window.location.hash).toBe('#/instances/default')
})

test('1b.38c: a hashchange that arrives with document.activeElement null opens the dialog, and Keep editing gives the focus to the page h1', async () => {
  const { main } = await loaded()
  editPort(main)
  // A real browser keeps the page's element as activeElement while the user
  // types in the address bar, but a null one (the spec allows it) must not
  // throw. Stub the getter for the duration of the event, and probe that the
  // stub held when the handlers ran.
  const seen: (Element | null)[] = []
  const probe = () => seen.push(document.activeElement)
  window.addEventListener('hashchange', probe)
  Object.defineProperty(document, 'activeElement', {
    configurable: true,
    get: () => null,
  })
  try {
    await typeHash('#/profiles/model')
  } finally {
    delete (document as unknown as { activeElement?: Element | null })
      .activeElement
    window.removeEventListener('hashchange', probe)
  }
  expect(seen, 'the stub held while the event ran').toEqual([null])

  const d = await discardDialog()
  await keep(d)
  expect(document.activeElement, 'focus on the page h1').toBe(
    pageH1(main, 'default'),
  )
  expect(window.location.hash).toBe('#/instances/default')
})

// ── 38d: Discard changes ────────────────────────────────────────────────

test('1b.38d: Discard changes goes to the hash asked for, shows that page as a route change does, and asks nothing there', async () => {
  const { main } = await loaded()
  editPort(main)
  await navReady()
  await typeHash('#/instances/coder')

  const d = await discardDialog()
  fireEvent.click(confirmButton(d))
  await waitFor(() =>
    expect(window.location.hash, 'the page asked for').toBe(
      '#/instances/coder',
    ),
  )
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'coder',
  })
  expect(document.title).toBe(en('page-title', 'coder'))
  expect(navLink('coder').getAttribute('aria-current')).toBe('page')
  expect(navLink('default').getAttribute('aria-current')).toBeNull()
  expect(document.activeElement, 'focus on the new page h1').toBe(h1)
  await flushTasks()
  expect(
    screen.queryAllByRole('dialog'),
    'the hashchange of this navigation passes: no second ask',
  ).toHaveLength(0)
  // The edits were dropped, so the page left is clean: the next link asks nothing.
  fireEvent.click(navLink(en('nav-overview')))
  await flushTasks()
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
})

// ── 38e: a clean form ───────────────────────────────────────────────────

test('1b.38e: a clean form: a typed hash, Back and Forward change the page and nothing is asked', async () => {
  const { main } = await loadedAfterNavigation()
  await typeHash('#/profiles/model')
  expect(screen.queryByRole('dialog'), 'a typed hash asks nothing').toBeNull()
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('model-profiles-title'),
    }),
  ).toBeTruthy()

  await pressBack()
  expect(screen.queryByRole('dialog'), 'Back asks nothing').toBeNull()
  expect(pageH1(main, 'default')).toBeTruthy()

  await pressForward()
  expect(screen.queryByRole('dialog'), 'Forward asks nothing').toBeNull()
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('model-profiles-title'),
    }),
  ).toBeTruthy()
})

test('1b.38e: a save leaves the form clean, so Back leaves without asking', async () => {
  const { bff, main } = await loadedAfterNavigation()
  editPort(main)
  await save(main, bff)
  await within(main).findByText(en('save-saved'))
  // The clean flag reaches App's guard in a task after the reply's commit:
  // let it land before Back, as a user's click always does.
  await flushTasks()
  await pressBack()
  expect(screen.queryByRole('dialog'), 'after a save').toBeNull()
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
})

test('1b.38e (30): the bar Discard leaves the form clean, so Back leaves without asking', async () => {
  const { main } = await loadedAfterNavigation()
  editPort(main)
  fireEvent.click(discardButton(main))
  expect(status(main).textContent, 'clean').toBe('')
  await pressBack()
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
})

test('1b.38e (33b): an edit made while the PUT runs leaves the form dirty, and a typed hash asks', async () => {
  const { promise, resolve } = deferred()
  const { bff, main } = await loaded({ save: () => promise })
  editPort(main)
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
  type(box(main, 'Port'), '8582')
  resolve(reply(200, { revision: 'r-2', running: false }))
  await within(main).findByText(en('save-unsaved'))

  await typeHash('#/profiles/model')
  const d = await discardDialog()
  expect(window.location.hash).toBe('#/instances/default')
  expect(box(main, 'Port').value).toBe('8582')
  await keep(d)
  expect(box(main, 'Port').value).toBe('8582')
})

// ── 38f: while the session has ended ────────────────────────────────────

test('1b.38f (36a): a typed hash while the session has ended asks the same, and Keep keeps the edits and the alert', async () => {
  const { bff, main } = await loaded({
    save: () => reply(401, { key: 'session-required', args: [] }),
  })
  const port = box(main, 'Port')
  editPort(main)
  await save(main, bff)
  await within(main).findByText(en('session-ended'))
  port.focus()

  await typeHash('#/profiles/model')
  const d = await discardDialog()
  expect(window.location.hash).toBe('#/instances/default')
  expect(port.value).toBe('8581')
  await keep(d)
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(window.location.hash).toBe('#/instances/default')
  expect(port.value, 'the edits stay').toBe('8581')
  expectAlert(main, en('session-ended'), 'the session-ended alert stays')
  expect(document.activeElement, 'focus back on the field').toBe(port)
})

// ── 38g: a second change while the dialog is open ───────────────────────

test('1b.38g: a second hash typed while the guard dialog is open keeps one dialog, and Discard goes to the latest hash', async () => {
  const { main } = await loaded()
  editPort(main)
  await typeHash('#/profiles/model')
  const d = await discardDialog()
  const keepBtn = keepButton(d)
  expect(document.activeElement).toBe(keepBtn)

  await typeHash('#/instances/coder')
  expect(
    screen.getAllByRole('dialog'),
    'the one dialog stays open, not a second one',
  ).toHaveLength(1)
  expect(window.location.hash, 'the address is put back again').toBe(
    '#/instances/default',
  )
  expect(document.activeElement, 'focus stays in the dialog').toBe(keepBtn)
  fireEvent.click(confirmButton(d))
  await waitFor(() =>
    expect(window.location.hash, 'Discard goes to the latest hash').toBe(
      '#/instances/coder',
    ),
  )
  expect(
    await within(main).findByRole('heading', { level: 1, name: 'coder' }),
  ).toBeTruthy()
})

test('1b.38g: the dialog opened by a link takes the latest hash asked for', async () => {
  const { main } = await loaded()
  const link = navLink(en('nav-overview'))
  editPort(main)
  link.focus()
  fireEvent.click(link)
  const d = screen.getByRole('dialog', { name: en('discard-title') })
  expect(dialogs.opened).toContain(d)

  await typeHash('#/instances/coder')
  expect(screen.getAllByRole('dialog'), 'the one dialog').toHaveLength(1)
  expect(window.location.hash, 'the address is put back').toBe(
    '#/instances/default',
  )
  expect(pageH1(main, 'default'), 'the page stays').toBeTruthy()
  fireEvent.click(confirmButton(d))
  await waitFor(() =>
    expect(window.location.hash, 'Discard goes to the latest hash').toBe(
      '#/instances/coder',
    ),
  )
  expect(
    await within(main).findByRole('heading', { level: 1, name: 'coder' }),
  ).toBeTruthy()
})

test('1b.38g: Keep editing after a second typed hash returns the focus to the field that had it when the first hashchange came', async () => {
  const { main } = await loaded()
  const port = box(main, 'Port')
  editPort(main)
  port.focus()
  await typeHash('#/profiles/model')
  const d = await discardDialog()
  await typeHash('#/instances/coder')
  await keep(d)
  expect(
    document.activeElement,
    'the opener of the first hashchange is kept, not the dialog button focused since',
  ).toBe(port)
})

// ── 38h: what is not guarded ────────────────────────────────────────────

test('1b.38h: a hash equal to the one shown fires no hashchange, asks nothing and keeps the edits', async () => {
  const { main } = await loaded()
  editPort(main)
  await typeHash('#/instances/default')
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(window.location.hash).toBe('#/instances/default')
  expect(box(main, 'Port').value).toBe('8581')
  expect(status(main).textContent).toBe(en('save-unsaved'))
})
