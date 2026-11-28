// Brief 26, slice IP6b (fe/briefs/26-instance-profiles.md, acceptance items 6,
// 7, 8 and 11): the sections under the head of one instance profile's page.
// The Server profile section (26.6) says which server profile the profile runs
// and switches it; the Model profiles section (26.7) lists its model entries
// with their placement count; 26.8 adds an entry and removes one; 26.12 is the
// restart offer every write answering `running` leaves after its status. The
// head (26.5) is IP6a's page and stands here as the fixture; the entry page
// (26.9 to 26.11), the instance page (26.13 to 26.15) and the server profile
// pages are later slices, so nothing here reads them.
//
// The wire is `fe/briefs/api-contract.md` "IP4: the instance profile routes"
// (proposed, served by the bff in IP4c), which brief 26's decision 6 asked
// IP4 to settle: `GET /api/profiles/instance/{p}` answers `{id, server,
// models: [{id, keys}], usedBy, revision}`, the switch is `PUT .../server` with
// `{"server"}`, the add is `POST .../models` with `{"model"}` answering 201,
// and the remove is `DELETE .../models/{m}`. The three writes answer the
// affected answer `{"running": [<instance id>…], "notConverged": [...]}`, and
// carry **no revision**: the contract's "Revisions" keeps one for the entry's
// save alone, which withdraws the revision brief 26 section 3 asked on every
// write. Section 3's 409 `stale-revision` (read again,
// `api-stale-revision-reread`) belongs to the entry page's save alone, the one
// write that carries a revision, so nothing here pins it on the three writes.
//
// The reviewer's four findings are pinned too: a write whose re-read fails
// leaves the page standing with the failure in its section; a 404
// `no-instance-profile-model` and a 409 `instance-profile-model-listed` read the
// profile again, so the stale row or option goes; a remove that empties the list
// leaves the empty line and the `+` and no table of heads alone; a second press
// of Add while the add runs sends nothing.
//
// The IP6b review's R6 to R11 are pinned here as they read on the page: the row
// control is a minus and not a trash can (R6), the switch's options are the
// server profile ids and no port (R8), the device rule of this page speaks with
// `api-instance-profile-device-outside` and its three args (R9), the confirm's
// status during a remove is `instance-profile-model-removing` (R10), and
// strings/en_US.json carries no key twice (R11). R7, the same table not breaking
// a word in two at 320 px, is beyond jsdom's eye and is pinned in
// `fe/ux/check.js`, in the `instance-profile` state.
//
// What jsdom cannot see is left to the designer's `fe/ux` run: the states this
// slice adds, `instance-profile` (the page with three entries),
// `instance-profile-add` (the add form open) and `instance-profile-remove`
// (the confirm open), read the page at 1280 and 320 px, in light and dark.
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
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { instanceProfileHref, instanceProfileModelHref } from './route.ts'
import strings from './strings/en_US.json'
import {
  before,
  deferred,
  en,
  fakeBff,
  quiet,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type InstanceProfile,
  type ModelProfile,
  type ServerProfile,
} from './test/bff.ts'

// ── The fixture ─────────────────────────────────────────────────────────

/** The profile every test opens, and its routes (IP4's paths). */
const PROFILE = 'office'
const READ = `/api/profiles/instance/${PROFILE}`
const SERVER_PUT = `${READ}/server`
const MODELS_POST = `${READ}/models`
const modelPath = (m: string) => `${MODELS_POST}/${encodeURIComponent(m)}`

/** The page's read: the contract's body, over `over`. `models` carries each
 *  entry's id and the count of its keys, which the Placement column reads. */
function doc(
  over: Partial<Record<string, unknown>> = {},
): Record<string, unknown> {
  return {
    id: PROFILE,
    server: 'default',
    models: [
      { id: 'Qwen3-27B', keys: 0 },
      { id: 'gemma-3-12b-it', keys: 3 },
    ],
    usedBy: ['default'],
    revision: 'r1',
    ...over,
  }
}

/** The reads answer these rows in order, the last one for ever after: a page
 *  that writes reads the profile again, and the second answer carries what the
 *  write did (26.8's rows). */
function readsInOrder(rows: Record<string, unknown>[]) {
  let n = 0
  return () => reply(200, rows[Math.min(n++, rows.length - 1)] ?? doc())
}

/** The node's server profiles: three, so the switch has another one to list.
 *  Each carries the port of the instance that runs it (what
 *  `GET /api/profiles/server` answers): R8 is only a pin when the row has a port
 *  for the option to name, and the option must not name it. */
const SERVERS: ServerProfile[] = [
  { id: 'default', tags: [], port: 8080 },
  { id: 'fast', tags: [], port: 8081 },
  { id: 'lab', tags: [], port: 8082 },
]

/** The model profile pool: the page's two entries, so the add has nothing left
 *  to list, and a test that leaves one out of the entries has one. */
const MODEL_POOL: ModelProfile[] = [
  {
    id: 'Qwen3-27B',
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: true,
    usedBy: [PROFILE],
    model: '/m/Qwen3-27B.gguf',
  },
  {
    id: 'gemma-3-12b-it',
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: true,
    usedBy: [PROFILE],
    model: '/m/gemma-3-12b-it.gguf',
  },
]

