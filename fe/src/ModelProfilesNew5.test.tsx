// Brief 22, slice 22C2 (fe/briefs/22-model-sources.md, as the "22P/22C check
// (2026-11-26)" and the 2026-11-25 "22P check" amend it): the New model profile
// form's Hugging Face half, what it answers. This file is 22 P.10 with P.10a
// (each refusal in its place with the arguments the check gives, the 401's Sign
// in and the 403), 22 P.11 (an edit drops what was listed) and the Hugging Face
// halves of 22 C.4 (the pick a create needs) and C.6 (a 422 naming the
// Hugging Face field). The third choice, the repo field, Show quants, the
// listing that arrives and the create of the pick are ModelProfilesNew4; the
// form's shape, the file half and the checks of the other choices are
// ModelProfilesNew1, New2 and New3. 22 P.12 (the strings file) is `1a strings`
// in App.test.tsx.
//
// Every answer here is a reply of the contract (fe/briefs/api-contract.md
// 745-753): a status, and a body of `key` and `args`, answered by the fake
// bff's `hfFiles` route.
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
  HF_FILES,
  HF_REPO,
  before,
  en,
  fakeBff,
  hfFilesBody,
  isHidden,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

const CREATE = '/api/profiles/model'

const FILES = '/api/hf/files'

const QWEN = '/var/lib/llama.cpp/models/qwen/Qwen3-27B-Q8_0.gguf'

let dialogs: ReturnType<typeof stubDialog>

function modelProfile(id: string, enabled: boolean, model = ''): ModelProfile {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model }
}

/** The list behind the form: two profiles, so the copy radio is there. */
const LIST_ROWS: ModelProfile[] = [
  modelProfile('Qwen3-27B', true, QWEN),
  modelProfile('Old-Coder', false),
]

/** The fixture's default quant, the pick a listing opens on. */
const DEFAULT_ROW = HF_FILES.find((f) => f.default)!

beforeEach(() => {
  startAt('#/profiles/model')
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a dialog closing) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The app at #/profiles/model, the table loaded. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfiles: LIST_ROWS, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  return { bff, main }
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The create form, opened by the +. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(within(main).getByRole('button', { name: en('model-profiles-new') }))
  return within(main).findByRole('form', {
    name: en('model-profiles-new-title'),
  })
}

/** The create form, on the Hugging Face choice. */
async function openHf(main: HTMLElement): Promise<HTMLElement> {
  const form = await openForm(main)
  press(radioIn(form, 'model-source-hf'))
  await hfBox(form)
  return form
}

function boxIn(form: HTMLElement, label: string): HTMLInputElement {
  return within(form).getByRole('textbox', { name: label }) as HTMLInputElement
}

function buttonIn(form: HTMLElement, keyName: string): HTMLElement {
  return within(form).getByRole('button', { name: en(keyName) })
}

function radioIn(form: HTMLElement, keyName: string): HTMLInputElement {
  return within(form).getByRole('radio', {
    name: en(keyName),
  }) as HTMLInputElement
}

/** The repo input. */
function hfBox(form: HTMLElement): HTMLInputElement {
  return within(form).getByRole('textbox', {
    name: en('model-profile-hf-repo'),
  }) as HTMLInputElement
}

/** Show quants. */
function showButton(form: HTMLElement): HTMLElement {
  return buttonIn(form, 'hf-show-quants')
}

/** One text, whitespace collapsed. */
function text(el: Node | null | undefined): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The quants' fieldset, the one of the form whose legend is hf-quant-legend.
 *  A fieldset is a `group`, not a `radiogroup`, so it is read by its legend. */
function quantGroups(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll('fieldset')].filter(
    (f) => text(f.querySelector('legend')) === en('hf-quant-legend'),
  )
}

function quantGroup(form: HTMLElement): HTMLElement {
  const open = quantGroups(form).filter((f) => !isHidden(f))
  expect(open, 'one open fieldset legend hf-quant-legend').toHaveLength(1)
  return open[0]!
}

