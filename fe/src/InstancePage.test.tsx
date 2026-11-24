// The instance page (fe/briefs/01-server.md, slice 1a, sections 2.4 and 2.5,
// with the form of section 5): its header, and the Server section: the form,
// which every signed-in session edits (the web UI is run by administrators
// only, "Administrators only"). The whole app renders against a fake bff
// (mocked fetch); "1a.25" is the brief's criterion 25. Brief 16 slice 17a
// moved that form to a server profile's page, so the tests of the form
// (#/profiles/server/default) live here too, and the instance page's own
// (17a.10) start at '#/instances/default'.
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
  isHidden,
  reply,
  serverDoc,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'
import { serverProfileHref } from './route.ts'

beforeEach(() => startAt('#/profiles/server/default'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

/** The labels of the fields, in the order of section 5.1, card by card. */
const CARDS: [string, string[]][] = [
  ['server-network', ['Port', 'Bind to', 'Base URL', 'API key']],
  [
    'server-resources',
    [
      'CPU threads',
      'Batch threads',
      'Max models',
      'Memory margin',
      'Models folder',
    ],
  ],
  ['server-advanced', ['Web UI', 'Web UI MCP proxy', 'Log level']],
  ['server-snapshots', ['Save on shutdown', 'Snapshot folder']],
  [
    'server-sdk',
    [
      'hipBLASLt',
      'Profiler attach',
      'Tensor-split AllReduce',
      'Disable GPU graphs',
      'KV rotation',
    ],
  ],
]

const FIELDS = CARDS.flatMap(([, fields]) => fields)

/** The page with the server form loaded; returns main. */
async function loaded(options: BffOptions = {}): Promise<HTMLElement> {
  fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return main
}

/** The instance page (17a.10: no Server form), with its run controls shown. */
async function loadedInstance(
  options: BffOptions = {},
  hash = '#/instances/default',
): Promise<HTMLElement> {
  startAt(hash)
  fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('button', { name: en('run-stop') })
  return main
}

function about(label: string): string {
  return `${en('field-about')} ${label}`
}

/**
 * The accessible names of the About toggles in `container`, in DOM order,
 * for the names of FIELDS (each name found by role, so a toggle with
 * another name shows as missing).
 */
function toggles(container: HTMLElement): string[] {
  const all = within(container).queryAllByRole('button', { name: /^About / })
  const named = FIELDS.map(about).flatMap((name) =>
    within(container)
      .queryAllByRole('button', { name })
      .map((el) => ({ name, el })),
  )
  expect(named.length, 'every About toggle has a field name').toBe(all.length)
  return named.sort((a, b) => (before(a.el, b.el) ? -1 : 1)).map((n) => n.name)
}

function group(main: HTMLElement, name: string) {
  return within(within(main).getByRole('group', { name }))
}

/** The Server section: the element holding its h2 (the header's Start at boot is outside). */
function serverSection(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
  return h2.parentElement!
}

function selected(el: HTMLElement): string | undefined {
  return (el as HTMLSelectElement).selectedOptions[0]?.textContent ?? undefined
}

function options(el: HTMLElement): string[] {
  return [...(el as HTMLSelectElement).options].map((o) => o.textContent ?? '')
}

function checked(el: HTMLElement): boolean {
  return (el as HTMLInputElement).checked
}

function value(el: HTMLElement): string {
  return (el as HTMLInputElement).value
}

function apiKey(main: HTMLElement): HTMLInputElement {
  return within(main).getByLabelText(en('server-api-key'), {
    selector: 'input',
  }) as HTMLInputElement
}

// ── Failure paths first ─────────────────────────────────────────────────

test('1a.21: an id the instances list lacks shows instance-not-found and the overview link', async () => {
  startAt('#/instances/ghost')
  fakeBff()
  render(<App />)
  const main = screen.getByRole('main')
  expect(
    await within(main).findByText(en('instance-not-found', 'ghost')),
  ).toBeTruthy()
  const home = within(main).getByRole('link', { name: en('not-found-home') })
  expect(home.getAttribute('href')).toBe('#/')
  expect(
    within(main).queryByRole('heading', { level: 2, name: en('server-title') }),
  ).toBeNull()
})

test('1a.20 (2.6): while /api/instances fails, the page shows the id, the Overview message and no Server section', async () => {
  for (const [what, answer, key] of [
    [
      '503',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      'instances-manager-unavailable',
    ],
    [
      '502',
      () => reply(502, { key: 'client-not-authorized', args: [] }),
      'instances-not-authorized',
    ],
    [
      'network',
      () => Promise.reject(new TypeError('Failed to fetch')),
      'instances-failed',
    ],
  ] as const) {
    startAt('#/instances/default')
    const bff = fakeBff({ instances: answer })
    render(<App />)
    const main = screen.getByRole('main')
    expect(await within(main).findByText(en(key)), what).toBeTruthy()
    expect(
      within(main).getByRole('heading', { level: 1, name: 'default' }),
      what,
    ).toBeTruthy()
    for (const key of ['server-title', 'instance-server-title'])
      expect(
        within(main).queryByRole('heading', { level: 2, name: en(key) }),
        `${what}: ${key}`,
      ).toBeNull()
    expect(bff.to('GET', '/api/instances/default/server'), what).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1a.20 (2.6, 17a.10): while the instances list loads, the h1 shows at once with a loading status, and the answer brings no Server form', async () => {
  startAt('#/instances/default')
  // The nav reads the list too: each read gets its own Response, since a
  // body can be read only once.
  const gate = deferred()
  fakeBff({
    instances: () =>
      gate.promise.then(() =>
        reply(200, [{ id: 'default', engine: 'llama.cpp', state: 'active' }]),
      ),
  })
  render(<App />)
  const main = screen.getByRole('main')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'default',
  })
  const status = within(main).getByRole('status')
  expect(status.tagName).toBe('P')
  expect(status.textContent).toBe(en('loading'))
  expect(before(h1, status)).toBe(true)
  expect(within(main).queryByText(en('instance-engine'))).toBeNull()
  expect(
    within(main).queryByRole('heading', { level: 2, name: en('server-title') }),
  ).toBeNull()
  await act(async () => {
    gate.resolve(reply(204))
  })
  await within(main).findByRole('button', { name: en('run-stop') })
  expect(within(main).queryByText(en('loading'))).toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 3,
      name: en('server-network'),
    }),
    '17a.10: the form is on the profile page',
  ).toBeNull()
})