/** The pool, for the app's own reads (the nav, the list the back link leads to). */
const POOL: InstanceProfile[] = [
  {
    id: PROFILE,
    server: 'default',
    models: ['Qwen3-27B', 'gemma-3-12b-it'],
    usedBy: ['default'],
  },
]

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  dialogs = stubDialog()
  startAt(instanceProfileHref(PROFILE))
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** The English of a key, read from strings/en_US.json as the page reads it
 *  (the IP6a review's R4 reads its words the same way): the page shows what the
 *  file says, so a test never restates a sentence, and any words the coder
 *  chose are green. A key the slice has not added reads as the key in brackets,
 *  which no element of the page carries, so a section test fails on the page it
 *  cannot find, as it does today, and the strings test at the end of the file
 *  is the one that names the missing key. */
function words(key: string, ...args: string[]): string {
  const english = (strings as Record<string, string>)[key] ?? `[${key}]`
  let out = english
  args.forEach((a, i) => {
    out = out.replace(`{${i}}`, a)
  })
  return out
}

/** Every key this slice's tests read a control or a sentence by: 26.10 for the
 *  two sections, the reasons 26.6 and 26.8 list (the `api-` keys, whose words
 *  come from the reason file the strings file mirrors), 26.12 for the offer,
 *  and the keys of the parts of the page the sections sit in. The last test
 *  names the ones the file lacks; `1a strings` in App.test.tsx pins the English
 *  of the keys whose words the brief gives, as the list grows. */
const KEYS = [
  // 26.6, the Server profile section.
  'instance-profile-server-title',
  'instance-profile-server-switch-label',
  'instance-profile-server-switch',
  'instance-profile-server-switch-hint',
  'instance-profile-server-choose',
  'instance-profile-server-switching',
  'instance-profile-server-switched',
  'api-no-server-profile-named',
  'api-no-instance-profile-named',
  // R9: the device rule of this page, with its three args.
  'api-instance-profile-device-outside',
  // 26.7, the Model profiles section.
  'instance-profile-models-title',
  'instance-profile-models-intro',
  'instance-profile-models-column-name',
  'instance-profile-models-column-placement',
  'instance-profile-placement-default',
  'instance-profile-placement-count',
  'instance-profile-placement-count-one',
  'instance-profile-models-empty',
  // 26.8, add a model and remove one.
  'instance-profile-models-add',
  'instance-profile-models-add-title',
  'instance-profile-models-add-label',
  'instance-profile-models-add-button',
  'instance-profile-models-add-none',
  'instance-profile-model-added',
  'instance-profile-models-remove-row',
  'instance-profile-models-remove-question',
  'instance-profile-models-remove',
  // R10: what the confirm's status says while the DELETE runs.
  'instance-profile-model-removing',
  'instance-profile-model-removed',
  'api-no-model-profile-named',
  'api-instance-profile-model-listed',
  'api-no-instance-profile-model',
  // 26.12, the offer every write answering `running` leaves.
  'restart-offer-button',
  'restart-offer-running',
  'restart-offer-sent',
  // Section 3's answers, and the page the sections sit in.
  'session-ended-retry',
  'session-revoked',
  'session-sign-in',
  'signin-title',
  'request-failed',
  'page-title',
  'profiles-column-actions',
  'confirm-cancel',
  'field-error-choose',
  'nav-model-profiles',
]

/** The app on the profile's page. */
function open(options: BffOptions = {}) {
  const bff = fakeBff({
    instanceProfiles: POOL,
    instanceProfile: () => reply(200, doc()),
    serverProfiles: SERVERS,
    modelProfiles: MODEL_POOL,
    ...options,
  })
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** The page with its head read (26.5): the h1 is the id, the title names it,
 *  the bff is quiet. The title only, never the focus: these tests arrive at the
 *  address (a direct load moves no focus, brief 09); the h1 focus this page
 *  owes is the create's 201 arrival, which is IP6a's page (26.4), pinned in
 *  InstanceProfiles1. */
async function loaded(options: BffOptions = {}) {
  const r = open(options)
  await within(r.main).findByRole('heading', { level: 1, name: PROFILE })
  await waitFor(() =>
    expect(document.title, 'the title names the profile').toBe(
      en('page-title', PROFILE),
    ),
  )
  await quiet(r.bff)
  return r
}

function flushTasks(n = 2) {
  return act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The section of the page named by the `h2` the brief gives it. */
async function section(main: HTMLElement, key: string): Promise<HTMLElement> {
  const h2 = await within(main).findByRole('heading', {
    level: 2,
    name: en(key),
  })
  const s = h2.closest('section') ?? h2.parentElement
  expect(s, `the ${key} section holds its heading`).not.toBeNull()
  return s as HTMLElement
}

function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function choose(select: HTMLElement, value: string) {
  fireEvent.change(select, { target: { value } })
}

function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

/** The texts of a control's `aria-describedby` runs. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** What `el` says about itself at the moment the focus lands on it (the
 *  InstanceProfiles1 watchFocus, read for the section instead of a form). */
function watchFocus(el: HTMLElement, wanted: string) {
  const at = { focused: false, described: [] as string[], shown: false }
  el.addEventListener('focus', () => {
    at.focused = true
    at.described = (el.getAttribute('aria-describedby') ?? '')
      .split(/\s+/)
      .filter(Boolean)
      .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
    at.shown = (el.closest('section')?.textContent ?? '').includes(wanted)
  })
  return at
}

/** The offer's buttons on screen, in DOM order: every button named
 *  `restart-offer-button` filled with a name (26.12). */
function offerButtons(root: HTMLElement): HTMLButtonElement[] {
  return within(root).queryAllByRole('button', {
    name: /^Restart /,
  }) as HTMLButtonElement[]
}

/** The status line holding `wanted`, anywhere in `root`. */
function statusOf(root: HTMLElement, wanted: string): HTMLElement {
  const el = within(root).getByText(wanted)
  const status = el.closest('[role="status"]')
  expect(status, `${wanted}: in a status line`).not.toBeNull()
  return status as HTMLElement
}

/** The `role="alert"` of `root` holding `wanted`. */
function alertOf(root: HTMLElement, wanted: string): HTMLElement {
  const el = within(root).getByText(wanted)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${wanted}: in a role="alert"`).not.toBeNull()
  return alert as HTMLElement
}

// ── 26.6: the Server profile section ────────────────────────────────────

/** The section of 26.6, with its heading and its link to the server it runs. */
async function serverSection(main: HTMLElement): Promise<HTMLElement> {
  const s = await section(main, 'instance-profile-server-title')
  await within(s).findByRole('link', { name: 'default' })
  return s
}

function switchSelect(s: HTMLElement): HTMLSelectElement {
  return within(s).getByRole('combobox', {
    name: en('instance-profile-server-switch-label'),
  }) as HTMLSelectElement
}

function switchButton(s: HTMLElement): HTMLButtonElement {
  return within(s).getByRole('button', {
    name: en('instance-profile-server-switch'),
  }) as HTMLButtonElement
}

/** The section's own status line, the restart offer's left out: 26.12 puts the
 *  offer after the status, and `RestartOffer` says its running and sent lines
 *  in a status of its own, so a section with an offer has two. */
function sectionStatus(s: HTMLElement): HTMLElement {
  const all = within(s)
    .queryAllByRole('status')
    .filter((el) => el.closest('.restart-offer') === null)
  expect(
    all,
    'the section has its one status line beside the offer (26.12)',
  ).toHaveLength(1)
  return all[0]!
}

/** The reads of the profile the page has sent. Section 3 asks for a read after
 *  every write, so a page that wrote sent two: its own read and the re-read.
 *  Count them once the app is quiet, the way every exact count is counted. */
function readCount(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', READ).length
}

function sentSwitch(bff: ReturnType<typeof fakeBff>): unknown[] {
  return bff.calls
    .filter((c) => c.method === 'PUT' && c.path === SERVER_PUT)
    .map((c) => c.body)
}

function sentAdd(bff: ReturnType<typeof fakeBff>): unknown[] {
  return bff.calls
    .filter((c) => c.method === 'POST' && c.path === MODELS_POST)
    .map((c) => c.body)
}

function sentRemove(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter(
      (c) => c.method === 'DELETE' && c.path.startsWith(`${MODELS_POST}/`),
    )
    .map((c) => c.path)
}

/** Choose a server profile and press: the select alone sends nothing. */
async function switchTo(
  s: HTMLElement,
  bff: ReturnType<typeof fakeBff>,
  server: string,
) {
  choose(switchSelect(s), server)
  expect(sentSwitch(bff), 'the select alone sends nothing').toEqual([])
  press(switchButton(s))
  await waitFor(() => expect(sentSwitch(bff)).toHaveLength(1))
}

test('26.6: the section is named instance-profile-server-title, names the server profile it runs as a link to that profile, and its Switch to field lists every other server profile of the pool, with the brief hint describing the select', async () => {
  const { bff, main } = await loaded()
  const s = await serverSection(main)
  const h2 = within(s).getByRole('heading', {
    level: 2,
    name: en('instance-profile-server-title'),
  })
  const link = within(s).getByRole('link', { name: 'default' })
  expect(before(h2, link), 'the link is under the heading').toBe(true)
  expect(
    link.getAttribute('href'),
    'the link leads to that server profile',
  ).toBe('#/profiles/server/default')
  const select = switchSelect(s)
  const button = switchButton(s)
  expect(before(link, select), 'the field follows the link').toBe(true)
  expect(before(select, button), 'the button follows the select').toBe(true)
  expect(
    [...select.options].map((o) => o.value).filter((v) => v !== ''),
    'every other server profile of the pool, the one it runs left out (26.6, P1)',
  ).toEqual(['fast', 'lab'])
  // R8 (26.6): the option is named by the id alone. The port is the instance's
  // since IP6, so `instance-server-option` ("{0} (port {1})") has no place on
  // this page: it would name a port this profile does not have.
  expect(
    [...select.options].map((o) => text(o)),
    'R8: each option says its server profile id, nothing else',
  ).toEqual([en('instance-profile-server-choose'), 'fast', 'lab'])
  expect(
    [...select.options].map((o) => text(o)).join(' '),
    'R8: no option names a port',
  ).not.toContain('port')
  expect(inactive(button), 'the button waits for a choice').toBe(true)
  expect(described(select), 'the hint describes the select').toContain(
    en('instance-profile-server-switch-hint'),
  )
  await quiet(bff)
  expect(
    bff.to('GET', READ).length,
    'the page reads the profile, at the address',
  ).toBeGreaterThan(0)
  expect(sentSwitch(bff), 'the page writes nothing at rest').toEqual([])
})

test('26.6, 17d2.3: the field shows only when the pool holds another server profile: a pool of the one it runs has the link, no select and no button', async () => {
  const { main } = await loaded({
    serverProfiles: [{ id: 'default', tags: [] }],
  })
  const s = await serverSection(main)
  within(s).getByRole('link', { name: 'default' })
  expect(
    within(s).queryByRole('combobox', {
      name: en('instance-profile-server-switch-label'),
    }),
    'no other profile: no select',
  ).toBeNull()
  expect(
    within(s).queryByRole('button', {
      name: en('instance-profile-server-switch'),
    }),
    'no other profile: no button',
  ).toBeNull()
})

test('26.6: the switch sends PUT .../server with the chosen id alone, reads instance-profile-server-switching with the button switched off while it runs and ignores a second press, and on the answer names the new server profile, reads instance-profile-server-switched and reads the profile again', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    instanceProfileServerSet: () => gate.promise,
  })
  const s = await serverSection(main)
  const reads = bff.to('GET', READ).length
  await switchTo(s, bff, 'fast')
  expect(
    sentSwitch(bff)[0],
    'the body is the server profile alone: the contract sends no revision here',
  ).toEqual({ server: 'fast' })
  expect(text(sectionStatus(s)), 'the send says so').toBe(
    en('instance-profile-server-switching'),
  )
  expect(inactive(switchButton(s)), 'the button is switched off').toBe(true)
  press(switchButton(s))
  press(switchButton(s))
  expect(
    sentSwitch(bff),
    'a second and a third press send nothing',
  ).toHaveLength(1)
  gate.resolve(reply(200, { running: [], notConverged: [] }))
  await flushTasks(6)
  expect(text(sectionStatus(s)), 'the answer').toBe(
    en('instance-profile-server-switched', 'fast'),
  )
  const link = within(s).getByRole('link', { name: 'fast' })
  expect(
    link.getAttribute('href'),
    'the section names the server profile it runs now',
  ).toBe('#/profiles/server/fast')
  expect(
    bff.to('GET', READ).length,
    'the profile read again, so the section is the file',
  ).toBeGreaterThan(reads)
})

test('26.6, 26.12: a switch answering running leaves one Restart per instance named by its name, and pressing one sends that instance restart; an empty running list leaves no offer', async () => {
  const { bff, main } = await loaded({
    instanceProfileServerSet: () =>
      reply(200, { running: ['default', 'coder'], notConverged: [] }),
  })
  const s = await serverSection(main)
  await switchTo(s, bff, 'fast')
  await flushTasks(6)
  expect(text(sectionStatus(s)), 'the switch itself is done').toBe(
    en('instance-profile-server-switched', 'fast'),
  )
  expect(
    offerButtons(main).map((b) => text(b)),
    'one button per running instance, by name (26.12)',
  ).toEqual([
    en('restart-offer-button', 'default'),
    en('restart-offer-button', 'coder'),
  ])
  press(
    within(main).getByRole('button', {
      name: en('restart-offer-button', 'coder'),
    }),
  )
  await waitFor(() =>
    expect(
      bff.to('POST', '/api/instances/coder/restart').length,
      'that instance restart',
    ).toBe(1),
  )
  expect(
    bff.to('POST', '/api/instances/default/restart'),
    'only the one pressed',
  ).toHaveLength(0)
  cleanup()
  const empty = await loaded({
    instanceProfileServerSet: () =>
      reply(200, { running: [], notConverged: [] }),
  })
  const s2 = await serverSection(empty.main)
  await switchTo(s2, empty.bff, 'fast')
  await flushTasks(6)
  expect(
    offerButtons(empty.main),
    'an empty running list shows no offer (26.12)',
  ).toEqual([])
})

test('26.12: the offer says its own lines: restart-offer-running with the name while the restart runs, and restart-offer-sent when it answers', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    instanceProfileServerSet: () =>
      reply(200, { running: ['default'], notConverged: [] }),
    run: () => gate.promise,
  })
  const s = await serverSection(main)
  await switchTo(s, bff, 'fast')
  await flushTasks(6)
  const button = offerButtons(main)[0]!
  press(button)
  await waitFor(() =>
    expect(
      bff.to('POST', '/api/instances/default/restart').length,
      'the restart sent',
    ).toBe(1),
  )
  expect(
    text(statusOf(main, en('restart-offer-running', 'default'))),
    'the offer says it is restarting',
  ).toBe(en('restart-offer-running', 'default'))
  gate.resolve(reply(204))
  await flushTasks(6)
  expect(
    text(statusOf(main, en('restart-offer-sent', 'default'))),
    'the answer',
  ).toBe(en('restart-offer-sent', 'default'))
})

test('26.6: a 422 no-server-profile-named with field server says the file words in the section, marks the select aria-invalid, describes it by them, and moves the focus there once they are rendered', async () => {
  const { main } = await loaded({
    instanceProfileServerSet: () =>
      reply(422, {
        key: 'no-server-profile-named',
        args: ['ghost'],
        field: 'server',
      }),
  })
  const s = await serverSection(main)
  const select = switchSelect(s)
  const sentence = en('api-no-server-profile-named', 'ghost')
  const at = watchFocus(select, sentence)
  choose(select, 'fast')
  press(switchButton(s))
  await waitFor(() =>
    expect(select.getAttribute('aria-invalid'), 'the select is marked').toBe(
      'true',
    ),
  )
  expect(text(s), 'the words are in the section').toContain(sentence)
  expect(
    alertOf(s, sentence),
    'the words are in the section alert',
  ).toBeTruthy()
  expect(at.focused, 'the focus moves to the marked select').toBe(true)
  expect(at.shown, 'the focus moves after the words render').toBe(true)
  expect(
    at.described.some((d) => d.includes(sentence)),
    'the select is described by the words',
  ).toBe(true)
})

test('26.6: the refusals go in the section alert by their own words, and the section keeps the server profile it runs: model-profile-device-outside, 401 with a Sign in that opens the sign-in dialog, 403, a 404, and anything else as request-failed', async () => {
  const cases: Array<[() => Response, string]> = [
    [
      // R9 (26.6): the device rule answers with three args (the model, the key,
      // the device) and this page's own key: today's
      // `api-model-profile-device-outside` takes four and names an instance, so
      // with these args it would leave a raw `{3}` on the screen.
      () =>
        reply(422, {
          key: 'model-profile-device-outside',
          args: ['Qwen3-27B', 'tensor-split', '40,25'],
        }),
      en(
        'api-instance-profile-device-outside',
        'Qwen3-27B',
        'tensor-split',
        '40,25',
      ),
    ],
    [
      () => reply(401, { key: 'session-required', args: [] }),
      en('session-ended-retry'),
    ],
    [
      () => reply(403, { key: 'session-not-administrator', args: [] }),
      en('session-revoked'),
    ],
    [
      () => reply(404, { key: 'no-instance-profile-named', args: [] }),
      en('api-no-instance-profile-named', PROFILE),
    ],
    [() => reply(500), en('request-failed')],
  ]
  for (const [answer, sentence] of cases) {
    const { bff, main } = await loaded({ instanceProfileServerSet: answer })
    const s = await serverSection(main)
    await switchTo(s, bff, 'fast')
    await flushTasks(6)
    expect(text(alertOf(s, sentence)), sentence).toBe(sentence)
    expect(
      text(within(s).getByRole('link', { name: 'default' })),
      `${sentence}: the link stays on the server profile it runs`,
    ).toBe('default')
    cleanup()
  }
  const signed = await loaded({
    instanceProfileServerSet: () =>
      reply(401, { key: 'session-required', args: [] }),
  })
  const s = await serverSection(signed.main)
  const button = switchButton(s)
  await switchTo(s, signed.bff, 'fast')
  await flushTasks(6)
  const alert = alertOf(s, en('session-ended-retry'))
  const signIn = within(s).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(before(alert, signIn), 'the Sign in follows the alert').toBe(true)
  expect(document.activeElement, 'the focus back on the switch').toBe(button)
  press(signIn)
  await screen.findByRole('dialog', { name: en('signin-title') })
})

test('section 3, the review: a switch that worked and whose re-read of the profile fails keeps the page standing and says the failure in its section', async () => {
  // The reviewer's first finding. Section 3 reads the profile again after every
  // write, and that read can fail (a 503, a network fault) after a write the
  // manager accepted. The page keeps the record it has: the section that lost
  // its re-read says so, and the rest of the page stays, because a whole-page
  // failure would throw away a record that is on the manager and a write that
  // worked.
  let reads = 0
  let server = 'default'
  const { bff, main } = await loaded({
    instanceProfile: () => {
      reads += 1
      // The page's own read works; the re-read the write asks for does not; the
      // read after the next write works again.
      if (reads === 2) return reply(503)
      return reply(200, doc({ server }))
    },
    instanceProfileServerSet: (body) => {
      server = (body as { server: string }).server
      return reply(200, { running: [], notConverged: [] })
    },
  })
  const s = await serverSection(main)
  await switchTo(s, bff, 'fast')
  await flushTasks(8)
  expect(readCount(bff), 'the write was followed by a read').toBe(2)
  // The section says the read did not go through, with the page's own words for
  // a request that failed (`request-failed`, the one `FAILED` already uses).
  expect(text(s), 'the section says its read failed').toContain(
    en('request-failed'),
  )
  expect(
    text(main),
    'the page is not replaced by the page-level failure line',
  ).not.toContain(en('instance-profiles-failed'))
  within(main).getByRole('heading', { level: 1, name: PROFILE })
  within(main).getByRole('heading', {
    level: 2,
    name: en('instance-profile-models-title'),
  })
  expect(
    modelRows(await section(main, 'instance-profile-models-title')).map((r) =>
      text(r.querySelector('a')),
    ),
    'the Model profiles section keeps its rows',
  ).toEqual(['Qwen3-27B', 'gemma-3-12b-it'])
  // And the page keeps working: the next switch writes and reads its way to a
  // good answer, and the section stops saying anything failed.
  choose(switchSelect(s), 'lab')
  press(switchButton(s))
  await waitFor(() => expect(sentSwitch(bff)).toHaveLength(2))
  await flushTasks(8)
  expect(
    sentSwitch(bff),
    'the two switches went out, the failed read in between wrote nothing',
  ).toEqual([{ server: 'fast' }, { server: 'lab' }])
  expect(text(s), 'the section is clean again').not.toContain(
    en('request-failed'),
  )
  expect(
    text(s),
    'the section reads the server profile the file holds',
  ).toContain('lab')
})

// ── 26.7: the Model profiles section ────────────────────────────────────

/** The section of 26.7, with its heading and its intro. */
async function modelsSection(main: HTMLElement): Promise<HTMLElement> {
  const s = await section(main, 'instance-profile-models-title')
  await within(s).findByText(en('instance-profile-models-intro'))
  return s
}

function modelsTable(s: HTMLElement): HTMLElement {
  const t = s.querySelector('table')
  expect(t, 'the section holds a table').not.toBeNull()
  return t as HTMLElement
}

/** The table's body rows, the confirm row left out. */
function modelRows(s: HTMLElement): HTMLElement[] {
  return [...modelsTable(s).querySelectorAll<HTMLElement>('tbody tr')].filter(
    (r) => !r.classList.contains('confirm-row'),
  )
}

/** The row of one model entry, found by the id of the link it holds. */
function modelRow(s: HTMLElement, id: string): HTMLElement {
  const row = modelRows(s).find((r) =>
    [...r.querySelectorAll('a')].some((a) => text(a) === id),
  )
  expect(row, `the row of ${id}`).not.toBeUndefined()
  return row!
}

function removeMinus(s: HTMLElement, id: string): HTMLElement {
  return within(modelRow(s, id)).getByRole('button', {
    name: en('instance-profile-models-remove-row', id),
  })
}

test('26.7: the section is named instance-profile-models-title with its intro under it, its head is Model profile, Placement and a visually hidden Actions, and its rows are the entries in the read order, the id a link to the entry page and the row minus in the row', async () => {
  const { main } = await loaded()
  const s = await modelsSection(main)
  const h2 = within(s).getByRole('heading', {
    level: 2,
    name: en('instance-profile-models-title'),
  })
  const intro = within(s).getByText(en('instance-profile-models-intro'))
  expect(before(h2, intro), 'the intro is under the heading').toBe(true)
  expect(
    [...modelsTable(s).querySelectorAll('thead th')].map((th) => text(th)),
    'the three columns of 26.7',
  ).toEqual([
    en('instance-profile-models-column-name'),
    en('instance-profile-models-column-placement'),
    en('profiles-column-actions'),
  ])
  expect(
    modelsTable(s)
      .querySelectorAll('thead th')[2]
      ?.querySelector('.visually-hidden'),
    'the Actions head is the eye-invisible one',
  ).not.toBeNull()
  expect(
    modelRows(s).map((r) => text(r.querySelector('a'))),
    'the rows keep the read order, not the names in a sort',
  ).toEqual(['Qwen3-27B', 'gemma-3-12b-it'])
  for (const id of ['Qwen3-27B', 'gemma-3-12b-it']) {
    const link = within(modelRow(s, id)).getByRole('link', { name: id })
    expect(
      link.getAttribute('href'),
      `${id}: the row leads to this profile's entry page for the model`,
    ).toBe(instanceProfileModelHref(PROFILE, id))
  }
  expect(
    removeMinus(s, 'Qwen3-27B'),
    'the row minus names itself with the model',
  ).toBeTruthy()
})

