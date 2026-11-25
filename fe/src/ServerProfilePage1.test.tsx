// Brief 16, slice 17a (fe/briefs/16-server-profiles-and-instances.md): the
// server profile's own page at #/profiles/server/<id>, where the Server form
// moved from the instance page. Its back link, the Used by row, the read
// (GET /api/profiles/server/<id>, the document being today's server document
// plus `instance`), the save (PUT there, with today's body), the folders of a
// profile no instance runs, and leaving with edits. The list page and the nav
// are ServerProfiles1.test.tsx; the skeletons U17aSkeletons.test.tsx; every
// other reply the save answers is the migrated ServerSave suite (it is the
// save bar's rule, not this page's). Test names cite the brief first,
// `16 17a.5`, because brief 17's UX pass owns the bare `17a.n` names.
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
  en,
  fakeBff,
  reply,
  serverDoc,
  serverProfileDoc,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'
import { instanceHref } from './route.ts'

const HASH = '#/profiles/server/default'
const PUT = '/api/profiles/server/default'
/** The route the instance page used for the same file (17a.10: it is gone). */
const OLD = '/api/instances/default/server'

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

/** The page, with its Server form loaded. */
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

/** The page, at a hash of the caller's. */
async function openAt(hash: string, options: BffOptions = {}) {
  startAt(hash)
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

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

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

/** The save bar's status line (the one status after the section's h2). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
  const all = within(main)
    .getAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line after the h2').toHaveLength(1)
  return all[0]!
}

/** The text of every `role="alert"` on the page. */
function alerts(main: HTMLElement): string[] {
  return within(main)
    .queryAllByRole('alert')
    .map((a) => a.textContent ?? '')
}

/** The discard dialog of `fe/DESIGN.md` 2.3, once it is open. */
async function discardDialog(): Promise<HTMLElement> {
  const d = await screen.findByRole('dialog', { name: en('discard-title') })
  expect(d.tagName, 'a native <dialog>').toBe('DIALOG')
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  return d
}

function navLink(name: string): HTMLElement {
  return within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).getByRole('link', { name })
}