test('1a.24: while the server read runs, a status paragraph says server-loading', async () => {
  const pending = deferred()
  fakeBff({ serverProfile: () => pending.promise })
  render(<App />)
  const main = screen.getByRole('main')
  const status = await within(main).findByText(en('server-loading'))
  expect(status.tagName).toBe('P')
  expect(status.getAttribute('role')).toBe('status')
  await act(async () => {
    pending.resolve(reply(200, serverDoc()))
  })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  expect(within(main).queryByText(en('server-loading'))).toBeNull()
})

test('1a.24: a 401 on the server read shows the sign-in form in place of the form', async () => {
  let reads = 0
  const bff = fakeBff({
    serverProfile: () =>
      ++reads === 1
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, serverDoc()),
  })
  render(<App />)
  const main = screen.getByRole('main')
  const username = (await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })) as HTMLInputElement
  const password = within(main).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  expect(
    within(main).queryByRole('heading', {
      level: 3,
      name: en('server-network'),
    }),
  ).toBeNull()
  // As on any protected page (2.2): the route stays, the form says why.
  expect(window.location.hash).toBe('#/profiles/server/default')
  // 09 S1b.3: the read answered 401 while the app was signed in, so the notice
  // says the session ended (it is `signin-required` on a first load signed out).
  expect(within(main).getByText(en('signin-session-ended'))).toBeTruthy()
  // The whole app is signed out (2.6), and signed out the shell offers
  // nothing: no Sign in in the banner, no nav (09 S1a.4).
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
      within(banner).queryByRole('link', { name: en('session-sign-in') }),
    '09 S1a.4: no Sign in in the banner signed out',
  ).toBeNull()
  expect(
    screen.queryByRole('navigation', { name: en('nav-label') }),
    '09 S1a.4: no nav signed out',
  ).toBeNull()
  // After sign-in the page comes back, with the server file read again.
  fireEvent.change(username, { target: { value: 'alice' } })
  fireEvent.change(password, { target: { value: 'hunter2' } })
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  expect(window.location.hash).toBe('#/profiles/server/default')
  expect(bff.to('GET', '/api/profiles/server/default')).toHaveLength(2)
})

test('1a.24: 403 and 503 show their api- message in place of the form', async () => {
  for (const [status, key, english] of [
    [403, 'client-not-authorized', en('api-client-not-authorized')],
    [503, 'client-unavailable', en('api-client-unavailable')],
  ] as const) {
    fakeBff({ serverProfile: () => reply(status, { key, args: [] }) })
    render(<App />)
    const main = screen.getByRole('main')
    expect(await within(main).findByText(english), key).toBeTruthy()
    expect(within(serverSection(main)).queryByRole('group'), key).toBeNull()
    // In place of the form: the page and its Server section stay.
    expect(
      within(main).getByRole('heading', { level: 1, name: 'default' }),
      key,
    ).toBeTruthy()
    expect(
      within(main).getByRole('heading', { level: 2, name: en('server-title') }),
      key,
    ).toBeTruthy()
    expect(within(main).queryByText(en('server-load-failed')), key).toBeNull()
    cleanup()
    vi.unstubAllGlobals()
  }
})

const SERVER_LOAD_FAILURES: [string, () => Response | Promise<Response>][] = [
  ['network', () => Promise.reject(new TypeError('Failed to fetch'))],
  ['500', () => reply(500, { key: 'secret-detail', args: [] })],
  ['502 not json', () => reply(502, 'secret-detail')],
  ['200 not json', () => reply(200, 'secret-detail')],
  ['200 {}', () => reply(200, {})],
  ['200 []', () => reply(200, [])],
  ['200 null', () => reply(200, 'null')],
]

test.each(SERVER_LOAD_FAILURES)(
  '1a.24 (17a.6): anything else, malformed included, shows server-load-failed, never the body: %s',
  async (what, serverProfile) => {
    // 17a moved the server document: the read that fails is the profile page's.
    fakeBff({ serverProfile })
    render(<App />)
    const main = screen.getByRole('main')
    expect(
      await within(main).findByText(en('server-load-failed')),
      what,
    ).toBeTruthy()
    expect(document.body.textContent, what).not.toContain('secret-detail')
    expect(within(serverSection(main)).queryByRole('group'), what).toBeNull()
    expect(
      within(main).getByRole('heading', { level: 2, name: en('server-title') }),
      what,
    ).toBeTruthy()
  },
)

// ── The header ──────────────────────────────────────────────────────────

test('1a.20: the header shows the id as h1, the engine, then the state with its shape', async () => {
  // The header line is the instance page's (17a.10).
  startAt('#/instances/default')
  fakeBff()
  render(<App />)
  const main = screen.getByRole('main')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'default',
  })
  const engine = await within(main).findByText(
    en('instance-engine', 'llama.cpp'),
  )
  const state = await within(main).findByText(
    en('instance-state', en('instance-state-active')),
  )
  expect(before(h1, engine)).toBe(true)
  expect(before(engine, state)).toBe(true)
  // The state badge's shape is hidden from assistive technology.
  const shape =
    state.querySelector('[aria-hidden="true"]') ??
    state.parentElement!.querySelector('[aria-hidden="true"]')
  expect(shape, 'an aria-hidden shape beside the state').not.toBeNull()
  expect(shape!.textContent ?? '').toBe('')
})

test('1a.20, 1c.41b (17a.10): the session is read once on open, no server file at all, and only the instances are polled', async () => {
  // Fake timers from the start, so an interval set at mount is caught too;
  // shouldAdvanceTime keeps the findBy* waits running.
  vi.useFakeTimers({ shouldAdvanceTime: true })
  startAt('#/instances/default')
  try {
    const bff = fakeBff()
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('button', { name: en('run-stop') })
    expect(bff.to('GET', '/api/instances').length).toBeGreaterThan(0)
    await act(async () => {
      await vi.advanceTimersByTimeAsync(65_000)
    })
    // The instances are polled from 1c (RunControls.test.tsx); nothing else is.
    expect(
      bff.to('GET', '/api/instances/default/server'),
      '17a.10: the page reads no server file',
    ).toHaveLength(0)
    expect(bff.to('GET', '/api/session')).toHaveLength(1)
  } finally {
    vi.useRealTimers()
  }
})