test('26.7: the Placement cell says the defaults for no keys, muted, the singular for one and the count for many', async () => {
  const cases: Array<[number, string, boolean]> = [
    [0, words('instance-profile-placement-default'), true],
    [1, words('instance-profile-placement-count-one'), false],
    [7, words('instance-profile-placement-count', '7'), false],
  ]
  for (const [keys, wanted, muted] of cases) {
    const { main } = await loaded({
      instanceProfile: () =>
        reply(200, doc({ models: [{ id: 'Qwen3-27B', keys }] })),
    })
    const s = await modelsSection(main)
    const cell = modelRow(s, 'Qwen3-27B').querySelectorAll('td')[1]
    expect(text(cell), `${keys} keys`).toBe(wanted)
    expect(
      cell!.querySelector('.muted') !== null,
      `${keys} keys: the defaults line is the muted one, the count is not`,
    ).toBe(muted)
    cleanup()
  }
})

test('26.7: with no entry the section says instance-profile-models-empty, has no table, and the add follows the line', async () => {
  const { main } = await loaded({
    instanceProfile: () => reply(200, doc({ models: [] })),
  })
  const s = await modelsSection(main)
  const empty = await within(s).findByText(en('instance-profile-models-empty'))
  expect(s.querySelector('table'), 'no entry: no table').toBeNull()
  expect(
    before(empty, addButton(s)),
    'the empty line stands where the table was, the add after it',
  ).toBe(true)
})