/** Types `hash` in the address bar: jsdom fires the hashchange in a task. */
async function typeHash(hash: string) {
  await act(async () => {
    window.location.hash = hash
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** Asks the browser whether leaving the page would be held. */
function asksBrowser(): boolean {
  const e = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(e)
  return e.defaultPrevented
}

// ── 17a.5: the page ─────────────────────────────────────────────────────

test('16 17a.5: a link server-profile-back to #/profiles/server sits before the h1 of the id, the h1 takes the focus, and the document title is the page', async () => {
  const { main } = await loaded()
  const h1 = within(main).getByRole('heading', { level: 1, name: 'default' })
  const back = within(main).getByRole('link', {
    name: en('server-profile-back'),
  })
  expect(back.getAttribute('href'), 'to the list').toBe('#/profiles/server')
  expect(before(back, h1), 'the link is before the h1').toBe(true)
  expect(document.activeElement, 'the h1 takes the focus').toBe(h1)
  expect(h1.getAttribute('tabindex'), 'so it is focusable').toBe('-1')
  expect(document.title).toBe(en('page-title', 'default'))
})

test('16 17a.5 (as 22 D4.9 amends it): after the h1, with the status row of 22 D2.8 gone, a dl.assets row server-profiles-column-used-by holds the document’s instance as a link to its page, before the h2', async () => {
  const { main } = await loaded()
  expect(
    main.querySelector('div.page-status'),
    '22 D4.9: the status row is gone',
  ).toBeNull()
  const dl = main.querySelector('dl.assets')
  expect(dl, 'the Value list').not.toBeNull()
  const dt = dl!.querySelector('dt')
  const dd = dl!.querySelector('dd')
  expect(dt?.textContent, 'the row’s name').toBe(
    en('server-profiles-column-used-by'),
  )
  const link = within(dd as HTMLElement).getByRole('link', {
    name: 'default',
  })
  expect(link.getAttribute('href'), 'to the instance page').toBe(
    instanceHref('default'),
  )
  const h1 = within(main).getByRole('heading', { level: 1, name: 'default' })
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
  expect(before(h1, dl!), 'the row follows the h1').toBe(true)
  expect(before(dl!, h2), 'and precedes the Server section').toBe(true)
})

test('16 17a.5: a profile no instance runs shows server-profiles-unused in a muted row, with no link', async () => {
  const { main } = await loaded({
    serverProfile: () => reply(200, serverProfileDoc({}, { instance: null })),
  })
  const dl = main.querySelector('dl.assets')
  expect(dl, 'the list is there, with its one row').not.toBeNull()
  const dd = dl!.querySelector('dd')
  expect(dd?.textContent).toBe(en('server-profiles-unused'))
  expect(dd!.className, 'muted').toContain('muted')
  expect(
    dd!.querySelector('a'),
    'no link to an instance that is not there',
  ).toBeNull()
  expect(dl!.querySelectorAll('dt').length, 'one row').toBe(1)
})

test('16 17a.5: the Server section is its h2, the paragraph server-profile-intro naming server@<id>.ini, and today’s form with its cards in order', async () => {
  const { main } = await loaded()
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
  const intro = within(main).getByText(
    en('server-profile-intro', 'server@default.ini'),
  )
  expect(intro.tagName, 'a paragraph').toBe('P')
  expect(before(h2, intro), 'the intro follows the h2').toBe(true)
  const cards = within(main)
    .getAllByRole('heading', { level: 3 })
    .map((h) => h.textContent)
  expect(cards).toEqual([
    en('server-network'),
    en('server-resources'),
    en('server-advanced'),
    en('server-snapshots'),
    en('server-sdk'),
  ])
})

// ── 17a.6: the read ─────────────────────────────────────────────────────

test('16 17a.6: the page sends one GET /api/profiles/server/<id> per arrival, with the id percent-encoded, and nothing to /api/instances/<id>/server', async () => {
  const { bff, main } = await openAt('#/profiles/server/edge%20box', {
    serverProfile: () => reply(200, serverProfileDoc({}, { instance: null })),
  })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  // The route the page must read for this id: the id percent-encoded, as
  // sent (the file-level GET is the default profile's, for the other tests).
  expect(
    bff.to('GET', '/api/profiles/server/edge%20box').length,
    'one read',
  ).toBe(1)
  expect(
    bff.to('GET', '/api/profiles/server/edge%20box')[0]?.path,
    'the encoded id, as sent',
  ).toContain('edge%20box')
  expect(
    within(main).getByRole('heading', { level: 1, name: 'edge box' }),
    'the h1 shows the decoded id',
  ).toBeTruthy()
  expect(bff.to('GET', OLD), '17a.10: the instance route is gone').toHaveLength(
    0,
  )
})

test('16 17a.6: a 404 client-no-server-profile shows server-profile-not-found with the id, the h1 and the back link, and no dl and no form', async () => {
  const { main } = await openAt('#/profiles/server/ghost', {
    serverProfile: () =>
      reply(404, { key: 'client-no-server-profile', args: ['ghost'] }),
  })
  expect(
    await within(main).findByText(en('server-profile-not-found', 'ghost')),
  ).toBeTruthy()
  const h1 = within(main).getByRole('heading', { level: 1, name: 'ghost' })
  const back = within(main).getByRole('link', {
    name: en('server-profile-back'),
  })
  expect(back.getAttribute('href')).toBe('#/profiles/server')
  expect(main.querySelector('dl.assets'), 'no Used by row').toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 3,
      name: en('server-network'),
    }),
    'no form',
  ).toBeNull()
  expect(
    within(main).queryByRole('heading', { level: 2, name: en('server-title') }),
    'no Server section',
  ).toBeNull()
  expect(
    before(h1, within(main).getByText(en('server-profile-not-found', 'ghost'))),
    'the message follows the h1',
  ).toBe(true)
})

// ── 17a.7: the save ─────────────────────────────────────────────────────

test('16 17a.7: Save sends PUT /api/profiles/server/<id> with today’s body, and nothing to the instance route', async () => {
  const { bff, main } = await loaded()
  type(box(main, 'Port'), '8581')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-saved'))
  const saves = bff.to('PUT', PUT)
  expect(saves, 'one save, to the profile').toHaveLength(1)
  expect(bff.to('PUT', OLD), 'the instance route is gone').toHaveLength(0)
  const body = saves[0]!.body as {
    revision: string
    values: Record<string, unknown>
  }
  expect(Object.keys(body).sort(), 'only revision and values').toEqual([
    'revision',
    'values',
  ])
  expect(body.revision, 'the revision that was read').toBe(serverDoc().revision)
  expect(body.values.Port).toBe(8581)
  expect(status(main).textContent).toBe(en('save-saved'))
})