/** The quants' radios, in the document's order. */
function quantRadios(form: HTMLElement): HTMLInputElement[] {
  const group = quantGroup(form)
  return within(form)
    .getAllByRole('radio')
    .filter((r) => r.closest('fieldset') === group) as HTMLInputElement[]
}

/** The text of every element named by `el`'s `aria-describedby`. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** The listing reads the bff was asked for, in order. */
function hfReads(bff: ReturnType<typeof fakeBff>) {
  return bff.calls.filter((c) => c.path.startsWith(FILES))
}

/** The creates the bff was asked for, in order. */
function posts(bff: ReturnType<typeof fakeBff>) {
  return bff.to('POST', CREATE)
}

/** The status lines inside `root`, in the document's order. */
function statusLines(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('[role="status"]')]
}

/** The text of every alert inside `root`, whitespace collapsed. */
function alertTexts(root: HTMLElement): string[] {
  return [...root.querySelectorAll<HTMLElement>('[role="alert"]')].map(text)
}

/** A form on the Hugging Face choice, `value` in the repo field, Show quants
 *  pressed, its read answered by `answer`. */
async function shown(
  value: string,
  answer: () => Response | Promise<Response>,
  options: BffOptions = {},
) {
  const { bff, main } = await loaded({ hfFiles: answer, ...options })
  const form = await openHf(main)
  const box = hfBox(form)
  type(box, value)
  const button = showButton(form)
  press(button)
  return { bff, main, form, box, button }
}

/** A form on the Hugging Face choice with the listing read and a quant picked,
 *  Name typed, Create pressed, the create answered by `answer`. */
async function sentHf(
  answer: () => Response | Promise<Response>,
  row = DEFAULT_ROW,
) {
  const { bff, main } = await loaded({ modelProfileCreate: answer })
  const form = await openHf(main)
  type(hfBox(form), HF_REPO)
  press(showButton(form))
  await waitFor(() => expect(quantRadios(form).length).toBe(HF_FILES.length))
  press(quantRadios(form)[HF_FILES.indexOf(row)]!)
  type(boxIn(form, en('model-profile-name')), 'New-Model')
  press(buttonIn(form, 'create-button'))
  return { bff, main, form, box: hfBox(form), row }
}

/** The form's Sign in, pressed; the Dialog filled and submitted, closed. */
async function signInDialog(form: HTMLElement) {
  press(within(form).getByRole('button', { name: en('session-sign-in') }))
  const d = await screen.findByRole('dialog', { name: en('signin-title') })
  type(within(d).getByRole('textbox', { name: en('signin-username') }), 'alice')
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
}

// ── 22 P.10, P.10a: the refusals ───────────────────────────────────────