// ── 26.8: add a model profile ───────────────────────────────────────────

function addButton(s: HTMLElement): HTMLElement {
  return within(s).getByRole('button', {
    name: en('instance-profile-models-add'),
  })
}

/** Press the `+` and return the form it opened in place. */
async function addForm(s: HTMLElement): Promise<HTMLElement> {
  press(addButton(s))
  return within(s).findByRole('form', {
    name: en('instance-profile-models-add-title'),
  })
}

function addSelect(form: HTMLElement): HTMLSelectElement {
  return within(form).getByRole('combobox', {
    name: en('instance-profile-models-add-label'),
  }) as HTMLSelectElement
}

function addVerb(form: HTMLElement): HTMLElement {
  return within(form).getByRole('button', {
    name: en('instance-profile-models-add-button'),
  })
}

/** The options of a select, the placeholder left out. */
function values(select: HTMLSelectElement): string[] {
  return [...select.options].map((o) => o.value).filter((v) => v !== '')
}

test('26.8: the add opens a form in place named instance-profile-models-add-title, whose select lists the model profiles not yet listed by id, with a primary Add and a Cancel; no choice gives field-error-choose on the select, described by it, with the focus there after the words, and sends nothing', async () => {
  const { bff, main } = await loaded({
    instanceProfile: () =>
      reply(200, doc({ models: [{ id: 'Qwen3-27B', keys: 0 }] })),
  })
  const s = await modelsSection(main)
  const form = await addForm(s)
  const select = addSelect(form)
  expect(
    values(select),
    'the pool less what the profile already lists (26.8)',
  ).toEqual(['gemma-3-12b-it'])
  expect(select.value, 'nothing chosen at rest').toBe('')
  const verb = addVerb(form)
  const cancel = within(form).getByRole('button', {
    name: en('confirm-cancel'),
  })
  expect(verb.className.split(/\s+/), 'Add is the primary verb').toContain(
    'primary',
  )
  expect(cancel.className.split(/\s+/), 'Cancel is secondary').toContain(
    'secondary',
  )
  const at = watchFocus(select, en('field-error-choose'))
  press(verb)
  await flushTasks()
  expect(select.getAttribute('aria-invalid'), 'the select is marked').toBe(
    'true',
  )
  expect(text(form), 'the check says choose one').toContain(
    en('field-error-choose'),
  )
  expect(at.focused, 'the focus is on the select').toBe(true)
  expect(at.shown, 'the focus follows the words').toBe(true)
  expect(
    at.described.some((d) => d.includes(en('field-error-choose'))),
    'the select is described by the words',
  ).toBe(true)
  await quiet(bff)
  expect(sentAdd(bff), 'a page check sends nothing').toEqual([])
  expect(readCount(bff), 'a page check reads nothing new').toBe(1)
  press(cancel)
  await flushTasks()
  expect(s.querySelector('form'), 'Cancel closes the form').toBeNull()
})

