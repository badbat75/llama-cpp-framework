// The page's version view, against a mocked fetch: no bff runs.
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import strings from './strings/en_US.json'
import Version from './Version.tsx'

// Vitest's globals are off, so Testing Library cannot register its own
// cleanup: unmount after every test here.
afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function reply(status: number, body: string): Response {
  return new Response(body, {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function stubFetch(answer: () => Promise<Response>) {
  const fetch = vi.fn(answer)
  vi.stubGlobal('fetch', fetch)
  return fetch
}

const unavailable = strings['version-unavailable']

// ── Failure paths first ─────────────────────────────────────────────────

test('a service that cannot be reached shows the failure message', async () => {
  stubFetch(() => Promise.reject(new TypeError('Failed to fetch')))
  render(<Version />)
  expect(await screen.findByText(unavailable)).toBeTruthy()
})

test('an error status shows the failure message, not the body', async () => {
  stubFetch(() => Promise.resolve(reply(500, '{"version":"9.9.9"}')))
  render(<Version />)
  expect(await screen.findByText(unavailable)).toBeTruthy()
  expect(document.body.textContent).not.toContain('9.9.9')
})

test('a body that is not the version shows the failure message', async () => {
  for (const body of ['not json', '{}', '{"version":42}', '{"version":""}']) {
    stubFetch(() => Promise.resolve(reply(200, body)))
    render(<Version />)
    expect(await screen.findByText(unavailable), body).toBeTruthy()
    expect(document.body.textContent, body).not.toMatch(/undefined|42|\{0\}/)
    cleanup()
  }
})

// ── What it shows ───────────────────────────────────────────────────────

test('the version read from /api/version is shown', async () => {
  const fetch = stubFetch(() =>
    Promise.resolve(reply(200, '{"version":"2.99.0"}')),
  )
  render(<Version />)
  expect(await screen.findByText('Version 2.99.0')).toBeTruthy()
  expect(fetch).toHaveBeenCalled()
  expect(String(fetch.mock.calls[0]?.[0])).toBe('/api/version')
  expect(document.body.textContent).not.toContain(unavailable)
})