test('16 17a.7: a 422 whose field names no control of the form shows save-refused with the bff’s text and marks no field', async () => {
  const { bff, main } = await loaded({
    serverProfileSave: () =>
      reply(422, {
        key: 'model-profile-over-memory',
        field: 'qwen-27b',
        args: ['qwen-27b', 'default', '30721', '24560'],
      }),
  })
  type(box(main, 'Port'), '8581')
  fireEvent.click(saveButton(main))
  const text = en(
    'api-model-profile-over-memory',
    'qwen-27b',
    'default',
    '30721',
    '24560',
  )
  await within(main).findByText(en('save-refused', text))
  expect(alerts(main), 'the bar’s alert, with the text as sent').toEqual([
    en('save-refused', text),
  ])
  expect(status(main).textContent, 'the form is still dirty').toBe(
    en('save-unsaved'),
  )
  expect(
    within(main)
      .queryAllByRole('textbox')
      .filter((el) => el.getAttribute('aria-invalid') === 'true'),
    'no field is marked: the name is a model profile',
  ).toEqual([])
  expect(bff.to('PUT', PUT), 'one save, nothing resent').toHaveLength(1)
})

// ── 17a.8: the folders of a profile no instance runs ────────────────────

test('16 17a.8: a null defaults.ModelsDir or defaults.StateDir shows server-folder-unused-default as the placeholder; a string default keeps server-folder-instance-default', async () => {
  const defaults = {
    ...serverDoc().defaults,
    ModelsDir: null,
    StateDir: null,
  }
  const { main } = await loaded({
    serverProfile: () => reply(200, serverProfileDoc({}, { defaults })),
  })
  expect(box(main, 'Models folder').placeholder).toBe(
    en('server-folder-unused-default'),
  )
  expect(box(main, 'Snapshot folder').placeholder).toBe(
    en('server-folder-unused-default'),
  )
  cleanup()
  vi.unstubAllGlobals()
  // The other half: a document that carries the defaults says them, as before.
  const other = await loaded()
  const d = serverDoc().defaults
  expect(box(other.main, 'Models folder').placeholder).toBe(
    en('server-folder-instance-default', d.ModelsDir),
  )
  expect(box(other.main, 'Snapshot folder').placeholder).toBe(
    en('server-folder-instance-default', d.StateDir),
  )
})

// ── 17a.9: leaving ──────────────────────────────────────────────────────

test('16 17a.9: with unsaved edits, a nav link, Back and a typed hash each ask with the discard dialog first, and the page stays', async () => {
  const { main } = await loaded()
  type(box(main, 'Port'), '8581')
  expect(asksBrowser(), 'dirty: the browser is asked too').toBe(true)
  // A nav link.
  fireEvent.click(navLink(en('nav-overview')))
  let d = await discardDialog()
  fireEvent.click(within(d).getByRole('button', { name: en('discard-keep') }))
  await flushTasks()
  expect(window.location.hash, 'kept on the profile').toBe(HASH)
  expect(box(main, 'Port').value, 'the edit is kept').toBe('8581')
  // The page's own Back link.
  fireEvent.click(
    within(main).getByRole('link', { name: en('server-profile-back') }),
  )
  d = await discardDialog()
  fireEvent.click(within(d).getByRole('button', { name: en('discard-keep') }))
  await flushTasks()
  expect(window.location.hash).toBe(HASH)
  // A hash typed in the address bar.
  await typeHash('#/profiles/model')
  d = await discardDialog()
  fireEvent.click(within(d).getByRole('button', { name: en('discard-keep') }))
  await flushTasks()
  expect(window.location.hash, 'put back').toBe(HASH)
  expect(box(main, 'Port').value).toBe('8581')
})

test('16 17a.9: a clean page asks nothing: Back leaves at once, the browser is not asked, and no dialog opens', async () => {
  const { main } = await loaded()
  expect(asksBrowser(), 'a clean form does not hold the browser').toBe(false)
  fireEvent.click(
    within(main).getByRole('link', { name: en('server-profile-back') }),
  )
  await flushTasks()
  expect(window.location.hash, 'left').toBe('#/profiles/server')
  expect(
    screen.queryByRole('dialog', { name: en('discard-title') }),
    'no question',
  ).toBeNull()
})

// ── 17a.11: wrapping, the half jsdom can see ────────────────────────────

test('16 17a.11: the stylesheet declares every class the page renders, so nothing arrives unstyled', async () => {
  const { main } = await loaded()
  const css = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
  const classes = new Set<string>()
  for (const el of main.querySelectorAll<HTMLElement>('[class]'))
    for (const c of el.classList) classes.add(c)
  const missing = [...classes].filter(
    (c) => !css.includes(`.${c}`) && !css.includes(`.${c}:`),
  )
  expect(missing, 'classes with no rule in index.css').toEqual([])
})
