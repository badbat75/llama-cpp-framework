// The page as main.tsx renders it: the version and, beside it, the
// instances, each from its own route of the bff (fetch mocked by URL).
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.tsx'

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function reply(body: string): Response {
  return new Response(body, {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}

test('the page shows the version and the instances', async () => {
  const fetch = vi.fn((url: RequestInfo | URL) => {
    switch (String(url)) {
      case '/api/version':
        return Promise.resolve(reply('{"version":"2.99.0"}'))
      case '/api/instances':
        return Promise.resolve(reply('[{"id":"default","state":"active"}]'))
      default:
        return Promise.reject(new TypeError(`unexpected ${String(url)}`))
    }
  })
  vi.stubGlobal('fetch', fetch)
  render(<App />)
  expect(await screen.findByText('Version 2.99.0')).toBeTruthy()
  expect(await screen.findByRole('cell', { name: 'default' })).toBeTruthy()
  const asked = fetch.mock.calls.map((c) => String(c[0])).sort()
  expect([...new Set(asked)]).toEqual(['/api/instances', '/api/version'])
})