test('1a.20: an unknown state is shown by its name in the header', async () => {
  startAt('#/instances/odd')
  fakeBff({
    instances: [{ id: 'odd', engine: 'llama.cpp', state: 'maintenance' }],
  })
  render(<App />)
  const main = screen.getByRole('main')
  expect(
    await within(main).findByText(
      en('instance-state', en('instance-state-other', 'maintenance')),
    ),
  ).toBeTruthy()
})

test('1a.22 (17a.10): no tab list; the Server profile section follows the header', async () => {
  const main = await loadedInstance()
  expect(within(main).queryByRole('tablist')).toBeNull()
  const headings = within(main)
    .getAllByRole('heading')
    .map((h) => `${h.tagName}:${h.textContent}`)
  expect(headings.slice(0, 2)).toEqual([
    'H1:default',
    `H2:${en('instance-server-title')}`,
  ])
})

// ── The Server section ──────────────────────────────────────────────────

test('1a.23: the section has its h2 and the intro naming server@<id>.ini', async () => {
  const main = await loaded()
  expect(
    within(main).getByRole('heading', { level: 2, name: en('server-title') }),
  ).toBeTruthy()
  expect(
    within(main).getByText(en('server-profile-intro', 'server@default.ini')),
  ).toBeTruthy()
})

test('1a.25: the cards and their fields come in the order of section 5.1', async () => {
  const main = await loaded()
  const cards = within(main).getAllByRole('heading', { level: 3 })
  expect(cards.map((h) => h.textContent)).toEqual(CARDS.map(([key]) => en(key)))
  for (const [key, fields] of CARDS) {
    const h3 = within(main).getByRole('heading', { level: 3, name: en(key) })
    const card = h3.closest('section')
    expect(card, `${key} is a section`).not.toBeNull()
    expect(toggles(card!), key).toEqual(fields.map(about))
  }
  expect(
    within(main)
      .getAllByRole('heading', { level: 4 })
      .map((h) => h.textContent),
  ).toEqual(['CUDA', 'ROCm', en('server-cuda-and-rocm'), 'llama.cpp'])
  const sdk = within(main).getByRole('heading', {
    level: 3,
    name: en('server-sdk'),
  })
  expect(
    within(sdk.closest('section')!).getByText(en('server-sdk-intro')),
  ).toBeTruthy()
})

// The read-only form and its Notice are gone with the non-administrator role
// (brief 01, "Administrators only", criterion 25 and 28): 1a.25 above shows
// the form for every session, and 1b.28 in ServerSave.test.tsx asserts the
// enabled controls and the save bar for both.

test('1a.25 (administrators only): every signed-in session gets the form with no read-only Notice', async () => {
  const main = await loaded()
  const section = serverSection(main)
  // 25: the Notice server-read-only is gone, so nothing of its own stands
  // between the intro and the first card.
  expect(section.querySelector('.notice')).toBeNull()
  // 28: the form is the editable one: a box that is clear leaves its
  // textbox enabled (5.3).
  const port = within(main).getByRole('textbox', { name: 'Port' })
  expect(port.getAttribute('disabled'), 'Port is editable').toBeNull()
})

test('1a.25: each control shows the value of the file (Network and Resources)', async () => {
  const main = await loaded()
  const port = group(main, 'Port')
  expect(
    checked(port.getByRole('checkbox', { name: en('field-default') })),
  ).toBe(false)
  expect(value(port.getByRole('textbox', { name: 'Port' }))).toBe('8580')

  const url = group(main, 'Base URL')
  expect(
    checked(url.getByRole('checkbox', { name: en('field-default') })),
  ).toBe(true)
  expect(value(url.getByRole('textbox', { name: 'Base URL' }))).toBe(
    'http://127.0.0.1:8580/v1',
  )

  const key = group(main, 'API key')
  expect(checked(key.getByRole('checkbox', { name: en('field-no-key') }))).toBe(
    false,
  )
  expect(apiKey(main).type).toBe('password')
  expect(apiKey(main).value).toBe('sk-local-123')

  const threads = group(main, 'CPU threads')
  expect(
    checked(threads.getByRole('checkbox', { name: en('field-auto') })),
  ).toBe(true)
  expect(value(threads.getByRole('textbox', { name: 'CPU threads' }))).toBe('')
  const batch = group(main, 'Batch threads')
  expect(checked(batch.getByRole('checkbox', { name: en('field-auto') }))).toBe(
    false,
  )
  expect(value(batch.getByRole('textbox', { name: 'Batch threads' }))).toBe(
    '16',
  )

  const max = group(main, 'Max models')
  expect(
    checked(max.getByRole('checkbox', { name: en('field-default') })),
  ).toBe(true)
  expect(value(max.getByRole('textbox', { name: 'Max models' }))).toBe('4')
  const margin = group(main, 'Memory margin')
  expect(
    checked(margin.getByRole('checkbox', { name: en('field-default') })),
  ).toBe(false)
  expect(value(margin.getByRole('textbox', { name: 'Memory margin' }))).toBe(
    '1024',
  )
  // The unit after the textbox, shown (the hint also says "in MiB", so the
  // group's text alone would not prove it).
  const unit = margin.getByText('MiB')
  expect(isHidden(unit)).toBe(false)
  expect(
    before(margin.getByRole('textbox', { name: 'Memory margin' }), unit),
  ).toBe(true)

  const models = within(main).getByRole('textbox', { name: 'Models folder' })
  expect(value(models)).toBe('')
  expect(models.getAttribute('placeholder')).toBe(
    en('server-folder-instance-default', '/var/lib/llama.cpp/models'),
  )
})