test('26.8: Add sends POST .../models with the model alone; the form closes, the row is appended, the focus is on its link, the status reads instance-profile-model-added, and a running list leaves its offer', async () => {
  const { bff, main } = await loaded({
    instanceProfile: readsInOrder([
      doc({ models: [{ id: 'Qwen3-27B', keys: 0 }] }),
      doc({
        models: [
          { id: 'Qwen3-27B', keys: 0 },
          { id: 'gemma-3-12b-it', keys: 0 },
        ],
      }),
    ]),
    instanceProfileModelAdd: () =>
      reply(201, { running: ['default'], notConverged: [] }),
  })
  const s = await modelsSection(main)
  const form = await addForm(s)
  choose(addSelect(form), 'gemma-3-12b-it')
  press(addVerb(form))
  await waitFor(() => expect(sentAdd(bff)).toHaveLength(1))
  expect(
    sentAdd(bff)[0],
    'the body is the model alone: no revision on the add',
  ).toEqual({ model: 'gemma-3-12b-it' })
  await flushTasks(8)
  expect(s.querySelector('form'), 'a success closes the form').toBeNull()
  const rows = modelRows(s)
  expect(
    rows.map((r) => text(r.querySelector('a'))),
    'the new row is appended, the file order kept',
  ).toEqual(['Qwen3-27B', 'gemma-3-12b-it'])
  const link = within(rows[1]!).getByRole('link', { name: 'gemma-3-12b-it' })
  expect(document.activeElement, 'the focus is on the new row link').toBe(link)
  const sentence = en('instance-profile-model-added', 'gemma-3-12b-it')
  expect(text(statusOf(s, sentence)), sentence).toBe(sentence)
  expect(
    offerButtons(main).map((b) => text(b)),
    'the add answers running: the offer stands after the status',
  ).toEqual([en('restart-offer-button', 'default')])
  await quiet(bff)
  expect(
    readCount(bff),
    'the add wrote: the profile is read again (section 3)',
  ).toBe(2)
})

