// The instances view, against a mocked fetch: no bff runs. The bff answers
// GET /api/instances with 200 and [{"id", "state"}] in the manager's order,
// 503 {"error":"unavailable"} with no manager, 502 {"error":"not-authorized"}
// when the manager refuses the bff, 502 {"error":"failed"} otherwise.
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import strings from './strings/en_US.json'
import Instances from './Instances.tsx'
import { t } from './strings.ts'

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

/** The text of `key`, which must be in the strings file. */
function text(key: string, ...args: string[]): string {
  const table: Record<string, string> = strings
  if (!(key in table)) throw new Error(`missing string ${key} in en_US.json`)
  return t(key, ...args)
}

function reply(status: number, body: string): Response {
  return new Response(body, {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function stubFetch(answer: () => Promise<Response>) {
  const fetch = vi.fn<typeof globalThis.fetch>(answer)
  vi.stubGlobal('fetch', fetch)
  return fetch
}

/** The data rows of the instances table: [id, state] per row. */
async function rows(): Promise<string[][]> {
  const table = await screen.findByRole('table')
  return within(table)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
    .map((row) =>
      within(row)
        .getAllByRole('cell')
        .map((c) => c.textContent ?? ''),
    )
}

// ── Failure paths first ─────────────────────────────────────────────────

test('a service that cannot be reached shows the failure message', async () => {
  stubFetch(() => Promise.reject(new TypeError('Failed to fetch')))
  render(<Instances />)
  expect(await screen.findByText(text('instances-failed'))).toBeTruthy()
  expect(screen.queryByRole('table')).toBeNull()
})

test('no manager shows the manager-unavailable message', async () => {
  stubFetch(() => Promise.resolve(reply(503, '{"error":"unavailable"}')))
  render(<Instances />)
  expect(
    await screen.findByText(text('instances-manager-unavailable')),
  ).toBeTruthy()
  expect(document.body.textContent).not.toContain(text('instances-failed'))
  expect(screen.queryByRole('table')).toBeNull()
})

test('a manager that refuses the service shows the not-authorized message', async () => {
  stubFetch(() => Promise.resolve(reply(502, '{"error":"not-authorized"}')))
  render(<Instances />)
  expect(await screen.findByText(text('instances-not-authorized'))).toBeTruthy()
  expect(screen.queryByRole('table')).toBeNull()
})

test('any other error shows the failure message, never the body', async () => {
  for (const [status, body] of [
    [502, '{"error":"failed"}'],
    [500, '{"error":"secret-detail"}'],
    [404, 'secret-detail'],
    [503, 'not json'],
  ] as const) {
    stubFetch(() => Promise.resolve(reply(status, body)))
    render(<Instances />)
    expect(await screen.findByText(text('instances-failed')), body).toBeTruthy()
    expect(document.body.textContent, body).not.toContain('secret-detail')
    expect(screen.queryByRole('table'), body).toBeNull()
    cleanup()
  }
})

test('a body that is not a list of instances shows the failure message', async () => {
  for (const body of [
    'not json',
    '{}',
    '{"instances":[]}',
    '[{"id":1,"state":"active"}]',
    '[{"id":"default"}]',
    '[{"id":"default","state":7}]',
    '[{"id":"","state":"active"}]',
    '[null]',
  ]) {
    stubFetch(() => Promise.resolve(reply(200, body)))
    render(<Instances />)
    expect(await screen.findByText(text('instances-failed')), body).toBeTruthy()
    expect(document.body.textContent, body).not.toMatch(
      /undefined|null|NaN|\{0\}/,
    )
    expect(screen.queryByRole('table'), body).toBeNull()
    cleanup()
  }
})

// ── What it shows ───────────────────────────────────────────────────────

test('it asks /api/instances', async () => {
  const fetch = stubFetch(() => Promise.resolve(reply(200, '[]')))
  render(<Instances />)
  await screen.findByText(text('instances-empty'))
  expect(fetch).toHaveBeenCalled()
  expect(String(fetch.mock.calls[0]?.[0])).toBe('/api/instances')
  const init = fetch.mock.calls[0]?.[1] as RequestInit | undefined
  expect(init?.method ?? 'GET').toBe('GET')
})

test('no instances shows the empty message, not a table', async () => {
  stubFetch(() => Promise.resolve(reply(200, '[]')))
  render(<Instances />)
  expect(await screen.findByText(text('instances-empty'))).toBeTruthy()
  expect(screen.queryByRole('table')).toBeNull()
  expect(document.body.textContent).not.toContain(text('instances-failed'))
})

test('each instance is a row with its state, in the order received', async () => {
  const instances = [
    { id: 'default', state: 'active' },
    { id: 'coder', state: 'failed' },
    { id: 'big-model', state: 'inactive' },
    { id: 'a', state: 'activating' },
    { id: 'z', state: 'deactivating' },
  ]
  stubFetch(() => Promise.resolve(reply(200, JSON.stringify(instances))))
  render(<Instances />)
  expect(
    await screen.findByRole('heading', { name: text('instances-title') }),
  ).toBeTruthy()
  expect(await rows()).toEqual([
    ['default', text('instance-state-active')],
    ['coder', text('instance-state-failed')],
    ['big-model', text('instance-state-inactive')],
    ['a', text('instance-state-activating')],
    ['z', text('instance-state-deactivating')],
  ])
  const table = screen.getByRole('table')
  expect(
    within(table)
      .getAllByRole('columnheader')
      .map((h) => h.textContent),
  ).toEqual([text('instances-column-id'), text('instances-column-state')])
  expect(document.body.textContent).not.toContain(text('instances-empty'))
})

test('a state the page does not know is shown by its name', async () => {
  stubFetch(() =>
    Promise.resolve(reply(200, '[{"id":"odd","state":"maintenance"}]')),
  )
  render(<Instances />)
  const shown = text('instance-state-other', 'maintenance')
  expect(shown).toContain('maintenance')
  expect(await rows()).toEqual([['odd', shown]])
  expect(document.body.textContent).not.toContain('{0}')
})

test('every state label is its own text', () => {
  const labels = [
    'active',
    'inactive',
    'activating',
    'deactivating',
    'failed',
  ].map((s) => text(`instance-state-${s}`))
  expect(new Set(labels).size).toBe(labels.length)
  for (const label of labels) expect(label).not.toMatch(/^instance-state-/)
})