test('22 P.10, P.10a: each refusal says its text in the alert with the arguments the check gives, keeps the field text, leaves the focus on the button and lists no quant; a 400 hf-repo-invalid is the field error instead, with the focus on the field', async () => {
  const cases: [string, () => Response | Promise<Response>, string][] = [
    [
      '404 hf-repo-unavailable',
      () => reply(404, { key: 'hf-repo-unavailable', args: [HF_REPO] }),
      en('api-hf-repo-unavailable', HF_REPO),
    ],
    [
      '502 hf-repo-moved',
      () => reply(502, { key: 'hf-repo-moved', args: [HF_REPO] }),
      en('api-hf-repo-moved', HF_REPO),
    ],
    [
      '504 hf-unreachable',
      () => reply(504, { key: 'hf-unreachable', args: [] }),
      en('api-hf-unreachable'),
    ],
    [
      '502 hf-bad-answer',
      () => reply(502, { key: 'hf-bad-answer', args: [] }),
      en('api-hf-bad-answer'),
    ],
    [
      '503 hf-tool-missing',
      () => reply(503, { key: 'hf-tool-missing', args: [] }),
      en('api-hf-tool-missing'),
    ],
    [
      '429 hf-busy',
      () => reply(429, { key: 'hf-busy', args: [] }),
      en('api-hf-busy'),
    ],
    [
      'a 400 with another key',
      () => reply(400, { key: 'hf-repo-malformed', args: [] }),
      en('request-failed'),
    ],
    [
      'a network failure',
      () => Promise.reject(new TypeError('no route')),
      en('request-failed'),
    ],
  ]
  for (const [what, answer, expected] of cases) {
    const { bff, form, box, button } = await shown(HF_REPO, answer)
    const alert = await within(form).findByText(expected)
    expect(
      alert.closest('[role="alert"]'),
      `${what}: the text is in a role="alert"`,
    ).not.toBeNull()
    expect(quantGroups(form), `${what}: no quant is listed`).toHaveLength(0)
    expect(box.value, `${what}: the field keeps its text`).toBe(HF_REPO)
    expect(
      document.activeElement,
      `${what}: the focus stays on the button`,
    ).toBe(button)
    expect(
      box.getAttribute('aria-invalid'),
      `${what}: the field is not marked`,
    ).toBeNull()
    expect(hfReads(bff), `${what}: read once`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
  // 400 hf-repo-invalid: the bff's check of the same rule, shown as P.8's.
  const { form, box, button } = await shown(HF_REPO, () =>
    reply(400, { key: 'hf-repo-invalid', args: [HF_REPO] }),
  )
  const error = await within(form).findByText(en('api-hf-repo-invalid'))
  expect(before(box, error), 'the text is under the field').toBe(true)
  expect(
    alertTexts(form),
    '400 hf-repo-invalid is not the alert',
  ).not.toContain(en('api-hf-repo-invalid'))
  expect(box.getAttribute('aria-invalid'), 'the field is marked').toBe('true')
  expect(described(box), 'described by its error').toContain(
    en('api-hf-repo-invalid'),
  )
  expect(document.activeElement, 'the focus is on the field').toBe(box)
  expect(button.getAttribute('aria-disabled'), 'the button is live').toBeNull()
})

test('22 P.10: a 401 says session-ended-retry with a Sign in after it; the Sign in opens the sign-in Dialog, and after a sign-in the focus returns to Show quants with nothing sent again; a 403 says session-revoked', async () => {
  const { bff, form, box, button } = await shown(HF_REPO, () =>
    reply(401, { key: 'session-required', args: [] }),
  )
  const alert = await within(form).findByText(en('session-ended-retry'))
  expect(
    alert.closest('[role="alert"]'),
    'the alert of the form',
  ).not.toBeNull()
  const signIns = within(form).getAllByRole('button', {
    name: en('session-sign-in'),
  })
  expect(signIns, 'one Sign in in the form').toHaveLength(1)
  expect(before(alert, signIns[0]!), 'the Sign in is after the alert').toBe(
    true,
  )
  press(signIns[0]!)
  const d = await screen.findByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), 'alice')
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
  expect(
    within(form).queryByText(en('session-ended-retry')),
    'the alert is gone',
  ).toBeNull()
  expect(box.value, 'the field keeps its text').toBe(HF_REPO)
  expect(document.activeElement, 'the focus returns to the button').toBe(button)
  expect(hfReads(bff), 'nothing is sent again').toHaveLength(1)
  cleanup()
  vi.unstubAllGlobals()
  // 403: the session ended.
  const second = await shown(HF_REPO, () =>
    reply(403, { key: 'session-not-administrator', args: [] }),
  )
  expect(
    await within(second.form).findByText(en('session-revoked')),
    'session-revoked is in the alert',
  ).toBeTruthy()
})

// ── 22 P.11: an edit drops what was listed ─────────────────────────────

