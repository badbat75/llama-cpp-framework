// The Overview's instances (fe/briefs/01-server.md, slice 1a, section 2.3),
// through the whole app at #/ against a fake bff (mocked fetch). The bff
// answers GET /api/instances with 200 and [{"id", "engine", "state"}] in the
// manager's order, and errors as {"key", "args"} (fe/briefs/api-contract.md):
// 503 client-unavailable with no manager, 502 client-not-authorized when the
// manager refuses the bff, anything else generic. Signed out here, so the nav
// lists no instance and the only instance links are the table's.
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { en, fakeBff, reply, startAt, type Instance } from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

function stubInstances(answer: () => Response | Promise<Response>) {
  return fakeBff({ session: 'signed-out', instances: answer })
}

function list(instances: Instance[]) {
  return fakeBff({ session: 'signed-out', instances })
}

/** The instances table of main. */
async function table(): Promise<HTMLElement> {
  return within(screen.getByRole('main')).findByRole('table')
}

/** The column headers of the table. */
async function headers(): Promise<string[]> {
  return within(await table())
    .getAllByRole('columnheader')
    .map((h) => h.textContent ?? '')
}

/** The data rows of the instances table: the cells' text per row. */
async function rows(): Promise<string[][]> {
  return within(await table())
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
    .map((row) =>
      within(row)
        .getAllByRole('cell')
        .map((c) => c.textContent ?? ''),
    )
}

// ── Failure paths first ─────────────────────────────────────────────────

test('1a.16: a service that cannot be reached shows the failure message', async () => {
  stubInstances(() => Promise.reject(new TypeError('Failed to fetch')))
  render(<App />)
  expect(await screen.findByText(en('instances-failed'))).toBeTruthy()
  expect(screen.queryByRole('table')).toBeNull()
})

test('1a.16: client-unavailable shows the manager-unavailable message', async () => {
  stubInstances(() => reply(503, { key: 'client-unavailable', args: [] }))
  render(<App />)
  expect(
    await screen.findByText(en('instances-manager-unavailable')),
  ).toBeTruthy()
  expect(document.body.textContent).not.toContain(en('instances-failed'))
  expect(screen.queryByRole('table')).toBeNull()
})

test('1a.16: client-not-authorized shows the not-authorized message', async () => {
  stubInstances(() => reply(502, { key: 'client-not-authorized', args: [] }))
  render(<App />)
  expect(await screen.findByText(en('instances-not-authorized'))).toBeTruthy()
  expect(screen.queryByRole('table')).toBeNull()
})

test('1a.16: the old {"error"} shape and any other error show the failure message, never the body', async () => {
  for (const [status, body] of [
    [503, '{"error":"unavailable"}'],
    [502, '{"error":"not-authorized"}'],
    [502, '{"key":"failed","args":[]}'],
    [500, '{"key":"secret-detail","args":[]}'],
    [404, 'secret-detail'],
    [503, 'not json'],
  ] as const) {
    stubInstances(() => reply(status, body))
    render(<App />)
    expect(await screen.findByText(en('instances-failed')), body).toBeTruthy()
    expect(document.body.textContent, body).not.toContain('secret-detail')
    expect(document.body.textContent, body).not.toContain(
      en('instances-manager-unavailable'),
    )
    expect(screen.queryByRole('table'), body).toBeNull()
    cleanup()
  }
})

test('1a.16: a body that is not a list of instances shows the failure message', async () => {
  for (const body of [
    'not json',
    '{}',
    '{"instances":[]}',
    '[{"id":1,"engine":"llama.cpp","state":"active"}]',
    '[{"id":"default","engine":"llama.cpp"}]',
    '[{"id":"default","engine":"llama.cpp","state":7}]',
    '[{"id":"","engine":"llama.cpp","state":"active"}]',
    '[null]',
  ]) {
    stubInstances(() => reply(200, body))
    render(<App />)
    expect(await screen.findByText(en('instances-failed')), body).toBeTruthy()
    expect(document.body.textContent, body).not.toMatch(/undefined|NaN|\{0\}/)
    expect(screen.queryByRole('table'), body).toBeNull()
    cleanup()
  }
})