test('1a.25: each control shows the value of the file (Advanced, snapshots, SDK tuning)', async () => {
  const main = await loaded()
  expect(
    checked(
      group(main, 'Web UI').getByRole('checkbox', {
        name: en('server-web-ui-toggle'),
      }),
    ),
  ).toBe(true)
  expect(
    checked(
      group(main, 'Web UI MCP proxy').getByRole('checkbox', {
        name: en('server-mcp-proxy-toggle'),
      }),
    ),
  ).toBe(false)
  const level = within(main).getByRole('combobox', { name: 'Log level' })
  expect(options(level)).toEqual([
    'OUTPUT:0',
    'ERROR:1',
    'WARN:2',
    'INFO:3',
    'TRACE:4',
    'DEBUG:5',
  ])
  expect(selected(level)).toBe('INFO:3')
  expect(
    checked(
      group(main, 'Save on shutdown').getByRole('checkbox', {
        name: en('server-save-on-shutdown-toggle'),
      }),
    ),
  ).toBe(true)
  expect(
    value(within(main).getByRole('textbox', { name: 'Snapshot folder' })),
  ).toBe('/data/llama-state')
  const hip = within(main).getByRole('combobox', { name: 'hipBLASLt' })
  expect(options(hip)).toEqual([
    en('option-default'),
    en('option-on'),
    en('option-off'),
  ])
  expect(selected(hip)).toBe(en('option-off'))
  expect(
    checked(
      group(main, 'Profiler attach').getByRole('checkbox', {
        name: en('server-profiler-attach-toggle'),
      }),
    ),
  ).toBe(false)
  const allreduce = within(main).getByRole('combobox', {
    name: 'Tensor-split AllReduce',
  })
  expect(options(allreduce)).toEqual([
    en('option-default'),
    'nccl',
    en('option-internal'),
    en('option-none'),
  ])
  expect(selected(allreduce)).toBe('nccl')
  expect(
    checked(
      group(main, 'Disable GPU graphs').getByRole('checkbox', {
        name: en('server-gpu-graphs-toggle'),
      }),
    ),
  ).toBe(false)
  expect(
    checked(
      group(main, 'KV rotation').getByRole('checkbox', {
        name: en('server-kv-rotation-toggle'),
      }),
    ),
  ).toBe(false)
})

test('1a.25: unset values show their defaults, other values their own branch', async () => {
  const main = await loaded({
    serverProfile: () =>
      reply(
        200,
        serverDoc({
          Port: null,
          Hostname: null,
          OpencodeApiKey: '',
          Threads: 8,
          ModelsMemoryMargin: null,
          ModelsDir: '/srv/models',
          Webui: false,
          LogVerbosity: null,
          SaveStateOnShutdown: null,
          StateDir: null,
          RocblasUseHipblaslt: null,
          RocprofAttach: true,
          CudaAllreduce: 'bogus',
          CudaDisableGraphs: true,
          AttnRotDisable: true,
        }),
      ),
  })
  const port = group(main, 'Port')
  expect(
    checked(port.getByRole('checkbox', { name: en('field-default') })),
  ).toBe(true)
  expect(value(port.getByRole('textbox', { name: 'Port' }))).toBe('9931')
  const bind = group(main, 'Bind to')
  expect(bind.getAllByRole('checkbox').map(checked)).toEqual([
    true,
    false,
    false,
  ])
  const key = group(main, 'API key')
  expect(checked(key.getByRole('checkbox', { name: en('field-no-key') }))).toBe(
    true,
  )
  expect(apiKey(main).value).toBe('')
  const threads = group(main, 'CPU threads')
  expect(
    checked(threads.getByRole('checkbox', { name: en('field-auto') })),
  ).toBe(false)
  expect(value(threads.getByRole('textbox', { name: 'CPU threads' }))).toBe('8')
  const margin = group(main, 'Memory margin')
  expect(
    checked(margin.getByRole('checkbox', { name: en('field-default') })),
  ).toBe(true)
  expect(value(margin.getByRole('textbox', { name: 'Memory margin' }))).toBe(
    '0',
  )
  expect(
    value(within(main).getByRole('textbox', { name: 'Models folder' })),
  ).toBe('/srv/models')
  expect(
    checked(
      group(main, 'Web UI').getByRole('checkbox', {
        name: en('server-web-ui-toggle'),
      }),
    ),
  ).toBe(false)
  expect(
    selected(within(main).getByRole('combobox', { name: 'Log level' })),
  ).toBe('TRACE:4')
  expect(
    checked(
      group(main, 'Save on shutdown').getByRole('checkbox', {
        name: en('server-save-on-shutdown-toggle'),
      }),
    ),
  ).toBe(false)
  const state = within(main).getByRole('textbox', { name: 'Snapshot folder' })
  expect(value(state)).toBe('')
  expect(state.getAttribute('placeholder')).toBe(
    en('server-folder-instance-default', '/var/lib/llama.cpp/state'),
  )
  expect(
    selected(within(main).getByRole('combobox', { name: 'hipBLASLt' })),
  ).toBe(en('option-default'))
  expect(
    checked(
      group(main, 'Profiler attach').getByRole('checkbox', {
        name: en('server-profiler-attach-toggle'),
      }),
    ),
  ).toBe(true)
  expect(
    selected(
      within(main).getByRole('combobox', { name: 'Tensor-split AllReduce' }),
    ),
  ).toBe(en('option-default'))
  expect(
    checked(
      group(main, 'Disable GPU graphs').getByRole('checkbox', {
        name: en('server-gpu-graphs-toggle'),
      }),
    ),
  ).toBe(true)
  expect(
    checked(
      group(main, 'KV rotation').getByRole('checkbox', {
        name: en('server-kv-rotation-toggle'),
      }),
    ),
  ).toBe(true)
})

/**
 * The switches as the shipped server@default.ini leaves them: no line for
 * any of them, so every one is null in the document.
 */
const SWITCHES_UNSET = {
  Webui: null,
  WebuiMcpProxy: null,
  SaveStateOnShutdown: null,
  RocprofAttach: null,
  CudaDisableGraphs: null,
  AttnRotDisable: null,
}

/** The switch fields with their checkbox names, in form order. */
const SWITCHES: [string, string][] = [
  ['Web UI', 'server-web-ui-toggle'],
  ['Web UI MCP proxy', 'server-mcp-proxy-toggle'],
  ['Save on shutdown', 'server-save-on-shutdown-toggle'],
  ['Profiler attach', 'server-profiler-attach-toggle'],
  ['Disable GPU graphs', 'server-gpu-graphs-toggle'],
  ['KV rotation', 'server-kv-rotation-toggle'],
]