test('22 P.11: editing the repo field removes the quants fieldset, the pick, the status text and the alert, silently and without moving the focus', async () => {
  const { bff, form, box, button } = await shown(HF_REPO, () =>
    reply(200, hfFilesBody()),
  )
  await waitFor(() => expect(quantRadios(form).length).toBe(HF_FILES.length))
  expect(
    quantRadios(form).filter((r) => r.checked).length,
    'a quant is picked',
  ).toBe(1)
  // The focus before the edit: Show quants kept it (22 P.9). A change event
  // moves no focus by itself, so P.11's "without moving the focus" is pinned
  // as the focus staying where it was, not landing on the field.
  const focus = document.activeElement
  expect(focus, 'the focus was on Show quants before the edit').toBe(button)
  type(box, `${HF_REPO}:Q2_K`)
  expect(
    quantGroups(form).filter((g) => !isHidden(g)),
    'the fieldset is gone',
  ).toHaveLength(0)
  for (const line of statusLines(form))
    expect(text(line), 'the status line says nothing').toBe('')
  expect(alertTexts(form), 'no alert').toEqual([])
  expect(document.activeElement, 'the edit moved no focus').toBe(focus)
  expect(hfReads(bff), 'the edit sent nothing').toHaveLength(1)
  expect(posts(bff), 'and no create').toHaveLength(0)
})

// ── 22 C.4, C.6: the Hugging Face halves ───────────────────────────────

