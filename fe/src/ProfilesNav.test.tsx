// The nav's Profiles group and the `#/profiles/model` routes, slice 05b
// (fe/briefs/05b-profiles-nav.md, criteria 5b.1 to 5b.11 and 5b.13): a Profiles
// group in the
// nav with one entry, the list at #/profiles/model and a model profile's page at
// #/profiles/model/<id>, and the old #/models links rewritten in place by
// history.replaceState. Navigation and names only: what the list and the page do
// is pinned in ModelProfiles.test.tsx, ModelProfilesFilter.test.tsx,
// ModelProfilesSwitch.test.tsx and the ModelProfile* files, which this brief
// retargets to the new addresses. "5b.3" is the brief's criterion 3 of 05b. The
// narrow menu's look (5b.4's 320 px reflow, contrast, focus ring) and axe on the
// labelled list are the UX check's, not jsdom's.
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
import strings from './strings/en_US.json'
import {
  EN,
  before,
  deferred,
  en,
  fakeBff,
  navigate,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const LIST_HASH = '#/profiles/model'
const LIST_GET = '/api/profiles/model'
const MODE = en('model-profile-embedding-mode')
const PROFILES = en('nav-profiles')
const ENTRY = en('nav-model-profiles')
/** Slice 13 (13 N.3): the last entry of the nav. */
const ABOUT = en('nav-about')
/** Slice 18 (18 AN.6): Node info, the entry before About. */
const NODE_INFO = en('nav-node')
/** The strings file itself (5b.11 pins its keys and texts, exactly). */
const S = strings as Record<string, string>

let dialogs: ReturnType<typeof stubDialog> | null = null

beforeEach(() => startAt(LIST_HASH))

afterEach(async () => {
  // A hash set lands as a hashchange in a task of its own: let it finish on
  // this test's page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  // The history spies of 5b.8 are the window's: put the real methods back.
  vi.restoreAllMocks()
  dialogs?.restore()
  dialogs = null
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

function modelProfile(
  id: string,
  enabled = true,
  engine = 'llama.cpp',
  engineAvailable = true,
  model = '',
): ModelProfile {
  return { id, engine, engineAvailable, enabled, model }
}

/** The app at the address the location holds; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

function nav(): HTMLElement {
  return screen.getByRole('navigation', { name: en('nav-label') })
}

/** The nav, once the session answered: 09 S1a.3, it arrives with the session. */
async function navShown(): Promise<HTMLElement> {
  return screen.findByRole('navigation', { name: en('nav-label') })
}

/** The names of the nav's links, in order. */
function navLinks(): string[] {
  return within(nav())
    .queryAllByRole('link')
    .map((a) => a.textContent ?? '')
}

/** The nav with its instance links in (they come with the instances read). */
async function navReady(): Promise<HTMLElement> {
  const n = nav()
  await within(n).findByRole('link', { name: 'coder' })
  return n
}

/** The Model profiles link in the nav (the nav comes with the session). */
async function entry(): Promise<HTMLElement> {
  return within(await navShown()).findByRole('link', { name: ENTRY })
}

/** The nav's Profiles list, found by the name of its label. */
async function profilesList(): Promise<HTMLElement> {
  return within(await navShown()).findByRole('list', { name: PROFILES })
}

function profilesLabel(): HTMLElement | null {
  return document.getElementById('nav-profiles')
}

/** The aria-current of the Model profiles link: null when it has none. */
function currentOf(link: HTMLElement): string | null {
  return link.getAttribute('aria-current')
}

async function heading1(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
}

async function table(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('table', { name: en('model-profiles-title') })
}

/** The list's filter field. */
function filterField(main: HTMLElement): HTMLInputElement {
  return within(main).getByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  }) as HTMLInputElement
}

async function switchOf(
  main: HTMLElement,
  id: string,
): Promise<HTMLInputElement> {
  await table(main)
  return within(main).getByRole('switch', { name: id }) as HTMLInputElement
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

/** Every read of a model profile: the list's, and a page's. */
function profileReads(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter((c) => c.method === 'GET' && c.path.startsWith(LIST_GET))
    .map((c) => c.path)
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

/**
 * Makes the model profile form dirty: Embedding mode picked to the option it
 * does NOT show (the default fixture's read has no `embeddings` key, so a fixed
 * value would be no change: 2e.93).
 */
function makeDirty(main: HTMLElement) {
  const select = selectOf(main, MODE)
  const value = select.value === 'true' ? 'false' : 'true'
  fireEvent.change(select, { target: { value } })
}

/** The loaded model profile page of `id`, the settings card in. */
async function loadedPage(id = ID, options: BffOptions = {}) {
  const { bff, main } = open(options)
  await within(main).findByRole('heading', { level: 1, name: id })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The not-found page of 5b.7 (its h1, `not-found`). */
async function notFound(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('heading', { level: 1, name: en('not-found') })
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

/** The discard dialog of 01's 38, opened by the app with showModal(). */
async function discardDialog(): Promise<HTMLElement> {
  const d = await screen.findByRole('dialog', { name: en('discard-title') })
  expect(d.tagName, 'a native <dialog>').toBe('DIALOG')
  expect(dialogs?.opened, 'opened with showModal()').toContain(d)
  return d
}

function keepButton(d: HTMLElement): HTMLElement {
  return within(d).getByRole('button', { name: en('discard-keep') })
}

// ── 5b.1, 5b.2: the nav's Profiles group ────────────────────────────────

test('5b.1: signed in, the nav holds a Profiles group: a p#nav-profiles.nav-label and a ul labelled by it, with one Model profiles link to #/profiles/model, after Overview and the Instances group', async () => {
  const { main } = open()
  await table(main)
  const n = await navReady()
  const label = profilesLabel()
  expect(label, 'the group has a label element with the id').not.toBeNull()
  expect(label!.tagName, 'a paragraph, as the Instances group').toBe('P')
  expect(label!.textContent).toBe(PROFILES)
  expect(label!.classList.contains('nav-label'), 'the nav-label class').toBe(
    true,
  )
  expect(label!.querySelectorAll('a').length, 'the label is not a link').toBe(0)
  const list = await profilesList()
  expect(list.tagName).toBe('UL')
  expect(list.getAttribute('aria-labelledby')).toBe('nav-profiles')
  const links = within(list).getAllByRole('link')
  expect(links, 'one entry').toHaveLength(1)
  expect(links[0]!.textContent).toBe(ENTRY)
  expect(links[0]!.getAttribute('href')).toBe(LIST_HASH)
  // DOM order: the Overview list, the Instances list, the label, the list.
  const lists = within(n).getAllByRole('list')
  const instances = within(n).getByRole('list', { name: en('nav-instances') })
  expect(lists[0], 'Overview first').not.toBe(instances)
  expect(before(lists[0]!, instances), 'Overview, then Instances').toBe(true)
  expect(before(instances, label!), 'the Instances group comes first').toBe(
    true,
  )
  expect(before(label!, list), 'the label right above its list').toBe(true)
  expect(n.contains(label), 'the group is inside the nav').toBe(true)
  // The screen reader's nav reads Overview, each instance, Model profiles.
  expect(navLinks()).toEqual([
    en('nav-overview'),
    'default',
    'coder',
    ENTRY,
    NODE_INFO,
    ABOUT,
  ])
  expect(
    within(main).queryByText(PROFILES),
    'the label is nav chrome',
  ).toBeNull()
})

test('5b.1: the Profiles group shows whatever the instances read gives: loading, empty or failed, and sits right after Overview when there is no Instances group', async () => {
  // Loading: the group is there while the instances read is pending.
  const pending = deferred()
  open({ instances: () => pending.promise })
  expect(await profilesList(), 'loading').toBeTruthy()
  await act(async () => {
    pending.resolve(reply(200, []))
  })
  cleanup()
  vi.unstubAllGlobals()
  // Empty: no Instances group, the Profiles group right after Overview.
  open({ instances: [] })
  expect(await profilesList(), 'empty').toBeTruthy()
  expect(
    within(nav()).queryByRole('list', { name: en('nav-instances') }),
    'no Instances group',
  ).toBeNull()
  expect(before(within(nav()).getAllByRole('list')[0]!, profilesLabel()!)).toBe(
    true,
  )
  expect(navLinks()).toEqual([en('nav-overview'), ENTRY, NODE_INFO, ABOUT])
  cleanup()
  vi.unstubAllGlobals()
  // Failed: the same, read on the Overview: the failure text belongs to the
  // instance list, so it is only on that page (as the sibling
  // ModelProfiles.test.tsx does). The nav's group is the point here.
  startAt('#/')
  open({
    instances: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  await screen.findByText(en('instances-manager-unavailable'))
  const failed = await profilesList()
  expect(failed, 'the group is there after a failed read').toBeTruthy()
  expect(failed.getAttribute('aria-labelledby'), 'labelled').toBe(
    'nav-profiles',
  )
  const failedLinks = within(failed).getAllByRole('link')
  expect(failedLinks, 'one entry').toHaveLength(1)
  expect(failedLinks[0]!.textContent).toBe(ENTRY)
  expect(failedLinks[0]!.getAttribute('href')).toBe(LIST_HASH)
  expect(
    within(nav()).queryByRole('list', { name: en('nav-instances') }),
    'no Instances group',
  ).toBeNull()
  expect(
    before(within(nav()).getAllByRole('list')[0]!, profilesLabel()!),
    'the group comes right after Overview',
  ).toBe(true)
  expect(navLinks()).toEqual([en('nav-overview'), ENTRY, NODE_INFO, ABOUT])
})

test('5b.1: signed out there is no nav at all: no Profiles label, no Profiles list, no Model profiles link (09 S1a.4)', async () => {
  const { main } = open({ session: 'signed-out' })
  // 09 S1a.1: signed out, the sign-in page is the only content.
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  expect(
    screen.queryByRole('navigation', { name: en('nav-label') }),
    '09 S1a.4: no nav signed out',
  ).toBeNull()
  expect(profilesLabel(), 'no label element').toBeNull()
  expect(
    document.querySelectorAll('[aria-labelledby="nav-profiles"]'),
    'no Profiles list',
  ).toHaveLength(0)
  expect(
    screen.queryAllByRole('link', { name: ENTRY }),
    'no Model profiles link',
  ).toHaveLength(0)
})

test('5b.2: nothing says Server profiles, nothing links under #/profiles/server, and no nav-server-profiles key is added (05c adds it)', async () => {
  const { main } = open()
  await table(main)
  const hrefs = [
    ...document.querySelectorAll<HTMLAnchorElement>('a[href]'),
  ].map((a) => a.getAttribute('href') ?? '')
  expect(hrefs.length, 'the app has links').toBeGreaterThan(0)
  for (const href of hrefs) {
    expect(href, `link to ${href}`).not.toMatch(/^#\/profiles\/server/)
  }
  expect(document.body.textContent).not.toContain('Server profiles')
  expect(
    S['nav-server-profiles'],
    'the strings file gains no such key',
  ).toBeUndefined()
  expect(EN['nav-server-profiles'], 'the test mirror neither').toBeUndefined()
})

// ── 5b.3: the current page ──────────────────────────────────────────────

test('5b.3: the Model profiles link has aria-current="page" on #/profiles/model and "true" on a model profile page, found or missing', async () => {
  open()
  const onList = await entry()
  await waitFor(() => expect(currentOf(onList)).toBe('page'))
  cleanup()
  vi.unstubAllGlobals()
  startAt(HASH)
  open()
  const onPage = await entry()
  await waitFor(() => expect(currentOf(onPage)).toBe('true'))
  cleanup()
  vi.unstubAllGlobals()
  // A model profile that is not there: the page it shows is still this route.
  startAt('#/profiles/model/Gone')
  open({
    modelProfile: () =>
      reply(404, { key: 'no-model-profile-named', args: ['Gone'] }),
  })
  const onMissing = await entry()
  await waitFor(() => expect(currentOf(onMissing)).toBe('true'))
  // The sentence arrives with the mocked 404, while the aria-current gate is
  // met on the route's first render: wait for the answer, not for this tick.
  await waitFor(() =>
    expect(
      document.body.textContent,
      'the missing one shows the id sentence',
    ).toContain(en('model-profile-not-found', 'Gone')),
  )
})

test('5b.3: no aria-current on the Overview, an instance, the not-found page or #/profiles/model/, and never on the Profiles label', async () => {
  startAt('#/')
  open()
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  expect(currentOf(await entry())).toBeNull()
  cleanup()
  vi.unstubAllGlobals()
  startAt('#/instances/default')
  open()
  expect(currentOf(await entry())).toBeNull()
  cleanup()
  vi.unstubAllGlobals()
  // The not-found page under the group's own prefix.
  startAt('#/profiles/model/')
  open()
  const main = screen.getByRole('main')
  await notFound(main)
  expect(currentOf(await entry())).toBeNull()
  expect(currentOf(profilesLabel()!), 'the label never carries it').toBeNull()
})

// ── 5b.4: the narrow menu ───────────────────────────────────────────────

test('5b.4: with the menu open, activating Model profiles closes the menu and moves the focus to the h1 Model profiles', async () => {
  startAt('#/')
  const { main } = open()
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  const menu = screen.getByRole('button', { name: en('nav-menu') })
  expect(menu.getAttribute('aria-expanded')).toBe('false')
  fireEvent.click(menu)
  expect(menu.getAttribute('aria-expanded')).toBe('true')
  fireEvent.click(await entry())
  expect(
    menu.getAttribute('aria-expanded'),
    'the menu closes as the entry is followed',
  ).toBe('false')
  const h1 = await heading1(main)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(h1.getAttribute('tabindex')).toBe('-1')
})

// ── 5b.5: the list page ─────────────────────────────────────────────────

test('5b.5: at #/profiles/model the h1 is Model profiles, so is the document title, there is no h2 and no #model-profiles-title, and the table is named by the h1', async () => {
  const { main } = open()
  const h1 = await heading1(main)
  expect(h1.id).toBe('page-heading')
  expect(h1.getAttribute('tabindex')).toBe('-1')
  expect(document.title).toBe(en('page-title', en('model-profiles-title')))
  expect(
    within(main).queryAllByRole('heading', { level: 2 }),
    'the list page has no h2',
  ).toHaveLength(0)
  expect(
    document.getElementById('model-profiles-title'),
    'the h2 and its id are gone',
  ).toBeNull()
  const t = await table(main)
  expect(t.getAttribute('aria-labelledby')).toBe('page-heading')
  // Under the h1, in today's order: the intro, the filter, the table, the note.
  const intro = within(main).getByText(en('model-profiles-intro'))
  const note = within(main).getByText(en('model-profiles-off-note'))
  const filter = filterField(main)
  expect(before(h1, intro), 'intro after the h1').toBe(true)
  expect(before(intro, filter), 'filter after the intro').toBe(true)
  expect(before(filter, t), 'table after the filter').toBe(true)
  expect(before(t, note), 'note after the table').toBe(true)
})

test('5b.5: signed out at #/profiles/model the sign-in form shows with signin-required, the address is kept and nothing is read; signing in opens the list with the focus on its h1 and one read', async () => {
  const { bff, main } = open({ session: 'signed-out' })
  const username = await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })
  expect(within(main).getByText(en('signin-required'))).toBeTruthy()
  expect(window.location.hash).toBe(LIST_HASH)
  expect(within(main).queryByRole('table')).toBeNull()
  expect(profileReads(bff), 'no read while signed out').toEqual([])
  fireEvent.change(username, { target: { value: 'alice' } })
  fireEvent.change(
    within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  const h1 = await heading1(main)
  await table(main)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(window.location.hash).toBe(LIST_HASH)
  expect(profileReads(bff)).toEqual([LIST_GET])
})

// ── 5b.6: a model profile page at #/profiles/model/<id> ─────────────────

test('5b.6: each row links to #/profiles/model/<encoded id>, the page links back to #/profiles/model, and an id with a space and a percent sign round-trips through the href, the h1 and the read', async () => {
  const { main } = open()
  const t = await table(main)
  const links = within(t)
    .getAllByRole('link')
    .map((a) => [a.textContent ?? '', a.getAttribute('href') ?? ''] as const)
  expect(links).toEqual(
    ['Qwen3.6-35B-A3B-UD-Q4_K_XL', 'gemma-3-12b-it'].map((id) => [
      id,
      `#/profiles/model/${encodeURIComponent(id)}`,
    ]),
  )
  cleanup()
  vi.unstubAllGlobals()
  // The page's way back.
  startAt(HASH)
  const second = open()
  // The page is behind the session read: an async finder, so the sign-in form
  // it shows first cannot be what the test reads.
  const back = await within(second.main).findByRole('link', {
    name: en('model-profile-back'),
  })
  expect(back.getAttribute('href')).toBe(LIST_HASH)
  expect(
    back.closest('.model-profile-back'),
    'it sits in the model-profile-back paragraph',
  ).not.toBeNull()
  cleanup()
  vi.unstubAllGlobals()
  // An id that needs encoding: the address keeps it encoded, the h1 shows it
  // decoded, the read carries the bytes of the address.
  startAt('#/profiles/model/Qwen%203%25')
  const third = open()
  const h1 = await within(third.main).findByRole('heading', {
    level: 1,
    name: 'Qwen 3%',
  })
  expect(h1).toBeTruthy()
  expect(window.location.hash).toBe('#/profiles/model/Qwen%203%25')
  await waitFor(() =>
    expect(profileReads(third.bff), '#/profiles/model/Qwen%203%25').toEqual([
      '/api/profiles/model/Qwen%203%25',
    ]),
  )
})

// ── 5b.7: addresses under #/profiles that are not pages ─────────────────

const DEAD: [string, string][] = [
  ['#/profiles', 'the group alone'],
  ['#/profiles/', 'the group with an empty segment'],
  ['#/profiles/server', 'the server profiles of 05c'],
  ['#/profiles/server/default', 'one of them'],
  ['#/profiles/other', 'any other group'],
  ['#/profiles/model/', 'an empty id'],
  ['#/profiles/model/a/b', 'two segments'],
  ['#/profiles/model/%E0%A4%A', 'a segment that does not decode'],
]

test.each(DEAD)(
  '5b.7: %s (%s) is the not-found page, signed in, with no read and no aria-current',
  async (hash) => {
    startAt(hash)
    const { bff, main } = open()
    await notFound(main)
    expect(window.location.hash, hash).toBe(hash)
    expect(profileReads(bff), `${hash}: nothing read`).toEqual([])
    expect(
      currentOf(await entry()),
      `${hash}: the entry is not current`,
    ).toBeNull()
  },
)

test.each(DEAD.slice(0, 3))(
  '5b.7: %s (%s) is the sign-in page signed out too (09 S1a.1, S1a.7), with no read',
  async (hash) => {
    startAt(hash)
    const { bff, main } = open({ session: 'signed-out' })
    // 09 S1a.1: signed out, no hash is the not-found page: the sign-in page is
    // the only content. 09 S1a.7: a hash that is a page shows the notice.
    await within(main).findByRole('heading', {
      level: 1,
      name: en('signin-title'),
    })
    expect(within(main).getByText(en('signin-required'))).toBeTruthy()
    expect(
      within(main).queryByRole('heading', {
        level: 1,
        name: en('not-found'),
      }),
      `${hash}: not the not-found page`,
    ).toBeNull()
    expect(window.location.hash, hash).toBe(hash)
    expect(profileReads(bff), `${hash}: nothing read`).toEqual([])
  },
)

// ── 5b.8: the old #/models links ────────────────────────────────────────

test('5b.8: starting at #/models rewrites the address to #/profiles/model with no history entry, shows the list, and leaves the focus where it is', async () => {
  startAt('#/models')
  const entries = window.history.length
  const { bff, main } = open()
  expect(window.location.hash, 'the address is rewritten').toBe(LIST_HASH)
  expect(window.history.length, 'replaceState adds no history entry').toBe(
    entries,
  )
  const t = await table(main)
  expect(t).toBeTruthy()
  expect(document.title).toBe(en('page-title', en('model-profiles-title')))
  expect(document.activeElement, 'the first load does not move the focus').toBe(
    document.body,
  )
  expect(profileReads(bff)).toEqual([LIST_GET])
  expect(currentOf(await entry()), 'the rewritten route is current').toBe(
    'page',
  )
})

test('5b.8: signed out at #/models the address is rewritten first and the sign-in form shows; signing in opens the list', async () => {
  startAt('#/models')
  const { bff, main } = open({ session: 'signed-out' })
  await within(main).findByText(en('signin-required'))
  expect(window.location.hash, 'rewritten before the form').toBe(LIST_HASH)
  expect(profileReads(bff), 'nothing read while signed out').toEqual([])
  fireEvent.change(
    await within(main).findByRole('textbox', { name: en('signin-username') }),
    { target: { value: 'alice' } },
  )
  fireEvent.change(
    within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  await table(main)
  expect(window.location.hash).toBe(LIST_HASH)
  expect(currentOf(await entry())).toBe('page')
})

test('5b.8: a hashchange to #/models from the Overview rewrites the address, opens the list and moves the focus to its h1', async () => {
  startAt('#/')
  const { main } = open()
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  const entries = window.history.length
  const replace = vi.spyOn(window.history, 'replaceState')
  const push = vi.spyOn(window.history, 'pushState')
  await typeHash('#/models')
  expect(window.location.hash, 'the typed address is rewritten').toBe(LIST_HASH)
  // The typed navigation is the one entry jsdom adds; the app's rewrite adds
  // none, so exactly this one and no more (the hash is typed AFTER the mount,
  // so a rewrite running only on load leaves it unrewritten).
  expect(window.history.length, 'and grows nothing of its own').toBe(
    entries + 1,
  )
  expect(
    replace.mock.calls.map((c) => String(c[2] ?? '')),
    'the rewrite is one replaceState of the new address, path kept',
  ).toEqual([
    `${window.location.pathname}${window.location.search}${LIST_HASH}`,
  ])
  expect(push, 'no pushState: no history entry').not.toHaveBeenCalled()
  const h1 = await heading1(main)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(document.title).toBe(en('page-title', en('model-profiles-title')))
})

test.each([
  ['#/modelsx', 'a longer word'],
  ['#/models-a', 'a dash'],
])(
  '5b.8: %s (%s) is not rewritten: the not-found page, and no list',
  async (hash) => {
    startAt(hash)
    const { bff, main } = open()
    await notFound(main)
    expect(window.location.hash, hash).toBe(hash)
    expect(within(main).queryByRole('table'), hash).toBeNull()
    expect(profileReads(bff), `${hash}: nothing read`).toEqual([])
  },
)

// ── 5b.9: the old #/models/<rest> links ─────────────────────────────────

const OLD: [string, string, string][] = [
  ['#/models/Qwen3-27B', '#/profiles/model/Qwen3-27B', 'the page of that id'],
  [
    '#/models/Qwen%203',
    '#/profiles/model/Qwen%203',
    'an encoded id, kept as it is',
  ],
  ['#/models/%2E%2E', '#/profiles/model/%2E%2E', 'the dot segment of 59a'],
  ['#/models/', '#/profiles/model/', 'an empty rest'],
  ['#/models/a/b', '#/profiles/model/a/b', 'two segments'],
  [
    '#/models/%E0%A4%A',
    '#/profiles/model/%E0%A4%A',
    'a rest that does not decode',
  ],
]

test.each(OLD)(
  '5b.9: %s (%s) becomes %s: same page the new address gives, the rest copied byte for byte',
  async (old, hash) => {
    const entries = window.history.length
    startAt(old)
    const { bff, main } = open()
    expect(window.location.hash, `${old}: rewritten`).toBe(hash)
    expect(window.history.length, `${old}: no history entry`).toBe(entries)
    const rest = hash.slice('#/profiles/model/'.length)
    if (rest === 'Qwen3-27B') {
      expect(
        await within(main).findByRole('heading', { level: 1, name: ID }),
        old,
      ).toBeTruthy()
      // The h1 is the id of the address, so it is on screen in the page's first
      // render, before the read is asked for: wait for the read, this tick is
      // not enough.
      await waitFor(() =>
        expect(profileReads(bff), old).toEqual([`/api/profiles/model/${rest}`]),
      )
    } else if (rest === 'Qwen%203') {
      // The page of `Qwen 3`: the h1 decoded, the read the address's bytes.
      expect(
        await within(main).findByRole('heading', { level: 1, name: 'Qwen 3' }),
        old,
      ).toBeTruthy()
      // The decoded h1 is the address's too, so the same race: wait for the read.
      await waitFor(() =>
        expect(profileReads(bff), old).toEqual([`/api/profiles/model/${rest}`]),
      )
    } else if (rest === '%2E%2E') {
      // 59a: the id is `..`, no read at all. The page is behind the session
      // read, so an async finder.
      expect(
        await within(main).findByText(en('model-profile-not-found', '..')),
        old,
      ).toBeTruthy()
      expect(profileReads(bff), old).toEqual([])
    } else {
      await notFound(main)
      expect(profileReads(bff), old).toEqual([])
    }
  },
)

test('5b.9: on a dirty model profile page, a hashchange to its own old address asks nothing and keeps the edits, and one to another page old address asks first', async () => {
  // The page's own old address: the rewritten address is the one already shown,
  // so the guard has nothing to ask.
  startAt(HASH)
  const first = await loadedPage()
  makeDirty(first.main)
  const select = selectOf(first.main, MODE)
  const edited = select.value
  await typeHash(`#/models/${ID}`)
  expect(
    screen.queryByRole('dialog'),
    'its own old address asks nothing',
  ).toBeNull()
  expect(window.location.hash, 'the address is the rewritten one').toBe(HASH)
  expect(selectOf(first.main, MODE), 'the same form, not a remount').toBe(
    select,
  )
  expect(selectOf(first.main, MODE).value, 'the edit stays').toBe(edited)
  cleanup()
  vi.unstubAllGlobals()
  // Another page's old address: the guard asks, as for any other address.
  dialogs = stubDialog()
  startAt(HASH)
  const second = await loadedPage()
  makeDirty(second.main)
  await typeHash('#/models/gemma-3-12b-it')
  const d = await discardDialog()
  expect(
    window.location.hash,
    'the address stays on the page until the answer',
  ).toBe(HASH)
  expect(document.activeElement, 'Keep editing is focused').toBe(keepButton(d))
  fireEvent.click(keepButton(d))
  await flushTasks()
  expect(window.location.hash).toBe(HASH)
  expect(screen.queryByRole('dialog'), 'Keep editing closed it').toBeNull()
})

// ── 5b.13: the address already shown is no navigation ───────────────────
// The rewritten address of the page the app already shows is the one it shows:
// the app does nothing beyond the rewrite (no focus move, no title, no read,
// the menu as it is, the guard not asked), clean or dirty alike. The focus
// stays in the field the user is typing in (2.4.3, 3.2.1, 3.2.2).

test('5b.13: on a clean model profile page with the focus in a field, a hashchange to its own old address leaves the focus on the field, the title, the reads and the page, and asks nothing', async () => {
  startAt(HASH)
  const { bff, main } = await loadedPage()
  const select = selectOf(main, MODE)
  select.focus()
  const h1 = within(main).getByRole('heading', { level: 1, name: ID })
  const reads = profileReads(bff)
  await typeHash(`#/models/${ID}`)
  expect(window.location.hash, 'the address is the rewritten one').toBe(HASH)
  expect(
    document.activeElement,
    'the focus stays in the field: a user typing is not moved to the h1',
  ).toBe(select)
  expect(document.activeElement, 'not the h1').not.toBe(h1)
  expect(document.title, 'the title of the page shown, unchanged').toBe(
    en('page-title', ID),
  )
  expect(profileReads(bff), 'the page is not read again: no new GET').toEqual(
    reads,
  )
  expect(
    screen.queryByRole('dialog'),
    'the leave guard was not asked',
  ).toBeNull()
})

test('5b.13: the same on a dirty model profile page: the draft is kept, the focus stays in the field, no dialog and no new read', async () => {
  startAt(HASH)
  const { bff, main } = await loadedPage()
  makeDirty(main)
  const select = selectOf(main, MODE)
  const edited = select.value
  select.focus()
  const reads = profileReads(bff)
  await typeHash(`#/models/${ID}`)
  expect(window.location.hash, 'the address is the rewritten one').toBe(HASH)
  expect(
    screen.queryByRole('dialog'),
    'its own old address asks nothing, dirty as clean',
  ).toBeNull()
  expect(selectOf(main, MODE), 'the same form, not a remount').toBe(select)
  expect(selectOf(main, MODE).value, 'the draft is kept').toBe(edited)
  expect(
    document.activeElement,
    'the focus stays in the field the user was editing',
  ).toBe(select)
  expect(document.title, 'the title of the page shown, unchanged').toBe(
    en('page-title', ID),
  )
  expect(profileReads(bff), 'no new read of the page').toEqual(reads)
})

test("5b.13: with the narrow menu open (the Menu button, as 5b.4), a hashchange to the page's own old address leaves the menu open and the focus in the field", async () => {
  startAt(HASH)
  const { bff, main } = await loadedPage()
  const menu = screen.getByRole('button', { name: en('nav-menu') })
  fireEvent.click(menu)
  expect(menu.getAttribute('aria-expanded')).toBe('true')
  const select = selectOf(main, MODE)
  select.focus()
  const reads = profileReads(bff)
  await typeHash(`#/models/${ID}`)
  expect(window.location.hash, 'the address is the rewritten one').toBe(HASH)
  expect(
    menu.getAttribute('aria-expanded'),
    'the menu stays open: nothing was navigated, and no link was activated',
  ).toBe('true')
  expect(document.activeElement, 'the focus stays in the field').toBe(select)
  expect(profileReads(bff), 'no new read of the page').toEqual(reads)
})

// ── 5b.10: focus and title on a rewritten address ───────────────────────

test('5b.10: an old address naming ANOTHER page, with a clean form, is still a navigation: the address is rewritten, the new page shows, its h1 takes the focus and the title follows', async () => {
  startAt(HASH)
  const { main } = await loadedPage()
  const select = selectOf(main, MODE)
  select.focus()
  await typeHash('#/models')
  expect(window.location.hash, 'the old list address is rewritten').toBe(
    LIST_HASH,
  )
  const listH1 = await heading1(main)
  await waitFor(() => expect(document.activeElement).toBe(listH1))
  expect(document.title).toBe(en('page-title', en('model-profiles-title')))
})

test('5b.10: from a model profile page to the list through the nav, and back through the rows link, the h1 takes the focus and the title follows, on the rewritten addresses', async () => {
  startAt(HASH)
  const { main } = await loadedPage()
  navigate(LIST_HASH)
  const listH1 = await heading1(main)
  await waitFor(() => expect(document.activeElement).toBe(listH1))
  expect(document.title).toBe(en('page-title', en('model-profiles-title')))
  const row = (await table(main)).querySelectorAll('a')[0]!
  fireEvent.click(row)
  const pageH1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'Qwen3.6-35B-A3B-UD-Q4_K_XL',
  })
  await waitFor(() => expect(document.activeElement).toBe(pageH1))
  expect(document.title).toBe(en('page-title', 'Qwen3.6-35B-A3B-UD-Q4_K_XL'))
})

// ── 5b.11: the strings ──────────────────────────────────────────────────

test('5b.11: en_US.json has the 05b keys and texts exactly, the two old keys are gone, and the nav gains no Server profiles key', () => {
  expect(S['nav-profiles']).toBe('Profiles')
  expect(S['nav-model-profiles']).toBe('Model profiles')
  expect('nav-models' in S, 'the old nav key is gone').toBe(false)
  expect('models-title' in S, 'the old page title key is gone').toBe(false)
  expect(S['model-profiles-title'], 'it is the h1 now').toBe('Model profiles')
  expect(S['model-profile-back'], 'the back link text is unchanged').toBe(
    'All model profiles',
  )
  expect(S['api-no-model-profile-named']).toBe(
    'There is no model profile named {0} any more: it was renamed or deleted elsewhere. Open the list of all model profiles again to see them as they are.',
  )
  expect(S['model-profile-off']).toBe(
    'This model profile is switched off: it is kept, but its engine does not serve it. To serve it, switch it on in the list of all model profiles.',
  )
  expect('nav-server-profiles' in S, '5b.2').toBe(false)
  // The test mirror follows the file (App.test.tsx pins that they match).
  expect(EN['nav-profiles']).toBe(S['nav-profiles'])
  expect(EN['nav-model-profiles']).toBe(S['nav-model-profiles'])
  expect(EN['api-no-model-profile-named']).toBe(S['api-no-model-profile-named'])
  // The words that name a folder or a llama-server flag stay (5b.11).
  expect(S['server-models-folder']).toBeTruthy()
  expect(S['server-max-models']).toBeTruthy()
})

test('5b.11: the changed text of api-no-model-profile-named is what a 404 no-model-profile-named on a switch shows, with the id filled in', async () => {
  const { bff, main } = open({
    modelProfiles: [modelProfile('Old-Coder', false)],
    modelProfileSwitch: () =>
      reply(404, { key: 'no-model-profile-named', args: ['Old-Coder'] }),
  })
  await table(main)
  press(await switchOf(main, 'Old-Coder'))
  const text = en('api-no-model-profile-named', 'Old-Coder')
  await within(main).findByText(text)
  expect(
    within(main).getByText(text).closest('[role="alert"], [role="status"]'),
    'the text is announced',
  ).not.toBeNull()
  expect(
    bff.calls.filter((c) => c.method === 'PUT'),
    'one write',
  ).toHaveLength(1)
})