function switches(main: HTMLElement): Record<string, boolean> {
  return Object.fromEntries(
    SWITCHES.map(([field, key]) => [
      field,
      checked(group(main, field).getByRole('checkbox', { name: en(key) })),
    ]),
  )
}

test('5.2: an unset Web UI shows ticked (the default), every other unset switch unticked', async () => {
  const main = await loaded({
    serverProfile: () => reply(200, serverDoc(SWITCHES_UNSET)),
  })
  expect(switches(main)).toEqual({
    'Web UI': true,
    'Web UI MCP proxy': false,
    'Save on shutdown': false,
    'Profiler attach': false,
    'Disable GPU graphs': false,
    'KV rotation': false,
  })
})

test('5.2: a Web UI set to false shows unticked', async () => {
  const main = await loaded({
    serverProfile: () =>
      reply(200, serverDoc({ ...SWITCHES_UNSET, Webui: false })),
  })
  expect(switches(main)['Web UI']).toBe(false)
})

test('1a.25: a set Base URL and Max models show their value, an unset API key No key', async () => {
  const main = await loaded({
    serverProfile: () =>
      reply(
        200,
        serverDoc({
          OpencodeBaseUrl: 'https://gw.example.net/v1',
          OpencodeApiKey: null,
          ModelsMax: 2,
          Hostname: '',
        }),
      ),
  })
  const url = group(main, 'Base URL')
  expect(
    checked(url.getByRole('checkbox', { name: en('field-default') })),
  ).toBe(false)
  expect(value(url.getByRole('textbox', { name: 'Base URL' }))).toBe(
    'https://gw.example.net/v1',
  )
  const key = group(main, 'API key')
  expect(checked(key.getByRole('checkbox', { name: en('field-no-key') }))).toBe(
    true,
  )
  expect(apiKey(main).value).toBe('')
  const max = group(main, 'Max models')
  expect(
    checked(max.getByRole('checkbox', { name: en('field-default') })),
  ).toBe(false)
  expect(value(max.getByRole('textbox', { name: 'Max models' }))).toBe('2')
  // An empty Hostname checks localhost, as null does.
  expect(group(main, 'Bind to').getAllByRole('checkbox').map(checked)).toEqual([
    true,
    false,
    false,
  ])
})

test('1a.25: Bind to lists localhost, 0.0.0.0, the values no longer present, then the interfaces', async () => {
  const main = await loaded()
  const bind = group(main, 'Bind to')
  const boxes = bind.getAllByRole('checkbox')
  const rows = [
    en('server-bind-localhost'),
    en('server-bind-all'),
    en('server-bind-gone', '10.0.0.9'),
    'enp5s0 (192.168.1.20)',
  ].map((name) => bind.queryByRole('checkbox', { name }))
  expect(boxes).toHaveLength(rows.length)
  rows.forEach((row, i) => expect(row, `row ${i}`).toBe(boxes[i]))
  expect(boxes.map(checked)).toEqual([true, false, true, true])
})

test('1a.25: the log level is clamped to 0..5 and Bind to matches without case', async () => {
  for (const [level, shown] of [
    [9, 'DEBUG:5'],
    [-2, 'OUTPUT:0'],
  ] as const) {
    const main = await loaded({
      serverProfile: () =>
        reply(200, serverDoc({ LogVerbosity: level, Hostname: 'LocalHost' })),
    })
    expect(
      selected(within(main).getByRole('combobox', { name: 'Log level' })),
      String(level),
    ).toBe(shown)
    const boxes = group(main, 'Bind to').getAllByRole('checkbox')
    expect(boxes.map(checked), 'no gone row for LocalHost').toEqual([
      true,
      false,
      false,
    ])
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1a.26: every field has its About toggle, enabled, in field order', async () => {
  const main = await loaded()
  // The Server section's: Start at boot's toggle sits in the header (1c.45).
  expect(toggles(serverSection(main))).toEqual(FIELDS.map(about))
  for (const b of within(main).getAllByRole('button', { name: /^About / })) {
    expect((b as HTMLButtonElement).disabled, b.textContent ?? '').toBe(false)
    expect(b.getAttribute('aria-expanded'), b.textContent ?? '').toBe('false')
    // A native button: Enter and Space press it as a click.
    expect(b.tagName).toBe('BUTTON')
    expect(b.getAttribute('type')).toBe('button')
  }
})

test('1a.26: the About toggle reveals the hint, one paragraph per line, and hides it again', async () => {
  const main = await loaded()
  const toggle = within(main).getByRole('button', {
    name: about('CPU threads'),
  })
  const lines = en('server-cpu-threads-hint').split('\n')
  expect(lines).toHaveLength(3)
  expect(isHidden(within(main).queryByText(lines[0]!))).toBe(true)
  fireEvent.click(toggle)
  expect(toggle.getAttribute('aria-expanded')).toBe('true')
  const hint = document.getElementById(
    toggle.getAttribute('aria-controls') ?? '',
  )
  expect(hint, 'aria-controls names the hint').not.toBeNull()
  expect(isHidden(hint)).toBe(false)
  expect([...hint!.querySelectorAll('p')].map((p) => p.textContent)).toEqual(
    lines,
  )
  fireEvent.click(toggle)
  expect(toggle.getAttribute('aria-expanded')).toBe('false')
  expect(isHidden(within(main).queryByText(lines[0]!))).toBe(true)
})

// Show's dependencies for the editable form are 1b's (5.3): ServerSave.test.tsx.
// The API key (2.8, criteria 27a to 27e): one role, so `withheld` is handled
// the same for every session (27a picks 27d, never a view of its own).

/** A document whose key the bff withheld (the contract's shape). */
function withheldDoc(
  key: unknown = null,
  withheld: unknown = ['OpencodeApiKey'],
) {
  return serverDoc({ OpencodeApiKey: key }, { withheld })
}

function noKey(main: HTMLElement): HTMLInputElement {
  return group(main, 'API key').getByRole('checkbox', {
    name: en('field-no-key'),
  }) as HTMLInputElement
}

function apiKeyGroup(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: en('server-api-key') })
}

function show(main: HTMLElement): HTMLButtonElement {
  return group(main, 'API key').getByRole('button', {
    name: en('field-show'),
  }) as HTMLButtonElement
}

