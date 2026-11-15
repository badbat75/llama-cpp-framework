// Sign-in and sign-out (fe/briefs/01-server.md, slice 1a, section 2.2):
// the form at #/sign-in and in place of a protected page while signed out,
// its checks, its errors and the session it opens. The whole app renders
// against a fake bff (mocked fetch); "1a.11" is the brief's criterion 11.
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
  ROLE_WORDS,
  before,
  deferred,
  en,
  fakeBff,
  reply,
  startAt,
  stubDialog,
} from './test/bff.ts'

beforeEach(() => startAt('#/sign-in'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

/** The sign-in form's controls, once it is shown in main. */
async function form() {
  const main = screen.getByRole('main')
  const username = (await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })) as HTMLInputElement
  const password = within(main).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  const submit = within(main).getByRole('button', { name: en('signin-submit') })
  return { main, username, password, submit }
}

function type(input: HTMLInputElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

// ── Failure paths first ─────────────────────────────────────────────────

test('1a.11: an empty user name and password send nothing and mark the user name first', async () => {
  const bff = fakeBff({ session: 'signed-out' })
  render(<App />)
  const { main, username, password, submit } = await form()
  fireEvent.click(submit)
  const error = await within(main).findByText(en('signin-empty-username'))
  expect(username.getAttribute('aria-invalid')).toBe('true')
  expect(username.getAttribute('aria-describedby') ?? '').not.toBe('')
  const described = username
    .getAttribute('aria-describedby')!
    .split(/\s+/)
    .map((id) => document.getElementById(id))
  expect(described.some((el) => el !== null && el.contains(error))).toBe(true)
  expect(document.activeElement).toBe(username)
  const passwordError = within(main).getByText(en('signin-empty-password'))
  expect(password.getAttribute('aria-invalid')).toBe('true')
  // Each error under its own field.
  expect(before(username, error)).toBe(true)
  expect(before(error, password)).toBe(true)
  expect(before(password, passwordError)).toBe(true)
  expect(bff.to('POST', '/api/session')).toHaveLength(0)
})

test('1a.11: an empty password sends nothing and marks the password', async () => {
  const bff = fakeBff({ session: 'signed-out' })
  render(<App />)
  const { main, username, password, submit } = await form()
  type(username, 'alice')
  fireEvent.click(submit)
  const error = await within(main).findByText(en('signin-empty-password'))
  expect(password.getAttribute('aria-invalid')).toBe('true')
  const described = (password.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .map((id) => document.getElementById(id))
  expect(described.some((el) => el !== null && el.contains(error))).toBe(true)
  expect(document.activeElement).toBe(password)
  expect(within(main).queryByText(en('signin-empty-username'))).toBeNull()
  expect(username.getAttribute('aria-invalid')).not.toBe('true')
  expect(bff.to('POST', '/api/session')).toHaveLength(0)
})

test('1a.14: wrong credentials show the alert, clear the password, keep the user name', async () => {
  fakeBff({
    session: 'signed-out',
    signIn: () => reply(401, { key: 'session-bad-credentials', args: [] }),
  })
  render(<App />)
  const { main, username, password, submit } = await form()
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const alert = await within(main).findByRole('alert')
  expect(alert.tagName).toBe('P')
  expect(alert.textContent).toBe(en('api-session-bad-credentials'))
  // Above the fields.
  expect(
    alert.compareDocumentPosition(username) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(password.value).toBe('')
  expect(username.value).toBe('alice')
  expect(document.activeElement).toBe(password)
  expect(
    screen.queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
})

test('1a.14: too many attempts shows the seconds to wait', async () => {
  fakeBff({
    session: 'signed-out',
    signIn: () =>
      reply(429, { key: 'session-too-many-attempts', args: ['30'] }),
  })
  render(<App />)
  const { main, username, password, submit } = await form()
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const alert = await within(main).findByRole('alert')
  expect(alert.textContent).toBe(en('api-session-too-many-attempts', '30'))
})

test('1a.14: any other failure shows signin-failed, never the body', async () => {
  const answers: [string, () => Response | Promise<Response>][] = [
    ['network', () => Promise.reject(new TypeError('Failed to fetch'))],
    ['500', () => reply(500, { key: 'secret-detail', args: [] })],
    ['503', () => reply(503, { key: 'client-unavailable', args: [] })],
    [
      '400 session-malformed',
      () => reply(400, { key: 'session-malformed', args: [] }),
    ],
    ['401 other key', () => reply(401, { key: 'secret-detail', args: [] })],
    // 14b: by key, not status.
    ['403 other key', () => reply(403, { key: 'secret-detail', args: [] })],
    ['403 no key', () => reply(403, {})],
    ['not json', () => reply(401, 'secret-detail')],
    ['200 malformed', () => reply(200, '{"user":42}')],
  ]
  for (const [what, signIn] of answers) {
    fakeBff({ session: 'signed-out', signIn })
    render(<App />)
    const { main, username, password, submit } = await form()
    type(username, 'alice')
    type(password, 'hunter2')
    fireEvent.click(submit)
    const alert = await within(main).findByRole('alert')
    expect(alert.textContent, what).toBe(en('signin-failed'))
    expect(document.body.textContent, what).not.toContain('secret-detail')
    expect(
      screen.queryByRole('button', { name: en('session-sign-out') }),
      what,
    ).toBeNull()
    cleanup()
    vi.unstubAllGlobals()
    startAt('#/sign-in')
  }
})

test('1a.14a: sign-in unavailable shows its own alert, clears the password, and a retry is still sent', async () => {
  const bff = fakeBff({
    session: 'signed-out',
    signIn: () => reply(503, { key: 'session-unavailable', args: [] }),
  })
  render(<App />)
  const { main, username, password, submit } = await form()
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const alert = await within(main).findByRole('alert')
  expect(alert.tagName).toBe('P')
  expect(alert.textContent).toBe(en('api-session-unavailable'))
  expect(alert.textContent).not.toBe(en('signin-failed'))
  // Above the fields.
  expect(
    alert.compareDocumentPosition(username) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(password.value).toBe('')
  expect(username.value).toBe('alice')
  expect(document.activeElement).toBe(password)
  expect(
    screen.queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
  // The form stays usable: a second submit is sent.
  expect(submit.textContent).toBe(en('signin-submit'))
  type(password, 'hunter3')
  fireEvent.click(submit)
  await waitFor(() => expect(bff.to('POST', '/api/session')).toHaveLength(2))
  expect(bff.to('POST', '/api/session')[1]!.body).toEqual({
    username: 'alice',
    password: 'hunter3',
  })
})

test('1a.14b: a non-administrator gets its own alert, the password cleared, the user name kept and focused; the app stays signed out', async () => {
  const bff = fakeBff({
    session: 'signed-out',
    signIn: () => reply(403, { key: 'session-not-administrator', args: [] }),
  })
  render(<App />)
  const { main, username, password, submit } = await form()
  type(username, 'bob')
  type(password, 'right-password')
  fireEvent.click(submit)
  const alert = await within(main).findByRole('alert')
  expect(alert.tagName).toBe('P')
  expect(alert.textContent).toBe(en('api-session-not-administrator'))
  expect(alert.textContent).not.toBe(en('signin-failed'))
  // Above the fields.
  expect(
    alert.compareDocumentPosition(username) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(password.value).toBe('')
  expect(username.value).toBe('bob')
  // Unlike 14 and 14a: the account needs changing, not the password.
  expect(document.activeElement).toBe(username)
  // Still signed out: the banner keeps Sign in, no Sign out.
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
      within(banner).queryByRole('link', { name: en('session-sign-in') }),
  ).not.toBeNull()
  expect(banner.textContent).not.toContain(en('session-signed-in-as', 'bob'))
  // The form stays usable for another try.
  expect(submit.textContent).toBe(en('signin-submit'))
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  await waitFor(() => expect(bff.to('POST', '/api/session')).toHaveLength(2))
  expect(bff.to('POST', '/api/session')[1]!.body).toEqual({
    username: 'alice',
    password: 'hunter2',
  })
})

test('1a.14b: after a non-administrator refusal the Overview link still works', async () => {
  fakeBff({
    session: 'signed-out',
    signIn: () => reply(403, { key: 'session-not-administrator', args: [] }),
  })
  render(<App />)
  const { main, username, password, submit } = await form()
  type(username, 'bob')
  type(password, 'right-password')
  fireEvent.click(submit)
  await within(main).findByText(en('api-session-not-administrator'))
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  fireEvent.click(within(nav).getByRole('link', { name: en('nav-overview') }))
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
  expect(
    await screen.findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
  expect(
    screen.queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
})

test('1a.14b, 36: in the session-ended sign-in dialog the refusal shows inside it; the dialog stays open and its Cancel works', async () => {
  const dialogs = stubDialog()
  try {
    startAt('#/instances/default')
    const bff = fakeBff({
      save: () => reply(401, { key: 'session-required', args: [] }),
      signIn: () => reply(403, { key: 'session-not-administrator', args: [] }),
    })
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('heading', {
      level: 3,
      name: en('server-network'),
    })
    const port = within(
      within(main).getByRole('group', { name: 'Port' }),
    ).getByRole('textbox', { name: 'Port' }) as HTMLInputElement
    type(port, '8581')
    fireEvent.click(
      within(main).getByRole('button', { name: en('save-button') }),
    )
    await waitFor(() =>
      expect(bff.to('PUT', '/api/instances/default/server')).toHaveLength(1),
    )
    fireEvent.click(
      await within(main).findByRole('button', { name: en('session-sign-in') }),
    )
    const d = screen.getByRole('dialog', { name: en('signin-title') })
    expect(dialogs.opened).toContain(d)
    const user = within(d).getByRole('textbox', {
      name: en('signin-username'),
    }) as HTMLInputElement
    const pass = within(d).getByLabelText(en('signin-password'), {
      selector: 'input',
    }) as HTMLInputElement
    type(user, 'bob')
    type(pass, 'right-password')
    fireEvent.click(
      within(d).getByRole('button', { name: en('signin-submit') }),
    )
    const alert = await within(d).findByRole('alert')
    expect(alert.textContent).toBe(en('api-session-not-administrator'))
    expect(screen.getByRole('dialog', { name: en('signin-title') })).toBe(d)
    expect(pass.value).toBe('')
    expect(user.value).toBe('bob')
    expect(document.activeElement).toBe(user)
    // The page keeps its edit.
    expect(port.value).toBe('8581')
    // Cancel closes the dialog.
    fireEvent.click(
      within(d).getByRole('button', { name: en('signin-cancel') }),
    )
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(bff.to('POST', '/api/session')).toHaveLength(1)
  } finally {
    dialogs.restore()
  }
})

// ── The form ────────────────────────────────────────────────────────────

test('1a.9: the form has its heading, intro, labelled fields with autocomplete and a submit button', async () => {
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const { main, username, password, submit } = await form()
  expect(
    within(main).getByRole('heading', { level: 1, name: en('signin-title') }),
  ).toBeTruthy()
  expect(within(main).getByText(en('signin-intro'))).toBeTruthy()
  expect(username.getAttribute('autocomplete')).toBe('username')
  expect(password.type).toBe('password')
  expect(password.getAttribute('autocomplete')).toBe('current-password')
  // One form holds both fields and a submit button, so Enter in either
  // field submits it (implicit submission).
  const element = username.closest('form')
  expect(element).not.toBeNull()
  expect(element!.contains(password)).toBe(true)
  expect(element!.contains(submit)).toBe(true)
  expect(submit.getAttribute('type') ?? 'submit').toBe('submit')
  // Paste is not blocked.
  expect(fireEvent.paste(username)).toBe(true)
  expect(fireEvent.paste(password)).toBe(true)
  // Not a protected page: no "sign in to see this page".
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
})

test('1a.9: submitting the form posts the user name and password', async () => {
  const bff = fakeBff({ session: 'signed-out' })
  render(<App />)
  const { username, password } = await form()
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.submit(username.closest('form')!)
  await waitFor(() => expect(bff.to('POST', '/api/session')).toHaveLength(1))
  expect(bff.to('POST', '/api/session')[0]!.body).toEqual({
    username: 'alice',
    password: 'hunter2',
  })
})

test('1a.10: a protected page shows the form in its place, with signin-required, keeping the route', async () => {
  startAt('#/instances/default')
  const bff = fakeBff({ session: 'signed-out' })
  render(<App />)
  const { main, username } = await form()
  const required = within(main).getByText(en('signin-required'))
  expect(
    required.compareDocumentPosition(username) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(window.location.hash).toBe('#/instances/default')
  expect(
    within(main).queryByRole('heading', { level: 2, name: en('server-title') }),
  ).toBeNull()
  expect(bff.calls.filter((c) => c.path.endsWith('/server'))).toHaveLength(0)
})

test('1a.12: while the request runs the button reads signin-busy and a second submit sends nothing', async () => {
  const pending = deferred()
  const bff = fakeBff({ session: 'signed-out', signIn: () => pending.promise })
  render(<App />)
  const { main, username, password, submit } = await form()
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const busy = await within(main).findByRole('button', {
    name: en('signin-busy'),
  })
  // Still enabled, so focus never drops (2.6).
  expect(busy).toBe(submit)
  expect((busy as HTMLButtonElement).disabled).toBe(false)
  expect(busy.getAttribute('aria-disabled')).toBeNull()
  expect(document.body.contains(busy)).toBe(true)
  fireEvent.click(busy)
  fireEvent.submit(username.closest('form')!)
  expect(bff.to('POST', '/api/session')).toHaveLength(1)
  await act(async () => {
    pending.resolve(reply(200, { user: 'alice' }))
  })
  await screen.findByRole('button', { name: en('session-sign-out') })
  expect(bff.to('POST', '/api/session')).toHaveLength(1)
})

test('1a.1, 13: a sign-in answered with {"user"} alone signs in: the banner names the user, no role word', async () => {
  const bff = fakeBff({
    session: 'signed-out',
    signIn: () => reply(200, { user: 'alice' }),
  })
  render(<App />)
  const { main, username, password, submit } = await form()
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const banner = screen.getByRole('banner')
  const signOut = await within(banner).findByRole('button', {
    name: en('session-sign-out'),
  })
  const text = banner.textContent ?? ''
  expect(text).toContain(en('session-signed-in-as', 'alice'))
  for (const word of ROLE_WORDS)
    expect(text, `no ${word} in the banner`).not.toContain(word)
  // The form is gone, the app landed on the Overview (13).
  expect(
    within(main).queryByRole('heading', { level: 1, name: en('signin-title') }),
  ).toBeNull()
  await screen.findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(bff.to('POST', '/api/session')).toHaveLength(1)
  expect(signOut).toBeTruthy()
})

test('1a.1: a sign-in answered with canConfigure still in the reply signs in the same way (an older bff sent it)', async () => {
  for (const canConfigure of [true, false]) {
    const who = String(canConfigure)
    fakeBff({
      session: 'signed-out',
      signIn: () => reply(200, { user: 'alice', canConfigure }),
    })
    render(<App />)
    const { username, password, submit } = await form()
    type(username, 'alice')
    type(password, 'hunter2')
    fireEvent.click(submit)
    const banner = screen.getByRole('banner')
    await within(banner).findByRole('button', {
      name: en('session-sign-out'),
    })
    const text = banner.textContent ?? ''
    expect(text, who).toContain(en('session-signed-in-as', 'alice'))
    for (const word of ROLE_WORDS)
      expect(text, `${who}: no ${word}`).not.toContain(word)
    await screen.findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    })
    cleanup()
    vi.unstubAllGlobals()
    // The sign-in moved the page to #/: the next reply renders at #/sign-in.
    startAt('#/sign-in')
  }
})

test('1a.13: signing in at #/sign-in updates the banner and nav and lands on the Overview', async () => {
  fakeBff({
    session: 'signed-out',
    signIn: () => reply(200, { user: 'bob' }),
  })
  render(<App />)
  const { username, password, submit } = await form()
  type(username, 'bob')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const banner = screen.getByRole('banner')
  await within(banner).findByRole('button', { name: en('session-sign-out') })
  const text = banner.textContent ?? ''
  expect(text).toContain(en('session-signed-in-as', 'bob'))
  for (const word of ROLE_WORDS)
    expect(text, `no ${word} in the banner`).not.toContain(word)
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(
    await within(nav).findByRole('list', { name: en('nav-instances') }),
  ).toBeTruthy()
  const h1 = await screen.findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  await waitFor(() => expect(document.activeElement).toBe(h1))
})

test('1a.13 (2.6): signed in, #/sign-in still shows the form, and signing in lands on the Overview', async () => {
  const bff = fakeBff({
    session: 'signed-in',
    signIn: () => reply(200, { user: 'bob' }),
  })
  render(<App />)
  const banner = screen.getByRole('banner')
  await within(banner).findByRole('button', { name: en('session-sign-out') })
  const { main, username, password, submit } = await form()
  expect(
    within(main).getByRole('heading', { level: 1, name: en('signin-title') }),
  ).toBeTruthy()
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  type(username, 'bob')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const h1 = await screen.findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(window.location.hash).toBe('#/')
  expect(banner.textContent).toContain(en('session-signed-in-as', 'bob'))
  expect(bff.to('POST', '/api/session')).toHaveLength(1)
})

test('1a.13: signing in on a protected page returns to it with focus on its h1', async () => {
  startAt('#/instances/default')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const { username, password, submit } = await form()
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const h1 = await screen.findByRole('heading', { level: 1, name: 'default' })
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(window.location.hash).toBe('#/instances/default')
  expect(screen.getByRole('banner').textContent).toContain(
    en('session-signed-in-as', 'alice'),
  )
})

// ── Sign out ────────────────────────────────────────────────────────────

test('1a.15: Sign out sends DELETE /api/session and shows the signed-out Overview whatever the answer', async () => {
  const answers: [string, () => Response | Promise<Response>][] = [
    ['204', () => reply(204)],
    ['401', () => reply(401, { key: 'session-required', args: [] })],
    ['500', () => reply(500, 'secret-detail')],
    ['network', () => Promise.reject(new TypeError('Failed to fetch'))],
  ]
  for (const [what, signOut] of answers) {
    startAt('#/instances/default')
    const bff = fakeBff({ session: 'signed-in', signOut })
    render(<App />)
    const out = await screen.findByRole('button', {
      name: en('session-sign-out'),
    })
    await screen.findByRole('heading', { level: 1, name: 'default' })
    fireEvent.click(out)
    await waitFor(() =>
      expect(bff.to('DELETE', '/api/session'), what).toHaveLength(1),
    )
    expect(
      await screen.findByRole('heading', {
        level: 1,
        name: en('overview-title'),
      }),
      what,
    ).toBeTruthy()
    expect(window.location.hash, what).toBe('#/')
    const banner = screen.getByRole('banner')
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
        within(banner).queryByRole('link', { name: en('session-sign-in') }),
      what,
    ).not.toBeNull()
    const nav = screen.getByRole('navigation', { name: en('nav-label') })
    expect(
      within(nav).queryByRole('list', { name: en('nav-instances') }),
      what,
    ).toBeNull()
    expect(document.body.textContent, what).not.toContain('secret-detail')
    cleanup()
    vi.unstubAllGlobals()
  }
})