test('26.8: with every model profile of the pool listed, the form says instance-profile-models-add-none with a link to the Model profiles page, has no select, and the add stays', async () => {
  const { main } = await loaded()
  const s = await modelsSection(main)
  const form = await addForm(s)
  expect(
    within(form).getByText(en('instance-profile-models-add-none')),
    'the form says every profile is listed',
  ).toBeTruthy()
  expect(
    within(form).queryByRole('combobox'),
    'nothing left: no select',
  ).toBeNull()
  const link = within(form).getByRole('link', {
    name: en('nav-model-profiles'),
  })
  expect(
    link.getAttribute('href'),
    'and points at the Model profiles page',
  ).toBe('#/profiles/model')
  expect(
    within(s).queryByRole('button', {
      name: en('instance-profile-models-add'),
    }),
    'the add stays (26.8: with none left the + stays)',
  ).toBeTruthy()
})

test('26.8: the add refusal of the page check goes in the section alert by its file words, on its field, and the form stays open with its choice', async () => {
  const { bff, main } = await loaded({
    instanceProfile: () =>
      reply(200, doc({ models: [{ id: 'Qwen3-27B', keys: 0 }] })),
    instanceProfileModelAdd: () =>
      reply(422, {
        key: 'no-model-profile-named',
        args: ['Ghost'],
        field: 'model',
      }),
  })
  const s = await modelsSection(main)
  const form = await addForm(s)
  const select = addSelect(form)
  const sentence = words('api-no-model-profile-named', 'Ghost')
  const at = watchFocus(select, sentence)
  choose(select, 'gemma-3-12b-it')
  press(addVerb(form))
  await waitFor(() => expect(sentAdd(bff)).toHaveLength(1))
  await flushTasks(6)
  expect(text(alertOf(s, sentence)), sentence).toBe(sentence)
  expect(select.getAttribute('aria-invalid'), 'the select is marked').toBe(
    'true',
  )
  expect(at.focused, 'the focus is on the marked select').toBe(true)
  expect(s.querySelector('form'), 'the form stays open').not.toBeNull()
  expect(select.value, 'with its choice').toBe('gemma-3-12b-it')
})

test('26.8, the review: an add refused 409 instance-profile-model-listed reads the profile again, so the option the file already lists goes out of the form', async () => {
  // The reviewer's second finding, half two. The 409 says the entry is on the
  // manager after all: the open form lists what the profile does not have, so
  // the page reads the profile again and the select follows. With the model
  // listed, 26.8 leaves the form saying every profile is listed, and its way to
  // create one.
  const { bff, main } = await loaded({
    instanceProfile: readsInOrder([
      doc({ models: [{ id: 'Qwen3-27B', keys: 0 }] }),
      doc(),
    ]),
    instanceProfileModelAdd: () =>
      reply(409, {
        key: 'instance-profile-model-listed',
        args: ['gemma-3-12b-it'],
      }),
  })
  const s = await modelsSection(main)
  const form = await addForm(s)
  const reads = readCount(bff)
  choose(addSelect(form), 'gemma-3-12b-it')
  press(addVerb(form))
  await waitFor(() => expect(sentAdd(bff)).toHaveLength(1))
  await flushTasks(8)
  const sentence = en('api-instance-profile-model-listed', 'gemma-3-12b-it')
  expect(text(alertOf(s, sentence)), sentence).toBe(sentence)
  expect(
    modelRows(s).map((r) => text(r.querySelector('a'))),
    'the row the file lists is on the table',
  ).toEqual(['Qwen3-27B', 'gemma-3-12b-it'])
  expect(s.querySelector('form'), 'the form stays open').not.toBeNull()
  expect(
    within(form).queryByRole('combobox'),
    'the stale option is gone: nothing is left to add, so the form says so',
  ).toBeNull()
  within(form).getByText(en('instance-profile-models-add-none'))
  await quiet(bff)
  expect(readCount(bff), 'the refusal read the profile again').toBeGreaterThan(
    reads,
  )
})

test('26.8, the review: a second press of Add while the add runs sends nothing', async () => {
  // The reviewer's fourth finding: the switch says it for the switch (26.6), and
  // the add owes the same. A second POST of the same model is a second write of
  // a record the manager already has.
  const gate = deferred()
  const { bff, main } = await loaded({
    instanceProfile: () =>
      reply(200, doc({ models: [{ id: 'Qwen3-27B', keys: 0 }] })),
    instanceProfileModelAdd: () => gate.promise,
  })
  const s = await modelsSection(main)
  const form = await addForm(s)
  choose(addSelect(form), 'gemma-3-12b-it')
  const verb = addVerb(form)
  press(verb)
  await waitFor(() => expect(sentAdd(bff)).toHaveLength(1))
  press(verb)
  press(verb)
  await flushTasks(4)
  expect(sentAdd(bff), 'the add running sends nothing more').toHaveLength(1)
  gate.resolve(reply(201, { running: [], notConverged: [] }))
  await flushTasks(8)
  expect(sentAdd(bff), 'and one add in all').toHaveLength(1)
})

// ── 26.8: remove a model profile ────────────────────────────────────────

/** What `Icon` draws for the minus (22 D2.2, the list-remove sign of
 *  DESIGN.md 5.2): the plus's horizontal stroke, with no vertical one. */
