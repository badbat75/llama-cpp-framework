// The Overview's instances (fe/briefs/01-server.md, slice 1a, section 2.3),
// through the whole app at #/ against a fake bff (mocked fetch). The bff
// answers GET /api/instances with 200 and [{"id", "engine", "state"}] in the
// manager's order, and errors as {"key", "args"} (fe/briefs/api-contract.md):
// 503 client-unavailable with no manager, 502 client-not-authorized when the
// manager refuses the bff, anything else generic. Signed in here (09 S1a.1:
// the Overview is the signed-in page), so the nav lists the instances beside
// the list (H6a rewrote the table of 1a.16 to 1a.18 as a list of tiles:
// fe/briefs/08-hearth-h6).
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
  return fakeBff({ instances: answer })
}

function list(instances: Instance[]) {
  return fakeBff({ instances })
}

/** The instances list of main (H6a.1: a `ul`, not a table). */
async function tiles(): Promise<HTMLElement> {
  return within(screen.getByRole('main')).findByRole('list')
}

/** The tiles of the instances list: the id link and the badge text of each. */
async function tileTexts(): Promise<string[][]> {
  return within(await tiles())
    .getAllByRole('listitem')
    .map((item) => [
      within(item).getByRole('link').textContent ?? '',
      item.querySelector('.badge')?.textContent ?? '',
    ])
}

/** No instances list in main: what every state that is not the list shows. */
function noList(what?: string): void {
  expect(within(screen.getByRole('main')).queryByRole('list'), what).toBeNull()
  expect(
    within(screen.getByRole('main')).queryByRole('table'),
    'and no table either: the table of 1a.16 is gone, the instances are tiles (H6a.2)',
  ).toBeNull()
}

// ── Failure paths first ─────────────────────────────────────────────────

test('1a.16: a service that cannot be reached shows the failure message', async () => {
  stubInstances(() => Promise.reject(new TypeError('Failed to fetch')))
  render(<App />)
  expect(await screen.findByText(en('instances-failed'))).toBeTruthy()
  noList()
})

test('1a.16: client-unavailable shows the manager-unavailable message', async () => {
  stubInstances(() => reply(503, { key: 'client-unavailable', args: [] }))
  render(<App />)
  expect(
    await screen.findByText(en('instances-manager-unavailable')),
  ).toBeTruthy()
  expect(document.body.textContent).not.toContain(en('instances-failed'))
  noList()
})

test('1a.16: client-not-authorized shows the not-authorized message', async () => {
  stubInstances(() => reply(502, { key: 'client-not-authorized', args: [] }))
  render(<App />)
  expect(await screen.findByText(en('instances-not-authorized'))).toBeTruthy()
  noList()
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
    noList(body)
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
    noList(body)
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
  await tiles()
  expect(
    within(main).queryByRole('table'),
    'the instances of the Overview are a list of tiles, not a table (H6a.2)',
  ).toBeNull()
  const init = bff.to('GET', '/api/instances')
  expect(init.length).toBeGreaterThan(0)
})

test('1a.16: no instances shows the empty message, not a table', async () => {
  list([])
  render(<App />)
  expect(await screen.findByText(en('instances-empty'))).toBeTruthy()
  noList()
  expect(document.body.textContent).not.toContain(en('instances-failed'))
})

test('1a.16: each instance is a tile with its state, in the order received', async () => {
  list([
    { id: 'default', engine: 'llama.cpp', state: 'active' },
    { id: 'coder', engine: 'llama.cpp', state: 'failed' },
    { id: 'big-model', engine: 'llama.cpp', state: 'inactive' },
    { id: 'a', engine: 'llama.cpp', state: 'activating' },
    { id: 'z', engine: 'llama.cpp', state: 'deactivating' },
  ])
  render(<App />)
  expect(await tileTexts()).toEqual([
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
  expect(await tileTexts()).toEqual([
    ['odd', en('instance-state-other', 'maintenance')],
  ])
  expect(document.body.textContent).not.toContain('{0}')
})

test('1a.17: each instance id in the list is a link to its page', async () => {
  list([
    { id: 'default', engine: 'llama.cpp', state: 'active' },
    { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
  ])
  render(<App />)
  const links = within(await tiles()).getAllByRole('link')
  expect(links.map((a) => [a.textContent, a.getAttribute('href')])).toEqual([
    ['default', '#/instances/default'],
    ['coder', '#/instances/coder'],
  ])
  // The link is the first thing in the head row of its tile (H6a.2).
  for (const a of links) {
    const head = a.closest('.tile-head')!
    expect(head).toBeTruthy()
    expect(head.firstElementChild).toBe(a)
  }
})

test('1a.18: one engine shows no engine line at all', async () => {
  list([
    { id: 'default', engine: 'llama.cpp', state: 'active' },
    { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
  ])
  render(<App />)
  expect(within(await tiles()).getAllByRole('listitem').length).toBe(2)
  // The engine line is a `p` of the tile (H6a.2): with one engine, none.
  expect(within(await tiles()).queryAllByRole('paragraph')).toEqual([])
  expect(document.body.textContent).not.toContain('llama.cpp')
})

test('1a.18: two or more engines add one engine line to each tile', async () => {
  list([
    { id: 'default', engine: 'llama.cpp', state: 'active' },
    { id: 'strata', engine: 'Strata', state: 'inactive' },
    { id: 'coder', engine: 'llama.cpp', state: 'failed' },
  ])
  render(<App />)
  const items = within(await tiles()).getAllByRole('listitem')
  expect(items.length).toBe(3)
  expect(
    items.map((item) => within(item).getByRole('paragraph').textContent),
  ).toEqual([
    en('instance-engine', 'llama.cpp'),
    en('instance-engine', 'Strata'),
    en('instance-engine', 'llama.cpp'),
  ])
  // The state is still the tile's text, and so is the id.
  expect(await tileTexts()).toEqual([
    ['default', en('instance-state-active')],
    ['strata', en('instance-state-inactive')],
    ['coder', en('instance-state-failed')],
  ])
})
