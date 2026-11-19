// The plain-HTTP sign-in notice (fe/briefs/03-insecure-sign-in.md): when
// window.isSecureContext is false the sign-in says the password crosses the
// network in clear, on the page (I1) and in the dialog of a session that ended
// (I2), without blocking anything (I3), as a plain paragraph (I4) read with the
// password field (I5), and nowhere else (I7); true or absent keeps the sign-in
// as it is (I8). "I1" is the brief's criterion. The whole app renders against a
// fake bff (mocked fetch). jsdom does not define isSecureContext, so each test
// sets it and the afterEach removes it (I9).
import {
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
  before,
  en,
  fakeBff,
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
} from './test/bff.ts'

/** The notice's text, as the brief gives it. */
const NOTICE = en('signin-insecure')

let dialogs: ReturnType<typeof stubDialog>

/** Sets the browser's `window.isSecureContext`; `undefined` removes it, as jsdom has it. */
function secure(value: boolean | undefined) {
  const w = window as unknown as { isSecureContext?: boolean }
  if (value === undefined) delete w.isSecureContext
  else
    Object.defineProperty(window, 'isSecureContext', {
      value,
      configurable: true,
    })
}

beforeEach(() => {
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  secure(undefined)
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

/** The sign-in form shown in `scope`. */
async function form(scope: HTMLElement) {
  const username = (await within(scope).findByRole('textbox', {
    name: en('signin-username'),
  })) as HTMLInputElement
  const password = within(scope).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  const submit = within(scope).getByRole('button', {
    name: en('signin-submit'),
  }) as HTMLButtonElement
  return { username, password, submit }
}

/** The notice paragraph inside `scope`. */
function notice(scope: HTMLElement): HTMLElement {
  return within(scope).getByText(NOTICE)
}

/** The text of what `input`'s aria-describedby names, in the order it names it. */
function described(input: HTMLElement): string[] {
  return (input.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => {
      const el = document.getElementById(id)
      expect(
        el,
        `aria-describedby names ${id}, which is not in the document`,
      ).not.toBeNull()
      return el!.textContent ?? ''
    })
}

function type(input: HTMLInputElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

// ── Failure paths first: the notice is missing where it must show ─────────

test('I1, I4, I6: insecure at #/sign-in, the notice is a plain paragraph after the intro and before the form, unannounced, with its lock-open icon of H8b.3', async () => {
  secure(false)
  startAt('#/sign-in')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  const { username, password } = await form(main)
  const p = notice(main)
  const intro = within(main).getByText(en('signin-intro'))
  const formEl = username.closest('form')!
  expect(before(intro, p), 'after the intro').toBe(true)
  expect(before(p, formEl), 'before the form').toBe(true)
  expect(before(p, password), 'before the password field').toBe(true)
  expect(p.closest('form'), 'the paragraph is outside the form').toBeNull()
  // I4: static content, not a status message.
  expect(p.tagName, 'a plain paragraph').toBe('P')
  expect(p.getAttribute('role'), 'no role of its own').toBeNull()
  expect(p.closest('[role="alert"], [role="status"], [aria-live]')).toBeNull()
  // H8b.3: the notice holds its icon (brief 08): the shared decorative
  // lock-open svg, the first child, no image and no text of its own. The
  // shape of the drawing is pinned in HearthIconsPages.test.tsx.
  expect(p.querySelector('img'), 'no image').toBeNull()
  const icon = p.querySelector('svg')
  expect(icon, 'the icon of H8b.3').toBeTruthy()
  expect(icon!.getAttribute('class'), 'the shared icon class').toBe('icon')
  expect(icon!.getAttribute('aria-hidden'), 'decorative').toBe('true')
  expect(icon!.getAttribute('focusable'), 'takes no tab place').toBe('false')
  expect(p.firstElementChild, 'the icon is the notice first child').toBe(icon)
  expect(p.querySelectorAll('svg').length, 'one icon in the notice').toBe(1)
  expect(p.textContent, 'the icon adds no text').toBe(en('signin-insecure'))
  // I6: the existing Notice of D5.2.
  expect(p.classList.contains('notice'), 'the .notice look').toBe(true)
})

test('I1: insecure on a protected page signed out, the notice follows the signin-required Notice and precedes the form', async () => {
  secure(false)
  startAt('#/instances/default')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  const { username } = await form(main)
  const required = within(main).getByText(en('signin-required'))
  const p = notice(main)
  expect(before(required, p), 'after the signin-required Notice').toBe(true)
  expect(before(p, username.closest('form')!), 'before the form').toBe(true)
})

test('I5: insecure, the password field is described by the notice alone, the user name field is not', async () => {
  secure(false)
  startAt('#/sign-in')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  const { username, password } = await form(main)
  expect(notice(main).id, 'the paragraph has an id').not.toBe('')
  expect(described(password), 'the notice is the description').toEqual([NOTICE])
  expect(described(username), 'the user name is not described by it').toEqual(
    [],
  )
})

test('I5: insecure, after an empty submit the password is described by the error first, then by the notice', async () => {
  secure(false)
  startAt('#/sign-in')
  const bff = fakeBff({ session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  const { username, password, submit } = await form(main)
  type(username, 'alice')
  fireEvent.click(submit)
  await within(main).findByText(en('signin-empty-password'))
  expect(described(password)).toEqual([en('signin-empty-password'), NOTICE])
  expect(
    bff.to('POST', '/api/session'),
    'the empty check sends nothing',
  ).toHaveLength(0)
})

test('I3: insecure, nothing is disabled: filling both fields sends POST /api/session and signs in', async () => {
  secure(false)
  startAt('#/sign-in')
  const bff = fakeBff({
    session: 'signed-out',
    signIn: () => reply(200, { user: 'alice' }),
  })
  render(<App />)
  const main = screen.getByRole('main')
  const { username, password, submit } = await form(main)
  expect(notice(main), 'the notice is shown').toBeTruthy()
  expect(username.disabled, 'the user name is enabled').toBe(false)
  expect(password.disabled, 'the password is enabled').toBe(false)
  expect(submit.disabled, 'Sign in is enabled').toBe(false)
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  await within(screen.getByRole('banner')).findByRole('button', {
    name: en('session-sign-out'),
  })
  expect(bff.to('POST', '/api/session')[0]!.body).toEqual({
    username: 'alice',
    password: 'hunter2',
  })
})

test('I2, I3: insecure, the dialog of a save answered 401 holds the notice after its intro and before its form; its fields and Cancel work', async () => {
  secure(false)
  startAt('#/profiles/model/Qwen3-27B')
  const bff = fakeBff({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc(
          {},
          { revision: 'r1', values: { embeddings: 'true', pooling: 'mean' } },
        ),
      ),
    modelProfileSave: () => reply(401, { key: 'session-required', args: [] }),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: 'Qwen3-27B' })
  const mode = (await within(main).findByRole('combobox', {
    name: en('model-profile-embedding-mode'),
  })) as HTMLSelectElement
  fireEvent.change(mode, { target: { value: 'false' } })
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await waitFor(() =>
    expect(bff.to('PUT', '/api/profiles/model/Qwen3-27B')).toHaveLength(1),
  )
  fireEvent.click(
    await within(main).findByRole('button', { name: en('session-sign-in') }),
  )
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  const p = notice(d)
  const { username, password, submit } = await form(d)
  expect(
    within(d).getByText(en('signin-intro')),
    'the dialog keeps its intro',
  ).toBeTruthy()
  expect(
    before(within(d).getByText(en('signin-intro')), p),
    'after the intro',
  ).toBe(true)
  expect(before(p, username.closest('form')!), 'before the form').toBe(true)
  expect(described(password), 'described by the notice').toEqual([NOTICE])
  // I3 in the dialog: the fields, Sign in and Cancel are all usable.
  expect(username.disabled).toBe(false)
  expect(password.disabled).toBe(false)
  expect(submit.disabled).toBe(false)
  const cancel = within(d).getByRole('button', {
    name: en('signin-cancel'),
  }) as HTMLButtonElement
  expect(cancel.disabled, 'Cancel is enabled').toBe(false)
  fireEvent.click(cancel)
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(bff.to('POST', '/api/session'), 'Cancel sent nothing').toHaveLength(0)
})

test('I7: insecure, the Overview signed out and the banner show no notice', async () => {
  secure(false)
  startAt('#/')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(
    within(main).queryByText(NOTICE),
    'no notice on the Overview',
  ).toBeNull()
  expect(document.body.textContent).not.toContain(NOTICE)
})

test('I8: isSecureContext true, and the property absent: no notice, and no description on the password', async () => {
  for (const value of [true, undefined]) {
    secure(value)
    expect(window.isSecureContext, `the property is ${String(value)}`).toBe(
      value,
    )
    startAt('#/sign-in')
    fakeBff({ session: 'signed-out' })
    render(<App />)
    const main = screen.getByRole('main')
    const { username, password } = await form(main)
    expect(
      within(main).queryByText(NOTICE),
      `no notice when isSecureContext is ${String(value)}`,
    ).toBeNull()
    expect(described(password), 'the password is undescribed').toEqual([])
    expect(described(username), 'the user name is undescribed').toEqual([])
    cleanup()
    vi.unstubAllGlobals()
  }
})