test('22 C.4: a Hugging Face choice with no quant picked shows field-error-pick-quant under the repo field, marks and focuses it and sends nothing, also when the listing failed', async () => {
  for (const [what, answer] of [
    ['the listing read', () => reply(200, hfFilesBody())],
    ['the listing failed', () => reply(429, { key: 'hf-busy', args: [] })],
  ] as const) {
    const { bff, form, box } = await shown(HF_REPO, answer)
    if (what === 'the listing read') {
      await waitFor(() =>
        expect(quantRadios(form).length).toBe(HF_FILES.length),
      )
      // An edit drops the pick (22 P.11), so the choice has no value.
      type(box, HF_REPO)
    }
    type(boxIn(form, en('model-profile-name')), 'New-Model')
    press(buttonIn(form, 'create-button'))
    const expected = en('field-error-pick-quant')
    const error = await within(form).findByText(expected)
    expect(before(box, error), `${what}: under the repo field`).toBe(true)
    expect(
      box.getAttribute('aria-invalid'),
      `${what}: the field is marked`,
    ).toBe('true')
    expect(described(box), `${what}: described by it`).toContain(expected)
    expect(document.activeElement, `${what}: the focus is on the field`).toBe(
      box,
    )
    expect(
      boxIn(form, en('model-profile-name')).getAttribute('aria-invalid'),
      `${what}: Name is not marked`,
    ).toBeNull()
    expect(hfReads(bff), `${what}: the listing was read`).toHaveLength(1)
    expect(posts(bff), `${what}: nothing sent`).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('22 C.6: a 422 with field hf-repo, and one with field hf-file, show their text under the repo field, mark and focus it, and keep the listing and the pick', async () => {
  for (const field of ['hf-repo', 'hf-file'] as const) {
    const value = DEFAULT_ROW.value['hf-repo']
    const { form, box } = await sentHf(() =>
      reply(422, { key: 'comment-markers', args: [value], field }),
    )
    const expected = en('api-comment-markers', value)
    const error = await within(form).findByText(expected)
    expect(before(box, error), `field ${field}: under the repo field`).toBe(
      true,
    )
    expect(box.getAttribute('aria-invalid'), `field ${field}: marked`).toBe(
      'true',
    )
    expect(described(box), `field ${field}: described by it`).toContain(
      expected,
    )
    expect(document.activeElement, `field ${field}: the focus`).toBe(box)
    expect(box.value, `field ${field}: the pick stays`).toBe(value)
    expect(
      quantRadios(form).filter((r) => r.checked).length,
      `field ${field}: the pick stays listed`,
    ).toBe(1)
    expect(
      boxIn(form, en('model-profile-name')).value,
      `field ${field}: Name keeps what was typed`,
    ).toBe('New-Model')
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 22 C.7: where a sign-in returns the focus, after another read asked ──

test('22 C.7: a sign-in after a Create 401 returns the focus to Create, also when a listing 401 asked for a sign-in first (22C2 review: the Create 401 focus)', async () => {
  // The listing answers 401 the first time and 200 the second, so the form
  // carries a listing 401 sign-in before the create's own.
  let reads = 0
  const { bff, main } = await loaded({
    hfFiles: () =>
      ++reads === 1
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, hfFilesBody()),
    modelProfileCreate: () => reply(401, { key: 'session-required', args: [] }),
  })
  const form = await openHf(main)
  const box = hfBox(form)
  type(box, HF_REPO)
  const button = showButton(form)
  press(button)
  // The listing's 401: Sign in, signed in, the focus on Show quants (22 P.10).
  await within(form).findByText(en('session-ended-retry'))
  await signInDialog(form)
  expect(
    document.activeElement,
    'the listing sign-in: the focus on Show quants',
  ).toBe(button)
  // The listing read again, a quant picked, the create sent, and 401.
  press(button)
  await waitFor(() => expect(quantRadios(form).length).toBe(HF_FILES.length))
  press(quantRadios(form)[HF_FILES.indexOf(DEFAULT_ROW)]!)
  type(boxIn(form, en('model-profile-name')), 'New-Model')
  const create = buttonIn(form, 'create-button')
  press(create)
  const alert = await within(form).findByText(en('session-ended-retry'))
  expect(
    alert.closest('[role="alert"]'),
    'the create 401 is the form alert',
  ).not.toBeNull()
  expect(hfReads(bff), 'the listing was read twice').toHaveLength(2)
  expect(posts(bff), 'the create was sent once').toHaveLength(1)
  // The sign-in: the focus goes to the Create button, not to Show quants.
  await signInDialog(form)
  expect(
    document.activeElement,
    'the create sign-in: the focus on Create',
  ).toBe(create)
  expect(
    within(form).queryByText(en('session-ended-retry')),
    'the alert is gone',
  ).toBeNull()
  expect(
    box.value,
    "the field holds the picked row's hf-repo, tag included, as after the pick (22 P.9, 22C2 review A3)",
  ).toBe(DEFAULT_ROW.value['hf-repo'])
  expect(
    quantRadios(form).filter((r) => r.checked).length,
    'the pick stays listed',
  ).toBe(1)
  expect(
    boxIn(form, en('model-profile-name')).value,
    'Name keeps what was typed',
  ).toBe('New-Model')
  expect(posts(bff), 'nothing is sent again').toHaveLength(1)
  expect(hfReads(bff), 'the listing is not read again').toHaveLength(2)
})

test('22 C.7: a listing 401 never signed in, then a Create 401 signed in, returns the focus to Create (22C2 review: the Create 401 focus)', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    hfFiles: () =>
      ++reads === 1
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, hfFilesBody()),
    modelProfileCreate: () => reply(401, { key: 'session-required', args: [] }),
  })
  const form = await openHf(main)
  type(hfBox(form), HF_REPO)
  const button = showButton(form)
  press(button)
  // The listing's 401, left there: its Sign in is never pressed.
  await within(form).findByText(en('session-ended-retry'))
  expect(
    within(form).getAllByRole('button', { name: en('session-sign-in') }),
    'the listing ask offers its Sign in',
  ).toHaveLength(1)
  // The listing read again (200), a quant picked, the create sent, and 401.
  press(button)
  await waitFor(() => expect(quantRadios(form).length).toBe(HF_FILES.length))
  press(quantRadios(form)[HF_FILES.indexOf(DEFAULT_ROW)]!)
  type(boxIn(form, en('model-profile-name')), 'New-Model')
  const create = buttonIn(form, 'create-button')
  press(create)
  await within(form).findByText(en('session-ended-retry'))
  expect(hfReads(bff), 'the listing was read twice').toHaveLength(2)
  expect(posts(bff), 'the create was sent once').toHaveLength(1)
  await signInDialog(form)
  expect(
    document.activeElement,
    'the create sign-in: the focus on Create',
  ).toBe(create)
  expect(posts(bff), 'nothing is sent again').toHaveLength(1)
})