const MINUS = ['M5 12h14']

/** What `Icon` draws for `trash-2` (22 D4.21, of lucide-static 1.74.0): the
 *  five paths the package writes, in its order. R6 keeps it off this table: the
 *  can reads as deleting the model profile, which stays. */
const TRASH = [
  'M10 11v6',
  'M14 11v6',
  'M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6',
  'M3 6h18',
  'M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2',
]

/** The `d` of every path `el` draws, in document order. */
function paths(el: Element): string[] {
  return [...el.querySelectorAll('svg path')].map((p) => p.getAttribute('d')!)
}

/** Press a row's minus and return the Confirm group it opened in place. */
async function removeConfirm(s: HTMLElement, id: string): Promise<HTMLElement> {
  const question = en('instance-profile-models-remove-question', id)
  press(removeMinus(s, id))
  return within(s).findByRole('group', { name: question })
}

test('R6, 26.8: the entry row\u2019s control is the minus of DESIGN.md 5.2 and not the trash can: the table draws no can path, the model profile staying is what the row says', async () => {
  const { main } = await loaded()
  const s = await modelsSection(main)
  for (const id of ['Qwen3-27B', 'gemma-3-12b-it']) {
    const minus = removeMinus(s, id)
    expect(
      paths(minus),
      `${id}: the row control draws the horizontal stroke alone`,
    ).toEqual(MINUS)
  }
  expect(
    paths(modelsTable(s)),
    'R6: no trash can anywhere in the table (the can is the list delete, which deletes the profile itself)',
  ).not.toEqual(expect.arrayContaining(TRASH))
  for (const d of TRASH) {
    expect(
      paths(modelsTable(s)).includes(d),
      `R6: the can's path "${d}" is not drawn here`,
    ).toBe(false)
  }
})

test('26.8: the row minus is able to act on a profile an instance runs and opens the Confirm in place named by instance-profile-models-remove-question, with the red verb instance-profile-models-remove and Cancel; Cancel closes it, leaves the row, sends nothing and puts the focus back on the minus', async () => {
  const { bff, main } = await loaded()
  const s = await modelsSection(main)
  const minus = removeMinus(s, 'Qwen3-27B')
  expect(
    inactive(minus),
    'a profile an instance runs: the minus still acts (26.8)',
  ).toBe(false)
  const group = await removeConfirm(s, 'Qwen3-27B')
  const verb = within(group).getByRole('button', {
    name: en('instance-profile-models-remove'),
  })
  expect(
    verb.className.split(/\s+/),
    'the verb carries the confirm danger look (CD)',
  ).toContain('danger')
  const cancel = within(group).getByRole('button', {
    name: en('confirm-cancel'),
  })
  expect(document.activeElement, 'the focus is on Cancel (22 D4.4)').toBe(
    cancel,
  )
  press(cancel)
  await flushTasks()
  expect(
    s.querySelector('.confirm-group'),
    'Cancel closes the confirm',
  ).toBeNull()
  expect(
    modelRows(s).map((r) => text(r.querySelector('a'))),
    'the row stays',
  ).toEqual(['Qwen3-27B', 'gemma-3-12b-it'])
  await quiet(bff)
  expect(sentRemove(bff), 'Cancel sends nothing').toEqual([])
  expect(readCount(bff), 'Cancel reads nothing new').toBe(1)
  expect(document.activeElement, 'the focus is back on the minus').toBe(minus)
})

test('26.8: the confirm verb sends DELETE .../models/<id> once, the row goes, the focus moves to the next row link, the status reads instance-profile-model-removed, and a running list leaves its offer', async () => {
  const { bff, main } = await loaded({
    instanceProfile: readsInOrder([
      doc(),
      doc({ models: [{ id: 'gemma-3-12b-it', keys: 3 }] }),
    ]),
    instanceProfileModelRemove: () =>
      reply(200, { running: ['default'], notConverged: [] }),
  })
  const s = await modelsSection(main)
  const group = await removeConfirm(s, 'Qwen3-27B')
  press(
    within(group).getByRole('button', {
      name: en('instance-profile-models-remove'),
    }),
  )
  await waitFor(() =>
    expect(sentRemove(bff), 'one delete of the encoded id').toEqual([
      modelPath('Qwen3-27B'),
    ]),
  )
  await flushTasks(8)
  expect(sentRemove(bff), 'one delete').toHaveLength(1)
  expect(
    bff.calls
      .filter((c) => c.method === 'DELETE' && c.path === modelPath('Qwen3-27B'))
      .map((c) => c.body),
    'the remove carries no body: no revision on this write either',
  ).toEqual([undefined])
  expect(
    modelRows(s).map((r) => text(r.querySelector('a'))),
    'the row is gone',
  ).toEqual(['gemma-3-12b-it'])
  const next = within(modelRow(s, 'gemma-3-12b-it')).getByRole('link', {
    name: 'gemma-3-12b-it',
  })
  expect(document.activeElement, 'the focus is on the next row link').toBe(next)
  const sentence = en('instance-profile-model-removed', 'Qwen3-27B')
  expect(text(statusOf(s, sentence)), sentence).toBe(sentence)
  expect(
    offerButtons(main).map((b) => text(b)),
    'the remove answers running: the offer',
  ).toEqual([en('restart-offer-button', 'default')])
  await quiet(bff)
  expect(
    readCount(bff),
    'the remove wrote: the profile is read again (section 3)',
  ).toBe(2)
})

test('26.8: removing the last row moves the focus to the add', async () => {
  const { bff, main } = await loaded({
    instanceProfile: readsInOrder([
      doc({ models: [{ id: 'Qwen3-27B', keys: 0 }] }),
      doc({ models: [] }),
    ]),
  })
  const s = await modelsSection(main)
  const group = await removeConfirm(s, 'Qwen3-27B')
  press(
    within(group).getByRole('button', {
      name: en('instance-profile-models-remove'),
    }),
  )
  await waitFor(() => expect(sentRemove(bff)).toHaveLength(1))
  await flushTasks(8)
  expect(
    s.querySelector('tbody tr'),
    'the table has no row now, and the empty section draws none',
  ).toBeNull()
  expect(
    document.activeElement,
    'the focus lands on the add (26.8: the + when it was the last)',
  ).toBe(addButton(s))
  await quiet(bff)
  expect(readCount(bff), 'the last row leaves the empty line').toBe(2)
})