// ── What it shows ───────────────────────────────────────────────────────

test('1a.16: the Overview has its h1, the version line and the instances section', async () => {
  const bff = list([{ id: 'default', engine: 'llama.cpp', state: 'active' }])
  render(<App />)
  const main = screen.getByRole('main')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  const version = await within(main).findByText(en('version-label', '2.99.0'))
  const section = within(main).getByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  expect(
    h1.compareDocumentPosition(version) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(
    version.compareDocumentPosition(section) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  await table()
  const init = bff.to('GET', '/api/instances')
  expect(init.length).toBeGreaterThan(0)
})

test('1a.16: no instances shows the empty message, not a table', async () => {
  list([])
  render(<App />)
  expect(await screen.findByText(en('instances-empty'))).toBeTruthy()
  expect(screen.queryByRole('table')).toBeNull()
  expect(document.body.textContent).not.toContain(en('instances-failed'))
})

test('1a.16: each instance is a row with its state, in the order received', async () => {
  list([
    { id: 'default', engine: 'llama.cpp', state: 'active' },
    { id: 'coder', engine: 'llama.cpp', state: 'failed' },
    { id: 'big-model', engine: 'llama.cpp', state: 'inactive' },
    { id: 'a', engine: 'llama.cpp', state: 'activating' },
    { id: 'z', engine: 'llama.cpp', state: 'deactivating' },
  ])
  render(<App />)
  expect(await rows()).toEqual([
    ['default', en('instance-state-active')],
    ['coder', en('instance-state-failed')],
    ['big-model', en('instance-state-inactive')],
    ['a', en('instance-state-activating')],
    ['z', en('instance-state-deactivating')],
  ])
  expect(document.body.textContent).not.toContain(en('instances-empty'))
})

test('1a.16: a state the page does not know is shown by its name', async () => {
  list([{ id: 'odd', engine: 'llama.cpp', state: 'maintenance' }])
  render(<App />)
  expect(await rows()).toEqual([
    ['odd', en('instance-state-other', 'maintenance')],
  ])
  expect(document.body.textContent).not.toContain('{0}')
})

test('1a.17: each instance id in the table is a link to its page', async () => {
  list([
    { id: 'default', engine: 'llama.cpp', state: 'active' },
    { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
  ])
  render(<App />)
  const links = within(await table()).getAllByRole('link')
  expect(links.map((a) => [a.textContent, a.getAttribute('href')])).toEqual([
    ['default', '#/instances/default'],
    ['coder', '#/instances/coder'],
  ])
  // The link sits in the first cell of its row.
  for (const a of links) {
    const cell = a.closest('td, th')!
    expect(cell.parentElement!.firstElementChild).toBe(cell)
  }
})

test('1a.18: one engine keeps the two columns of today, no Engine column', async () => {
  list([
    { id: 'default', engine: 'llama.cpp', state: 'active' },
    { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
  ])
  render(<App />)
  expect(await headers()).toEqual([
    en('instances-column-id'),
    en('instances-column-state'),
  ])
  expect(document.body.textContent).not.toContain('llama.cpp')
})

test('1a.18: two or more engines add the Engine column, the engine name as data', async () => {
  list([
    { id: 'default', engine: 'llama.cpp', state: 'active' },
    { id: 'strata', engine: 'Strata', state: 'inactive' },
    { id: 'coder', engine: 'llama.cpp', state: 'failed' },
  ])
  render(<App />)
  // Instance, Engine, State: the order of the instance page's header (2.6).
  expect(await headers()).toEqual([
    en('instances-column-id'),
    en('instances-column-engine'),
    en('instances-column-state'),
  ])
  expect(await rows()).toEqual([
    ['default', 'llama.cpp', en('instance-state-active')],
    ['strata', 'Strata', en('instance-state-inactive')],
    ['coder', 'llama.cpp', en('instance-state-failed')],
  ])
})