/** The paragraph of the removed withheld view (27b), if the page holds one. */
function withheldLine(main: HTMLElement): Element | null {
  return apiKeyGroup(main).querySelector('[id$="-withheld"]')
}

test('1a.27 (2.8, administrators only): every session gets the key masked in an editable box, Show reveals it', async () => {
  const main = await loaded()
  const input = apiKey(main)
  expect(input.type).toBe('password')
  expect(input.value).toBe('sk-local-123')
  // 5.3: No key is clear, so its box and Show are enabled (28).
  expect(input.disabled, 'the key box is editable').toBe(false)
  expect(noKey(main).disabled, 'No key is editable').toBe(false)
  const button = show(main)
  expect(button.disabled, 'Show is enabled').toBe(false)
  expect(button.getAttribute('aria-pressed')).toBe('false')
  fireEvent.click(button)
  expect(show(main).getAttribute('aria-pressed')).toBe('true')
  expect(apiKey(main).type).toBe('text')
  fireEvent.click(show(main))
  expect(show(main).getAttribute('aria-pressed')).toBe('false')
  expect(apiKey(main).type).toBe('password')
  expect(withheldLine(main)).toBeNull()
})

test('1a.27d (2.8): a session whose document withholds the key gets server-load-failed, no form, no save bar', async () => {
  for (const [what, doc] of [
    ['withheld, key null', withheldDoc(null)],
    ['withheld, key sent', withheldDoc('sk-local-123')],
    [
      'withheld among other names',
      withheldDoc(null, ['Other', 'OpencodeApiKey']),
    ],
  ] as const) {
    fakeBff({ serverProfile: () => reply(200, doc) })
    render(<App />)
    const main = screen.getByRole('main')
    expect(
      await within(main).findByText(en('server-load-failed')),
      what,
    ).toBeTruthy()
    expect(within(serverSection(main)).queryByRole('group'), what).toBeNull()
    expect(
      within(main).queryByRole('button', { name: en('save-button') }),
      what,
    ).toBeNull()
    expect(document.body.innerHTML, what).not.toContain('sk-local-123')
    expect(
      within(main).getByRole('heading', {
        level: 2,
        name: en('server-title'),
      }),
      what,
    ).toBeTruthy()
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1a.27a (2.8): a withheld that does not name the key picks the form of 5.2, for every session', async () => {
  for (const withheld of [[], ['Other'], null]) {
    const main = await loaded({
      serverProfile: () => reply(200, serverDoc({}, { withheld })),
    })
    const what = JSON.stringify(withheld)
    expect(noKey(main).checked, what).toBe(false)
    expect(apiKey(main).value, what).toBe('sk-local-123')
    expect(apiKey(main).disabled, `${what}: editable`).toBe(false)
    expect(within(main).queryByText(en('server-load-failed')), what).toBeNull()
    expect(withheldLine(main), what).toBeNull()
    cleanup()
    vi.unstubAllGlobals()
  }
})

// 27e with 27d: a withheld document fails the read, so the form (and with it F4)
// is not rendered at all, for every session.
test('1a.27e (2.8): a withheld key is rendered nowhere, even when the bff sends it anyway', async () => {
  fakeBff({
    serverProfile: () => reply(200, withheldDoc('sk-test')),
  })
  render(<App />)
  const main = screen.getByRole('main')
  expect(await within(main).findByText(en('server-load-failed'))).toBeTruthy()
  const html = document.documentElement.outerHTML
  expect(html).not.toContain('sk-test')
  expect(html).not.toContain('•')
  expect(
    within(main).queryByRole('group', { name: en('server-api-key') }),
  ).toBeNull()
  expect(document.querySelector('[id$="-withheld"]')).toBeNull()
  // Nothing holds the key: no F4 group, no input that could show it, no
  // paragraph standing in for it. The scan is the Server section's, the part of
  // the page the form renders in: brief 12 (section 3) put the Instance tags
  // textbox in the header, outside it, and it holds no key.
  const serverSection = within(main)
    .getByRole('heading', { level: 2, name: en('server-title') })
    .closest('section')
  expect(
    serverSection !== null,
    'the Server section is there, its form is not',
  ).toBe(true)
  expect(
    [...serverSection!.querySelectorAll('input')].map((i) => i.type),
    'no key box',
  ).toEqual([])
  // Page-wide, on the profile page this test loads (17a.6): 17a.10 moved the
  // Start at boot checkbox and the Instance tags chips to the instance page,
  // so when the form is not rendered the page holds no input at all.
  const inputs = [...main.querySelectorAll<HTMLInputElement>('input')]
  expect(
    inputs.map((i) => i.type),
    'no box anywhere on the page can hold a key',
  ).toEqual([])
})

// 27c rewritten by "Administrators only" (R2): the read-only shape of the field is
// gone, so with no key every session gets the editable shape, and Show stays disabled
// while there is nothing to show (5.4).
test('1a.27c (2.8): with no key every session gets No key ticked and editable, the box empty and enabled, Show disabled', async () => {
  for (const key of [null, '']) {
    const main = await loaded({
      serverProfile: () => reply(200, serverDoc({ OpencodeApiKey: key })),
    })
    const what = JSON.stringify(key)
    // 5.2: No key ticked; 5.3: with No key ticked the box and Show are disabled.
    expect(noKey(main).checked, what).toBe(true)
    expect(noKey(main).disabled, `${what}: No key is editable`).toBe(false)
    expect(apiKey(main).value, what).toBe('')
    expect(apiKey(main).disabled, `${what}: No key is ticked`).toBe(true)
    const button = show(main)
    expect(button.disabled, `${what}: Show has nothing to show`).toBe(true)
    expect(button.getAttribute('aria-pressed'), what).toBe('false')
    expect(withheldLine(main), what).toBeNull()
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── Keyboard (section 6) ────────────────────────────────────────────────

/** The elements Tab reaches, in DOM order (no positive tabindex is used). */
function tabbable(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

/**
 * The name Tab shows for a control: its text (a link, a button), or for an input the
 * label that names it: `aria-label`, `aria-labelledby`, a `<label for>`, a wrapping
 * `<label>`.
 */
function nameOf(el: HTMLElement): string {
  if (!['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName))
    return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
  // An input has no text of its own: its name is the label that points at it.
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((id) => document.getElementById(id)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const aria = el.getAttribute('aria-label')
  if (aria) return aria
  const id = el.getAttribute('id')
  const label = id ? document.querySelector(`label[for="${id}"]`) : null
  const wrap = el.closest('label')
  return (label?.textContent ?? wrap?.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

// "Administrators only" (section 6): the tab path is the editable form's, for
// every signed-in session. The shell, then per field its About toggle and the
// controls the form leaves enabled: a textbox on default or auto is disabled
// (Port, Base URL, CPU threads, Max models and Memory margin show only their
// box), the Bind to boxes all ticked are disabled (5.2), so only the absent
// interface and the wildcard are.
const SHELL = [
  en('skip-to-content'),
  en('app-name'),
  en('nav-menu'),
  // The theme switch sits between the Menu button and the session part
  // (brief 07 H4.2).
  en('theme-dark'),
  en('session-sign-out'),
  en('nav-overview'),
  'default',
  'coder',
  // Brief 20 (20 N.7): Node info sits in the host list, after its instances,
  // and fe/briefs/02-models.md 2a criterion 1 keeps Model profiles after it,
  // where brief 16 (17a.2) puts the new Server profiles before them.
  en('nav-node'),
  en('nav-server-profiles'),
  en('nav-model-profiles'),
  // Slice 13: About is the nav's last entry, so it precedes the page.
  en('nav-about'),
]

/** The instance page's path (17a.10: the form is gone, the tags field is the
 *  page's last one, and the Server profile section shows a link, not a box). */
const TAB_PATH_INSTANCE = [
  ...SHELL,
  // The header's run buttons, inactive ones included, then Start at boot's
  // About toggle and its box (1c.45b, 46a: the box is enabled for every session).
  en('run-start'),
  en('run-stop'),
  en('run-restart'),
  about(en('boot-label')),
  en('boot-toggle'),
  // 16 17f.9: the rename form's opener follows Start at boot.
  en('instance-rename'),
  // fe/briefs/12-instance-tags.md, section 5, as fe/briefs/15-tag-chips.md
  // section 3 leaves it: the field's About toggle, then the + button (the
  // instances of this fixture have no tags, so the field shows no chip). The
  // Server profile row of 17a.10 names no control: its instance has no server
  // profile, so the row is the muted text, with no link.
  about(en('instance-tags-own')),
  en('instance-tag-add'),
]

/** The server profile page's path (17a.5): the shell, Back, the Used by link,
 *  then the form, then its save bar. */
const TAB_PATH_FORM = [
  ...SHELL,
  en('server-profile-back'),
  // 17a.5: the one Value row, the instance that runs this profile.
  'default',
  ...[
    [about('Port'), en('option-default'), 'Port'],
    [
      about('Bind to'),
      en('server-bind-localhost'),
      en('server-bind-all'),
      en('server-bind-gone', '10.0.0.9'),
      'enp5s0 (192.168.1.20)',
    ],
    [about('Base URL'), en('option-default')],
    [
      about('API key'),
      en('field-no-key'),
      en('server-api-key'),
      en('field-show'),
    ],
    [about('CPU threads'), en('field-auto')],
    [about('Batch threads'), en('field-auto'), 'Batch threads'],
    [about('Max models'), en('option-default')],
    [about('Memory margin'), en('option-default'), 'Memory margin'],
    [about('Models folder'), 'Models folder'],
    [about('Web UI'), en('server-web-ui-toggle')],
    [about('Web UI MCP proxy'), en('server-mcp-proxy-toggle')],
    [about('Log level'), 'Log level'],
    [about('Save on shutdown'), en('server-save-on-shutdown-toggle')],
    [about('Snapshot folder'), 'Snapshot folder'],
    [about('hipBLASLt'), 'hipBLASLt'],
    [about('Profiler attach'), en('server-profiler-attach-toggle')],
    [about('Tensor-split AllReduce'), 'Tensor-split AllReduce'],
    [about('Disable GPU graphs'), en('server-gpu-graphs-toggle')],
    [about('KV rotation'), en('server-kv-rotation-toggle')],
  ].flat(),
  en('save-discard'),
  en('save-button'),
]

test('6 (17a.10): Tab goes skip link, product link, Menu, theme switch, Sign out, nav links, the run buttons, Start at boot and Instance tags, and no control of a Server form', async () => {
  const main = await loadedInstance()
  // Start at boot is read on its own after the page opens (1c.45): the field
  // renders nothing until that answer lands (`value === undefined`), so the
  // tab path is taken once its checkbox is there, as InstanceTagChips C1.2
  // does. A snapshot taken at the run buttons is of a page still loading the
  // field, and reads 2 stops short.
  await within(main).findByRole('checkbox', { name: en('boot-toggle') })
  expect(tabbable().map(nameOf)).toEqual(TAB_PATH_INSTANCE)
  expect(
    screen
      .getByRole('heading', { level: 1, name: 'default' })
      .getAttribute('tabindex'),
    'the h1 takes focus only from the page (tabindex -1)',
  ).toBe('-1')
  expect(main.querySelector('.save-bar'), 'no save bar').toBeNull()
})

test('6: Tab goes the shell, Back, the Used by link, then every control of the editable form in table order, then the save bar', async () => {
  await loaded()
  expect(tabbable().map(nameOf)).toEqual(TAB_PATH_FORM)
  // The h1 takes focus only from the page (tabindex -1).
  expect(
    screen
      .getByRole('heading', { level: 1, name: 'default' })
      .getAttribute('tabindex'),
  ).toBe('-1')
})

test('6: in a field the default box comes before its textbox, the About toggle after the label', async () => {
  const main = await loaded()
  for (const [label, box] of [
    ['Port', en('field-default')],
    ['Base URL', en('field-default')],
    ['CPU threads', en('field-auto')],
    ['Batch threads', en('field-auto')],
    ['Max models', en('field-default')],
    ['Memory margin', en('field-default')],
  ] as const) {
    const g = group(main, label)
    const checkbox = g.getByRole('checkbox', { name: box })
    const textbox = g.getByRole('textbox', { name: label })
    expect(before(checkbox, textbox), label).toBe(true)
    const toggle = g.getByRole('button', { name: about(label) })
    const text = g.getAllByText(label, { exact: true })[0]!
    expect(before(text, toggle), label).toBe(true)
    expect(before(toggle, checkbox), label).toBe(true)
  }
  const key = group(main, 'API key')
  expect(
    before(
      key.getByRole('checkbox', { name: en('field-no-key') }),
      apiKey(main),
    ),
  ).toBe(true)
  expect(
    before(apiKey(main), key.getByRole('button', { name: en('field-show') })),
  ).toBe(true)
})

// ── Brief 16, 17a.10: the page loses the Server form ────────────────────

/** The instance page's Server profile section, once the list answered. */
async function profileSection(main: HTMLElement): Promise<HTMLElement> {
  const h2 = await within(main).findByRole('heading', {
    level: 2,
    name: en('instance-server-title'),
  })
  return h2.parentElement!
}

/** The Value list of a section. */
function assetRows(section: HTMLElement): [string, string][] {
  const dl = section.querySelector('dl.assets')
  expect(dl, 'a Value list').not.toBeNull()
  return [...dl!.querySelectorAll('dt')].map((dt, i) => [
    (dt.textContent ?? '').trim(),
    (dl!.querySelectorAll('dd')[i]?.textContent ?? '').trim(),
  ])
}

/** The tags field: the page's one field, whose writes brief 15 guards. */
function tagsField(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: en('instance-tags-own') })
}

test('16 17a.10: the instance page reads and writes no server file, and shows no Server form, no h2 server-title and no save bar', async () => {
  const bff = fakeBff()
  const main = await loadedInstance()
  expect(
    within(main).queryByRole('heading', {
      level: 2,
      name: en('server-title'),
    }),
    'the Server section is on the profile page',
  ).toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 3,
      name: en('server-network'),
    }),
    'no form',
  ).toBeNull()
  expect(main.querySelector('.save-bar'), 'no save bar').toBeNull()
  expect(
    within(main).queryByRole('button', { name: en('save-button') }),
    'no Save',
  ).toBeNull()
  expect(
    bff.to('GET', '/api/instances/default/server'),
    'no read of the server file',
  ).toHaveLength(0)
  expect(
    bff.to('GET', '/api/profiles/server/default'),
    'nor of the profile file',
  ).toHaveLength(0)
  expect(
    bff.calls.filter((c) => c.method === 'PUT'),
    'nothing written',
  ).toEqual([])
})

test('16 17a.10: after the tags field, a section with h2 instance-server-title holds a Value row naming the list item’s serverProfile as a link to its page', async () => {
  const main = await loadedInstance({
    instances: [
      {
        id: 'default',
        engine: 'llama.cpp',
        state: 'active',
        serverProfile: 'edge box',
      },
    ],
  })
  const section = await profileSection(main)
  expect(assetRows(section)).toEqual([
    [en('instance-server-title'), 'edge box'],
  ])
  const link = within(section).getByRole('link', { name: 'edge box' })
  expect(link.getAttribute('href')).toBe(serverProfileHref('edge box'))
  expect(
    before(tagsField(main), section),
    'the section follows the tags field',
  ).toBe(true)
})

test('16 17a.10: an instance naming no server profile shows instance-server-none, muted, with no link', async () => {
  const main = await loadedInstance({
    instances: [
      {
        id: 'default',
        engine: 'llama.cpp',
        state: 'active',
        serverProfile: null,
      },
    ],
  })
  const section = await profileSection(main)
  expect(assetRows(section)).toEqual([
    [en('instance-server-title'), en('instance-server-none')],
  ])
  expect(within(section).queryByRole('link'), 'no link').toBeNull()
  const dd = section.querySelector('dl.assets dd')
  expect(dd!.className, 'muted').toContain('muted')
})

test('16 17a.10: the row follows every list answer: a polled answer that gives the instance a profile turns the text into a link', async () => {
  // The row follows the polls, and the polls run on fake seconds (IDLE_MS):
  // the clock is faked, so the test steps onto the next read.
  vi.useFakeTimers({ shouldAdvanceTime: true })
  try {
    // The test owns the fake bff (loadedInstance() stubs one of its own), so
    // the page is opened here.
    const bff = fakeBff()
    startAt('#/instances/default')
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('button', { name: en('run-stop') })
    const section = await profileSection(main)
    expect(assetRows(section)[0][1], 'the fixture names no profile').toBe(
      en('instance-server-none'),
    )
    bff.setInstances([
      {
        id: 'default',
        engine: 'llama.cpp',
        state: 'active',
        serverProfile: 'edge-01',
      },
    ])
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6_000)
    })
    await within(main).findByRole('link', { name: 'edge-01' })
    expect(assetRows(await profileSection(main))).toEqual([
      [en('instance-server-title'), 'edge-01'],
    ])
  } finally {
    vi.useRealTimers()
  }
})

test('16 17a.10: leaving asks only for the tags: an open tag pill asks with the discard dialog, and Confirm leaves at once', async () => {
  const dialogs = stubDialog()
  try {
    const main = await loadedInstance()
    expect(asksBrowser(), 'nothing open: the browser is not asked').toBe(false)
    const add = within(tagsField(main)).getByRole('button', {
      name: en('instance-tag-add'),
    })
    fireEvent.click(add)
    const pill = within(tagsField(main)).getByRole('combobox', {
      name: en('instance-tag-new'),
    }) as HTMLInputElement
    fireEvent.change(pill, { target: { value: 'fast' } })
    fireEvent.click(
      within(
        screen.getByRole('navigation', { name: en('nav-label') }),
      ).getByRole('link', { name: en('nav-overview') }),
    )
    const d = await screen.findByRole('dialog', { name: en('discard-title') })
    expect(dialogs.opened, 'opened with showModal()').toContain(d)
    expect(
      window.location.hash,
      'the page is still there while the question is open',
    ).toBe('#/instances/default')
    fireEvent.click(
      within(d).getByRole('button', { name: en('discard-confirm') }),
    )
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0))
    })
    expect(window.location.hash, 'left').toBe('#/')
  } finally {
    dialogs.restore()
  }
})

/** Asks the browser whether leaving the page would be held. */
function asksBrowser(): boolean {
  const e = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(e)
  return e.defaultPrevented
}