test('26.8: a remove refused 404 no-instance-profile-model says section 3\u2019s words in the section alert, naming the model, and the row stays', async () => {
  const { bff, main } = await loaded({
    instanceProfileModelRemove: () =>
      reply(404, { key: 'no-instance-profile-model', args: ['Qwen3-27B'] }),
  })
  const s = await modelsSection(main)
  const group = await removeConfirm(s, 'Qwen3-27B')
  press(
    within(group).getByRole('button', {
      name: en('instance-profile-models-remove'),
    }),
  )
  await waitFor(() => expect(sentRemove(bff)).toHaveLength(1))
  await flushTasks(6)
  const sentence = en('api-no-instance-profile-model', 'Qwen3-27B')
  expect(text(alertOf(s, sentence)), sentence).toBe(sentence)
  expect(
    modelRows(s).map((r) => text(r.querySelector('a'))),
    'the row stays: the page does not guess',
  ).toEqual(['Qwen3-27B', 'gemma-3-12b-it'])
})

test('R10, 26.8: while the remove runs the confirm says instance-profile-model-removing, not confirm-deleting: the row asks to remove an entry, the model profile stays', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    instanceProfileModelRemove: () => gate.promise,
  })
  const s = await modelsSection(main)
  const group = await removeConfirm(s, 'Qwen3-27B')
  const verb = within(group).getByRole('button', {
    name: en('instance-profile-models-remove'),
  })
  press(verb)
  await waitFor(() => expect(sentRemove(bff)).toHaveLength(1))
  expect(
    text(within(group).getByRole('status')),
    'the status of a remove that is running',
  ).toBe(en('instance-profile-model-removing'))
  expect(
    text(group),
    'R10: the delete word is not the one a remove says',
  ).not.toContain(en('confirm-deleting'))
  gate.resolve(reply(200, { running: [], notConverged: [] }))
  await flushTasks(8)
})

test('26.8, the review: a remove that empties the list leaves the empty line and the add, and no table of heads alone', async () => {
  // The reviewer's third finding: the row leaving must leave the section as 26.7
  // draws an empty profile, the line that says so and the `+`, not a table with
  // its heads over nothing.
  const { bff, main } = await loaded({
    instanceProfile: readsInOrder([
      doc({ models: [{ id: 'Qwen3-27B', keys: 0 }] }),
      doc({ models: [] }),
    ]),
  })
  const s = await modelsSection(main)
  const group = await removeConfirm(s, 'Qwen3-27B')
  press(
    within(group).getByRole('button', {
      name: en('instance-profile-models-remove'),
    }),
  )
  await waitFor(() => expect(sentRemove(bff)).toHaveLength(1))
  await flushTasks(8)
  const empty = await within(s).findByText(en('instance-profile-models-empty'))
  expect(s.querySelector('table'), 'no entry left: no table').toBeNull()
  expect(
    before(empty, addButton(s)),
    'the empty line stands where the table was, the add after it',
  ).toBe(true)
  await quiet(bff)
  expect(readCount(bff), 'the remove wrote: the profile is read again').toBe(2)
})

test('26.8, the review: a remove refused 404 no-instance-profile-model reads the profile again, so the row the file no longer lists goes', async () => {
  // The reviewer's second finding, half one. The 404 says the entry is not on
  // the manager, so the row on screen is stale: the page reads the profile
  // again, and what the read answers is what the table shows (here the entry
  // gone, there the row kept because the file does list it).
  const { bff, main } = await loaded({
    instanceProfile: readsInOrder([
      doc(),
      doc({ models: [{ id: 'gemma-3-12b-it', keys: 3 }] }),
    ]),
    instanceProfileModelRemove: () =>
      reply(404, { key: 'no-instance-profile-model', args: ['Qwen3-27B'] }),
  })
  const s = await modelsSection(main)
  const reads = readCount(bff)
  const group = await removeConfirm(s, 'Qwen3-27B')
  press(
    within(group).getByRole('button', {
      name: en('instance-profile-models-remove'),
    }),
  )
  await waitFor(() => expect(sentRemove(bff)).toHaveLength(1))
  await flushTasks(8)
  const sentence = en('api-no-instance-profile-model', 'Qwen3-27B')
  expect(text(alertOf(s, sentence)), sentence).toBe(sentence)
  expect(
    modelRows(s).map((r) => text(r.querySelector('a'))),
    'the stale row is gone, the one the file lists stays',
  ).toEqual(['gemma-3-12b-it'])
  await quiet(bff)
  expect(readCount(bff), 'the refusal read the profile again').toBeGreaterThan(
    reads,
  )
})

test('26.8, 26.12: the remove offers one Restart per running instance, and its restart reads restart-offer-running and then restart-offer-sent', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    instanceProfileModelRemove: () =>
      reply(200, { running: ['default', 'coder'], notConverged: [] }),
    run: () => gate.promise,
  })
  const s = await modelsSection(main)
  const group = await removeConfirm(s, 'Qwen3-27B')
  press(
    within(group).getByRole('button', {
      name: en('instance-profile-models-remove'),
    }),
  )
  await waitFor(() => expect(sentRemove(bff)).toHaveLength(1))
  await flushTasks(8)
  const buttons = offerButtons(main)
  expect(
    buttons.map((b) => text(b)),
    'one button per running instance (26.12)',
  ).toEqual([
    en('restart-offer-button', 'default'),
    en('restart-offer-button', 'coder'),
  ])
  press(buttons[1]!)
  await waitFor(() =>
    expect(
      bff.to('POST', '/api/instances/coder/restart').length,
      'the restart of the instance pressed',
    ).toBe(1),
  )
  expect(
    text(statusOf(main, en('restart-offer-running', 'coder'))),
    'the offer says it is restarting',
  ).toBe(en('restart-offer-running', 'coder'))
  gate.resolve(reply(204))
  await flushTasks(6)
  expect(
    text(statusOf(main, en('restart-offer-sent', 'coder'))),
    'the answer',
  ).toBe(en('restart-offer-sent', 'coder'))
})

// ── The strings of the two sections ─────────────────────────────────────

test('26.10, 26.6, 26.8, 26.12: strings/en_US.json holds every key these tests read', () => {
  const table = strings as Record<string, string>
  const missing = KEYS.filter((key) => table[key] === undefined)
  expect(missing, 'keys the page reads that have no English').toEqual([])
})

test('R11: strings/en_US.json writes no key twice', () => {
  // The file is JSON, so a key written twice is no syntax error: the import
  // keeps the last one and the other is dead bytes, invisible to the import.
  // IP6b left `api-no-model-profile-named` written twice, with two sentences.
  // The census reads the file as it stands, one key per top-level line.
  const raw = readFileSync(
    join(import.meta.dirname, 'strings', 'en_US.json'),
    'utf8',
  )
  const written = [...raw.matchAll(/^ {2}"([^"]+)":/gm)].map((m) => m[1]!)
  const twice = [
    ...new Set(written.filter((key, n) => written.indexOf(key) !== n)),
  ]
  expect(
    twice,
    'a key written twice: the import reads one, the other is dead',
  ).toEqual([])
  expect(
    written.length,
    'the census sees every entry the import has (a file shape the census cannot read?)',
  ).toBe(Object.keys(strings as Record<string, string>).length)
})
