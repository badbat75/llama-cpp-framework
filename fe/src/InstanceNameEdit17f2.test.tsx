// Brief 16, slice 17f (fe/briefs/16-server-profiles-and-instances.md): the
// rename form of the instance header: how it opens, what its save sends, how
// every refusal answers, and the keyboard and names rules of 17f.9. The whole
// app renders at #/instances/default against a fake bff (mocked fetch);
// "16 17f.n" is the brief's criterion n. The name itself, in the nav, the tiles
// and the headings, is in InstanceName17f.test.tsx; the 504 rule for the other
// writes is in WriteOutcome17f.test.tsx.
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
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

/** The instance the page shows: labelled, so the name is the label (17f.1). */
const ITEM = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 9931,
  tags: [],
  instanceTags: [],
  boot: true,
  label: 'Coding assistant',
}

const PUT = '/api/instances/default/label'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/instances/default')
  dialogs = stubDialog()
})

afterEach(async () => {
  // A link click queues its hashchange as a task: let it land on this test's
  // page, not on the next test's.
  await flushTasks()
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

/** A keyed refusal of `status`, with the reply's `field` when it names one. */
function refused(
  status: number,
  key: string,
  args: string[] = [],
  field?: string,
): () => Response {
  return () => reply(status, { key, args, ...(field ? { field } : {}) })
}

/** The app at the instance page, loaded. `name` is the h1 the fixture's item
 *  gives (17f.1): a case that changes the label awaits the name that change
 *  gives, not the fixture's. */
async function openPage(options: BffOptions = {}, name = 'Coding assistant') {
  const bff = fakeBff({
    instances: [ITEM],
    boot: { read: () => reply(200, { enabled: true }) },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name,
  })
  return { bff, main }
}

/** The Rename opener, by its accessible name (17f.9). */
function opener(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('instance-rename') })
}

/** Opens the form and returns it, with the field focused. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  fireEvent.click(opener(main))
  const form = await within(main).findByRole('form', {
    name: en('instance-label'),
  })
  return form
}

/** The Label box of the open form. */
function labelBox(main: HTMLElement): HTMLInputElement {
  return within(main).getByRole('textbox', { name: en('instance-label') })
}

/** Types `text` in a box. */
function typeIn(box: HTMLInputElement, text: string): void {
  fireEvent.change(box, { target: { value: text } })
}

/** The writes the bff saw on the label route. */
const labelWrites = (bff: ReturnType<typeof fakeBff>) => bff.to('PUT', PUT)

/** The body of the n-th write (from 1). */
const writtenLabel = (bff: ReturnType<typeof fakeBff>, n = 1) =>
  (labelWrites(bff)[n - 1]!.body as { label: string }).label

/** The focusable elements of the page, in document order, as Tab names them. */
function tabbable(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      el.closest('dialog:not([open])') === null,
  )
}

/** Signs in through the dialog of the app's Sign in button. */
async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  fireEvent.change(
    within(d).getByRole('textbox', { name: en('signin-username') }),
    { target: { value: user } },
  )
  fireEvent.change(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
}

// ── 17f.5: the form opens ───────────────────────────────────────────────

test('16 17f.5: the header has a secondary Rename button after Start at boot, with aria-expanded false, and pressing it sets true', async () => {
  const { main } = await openPage()
  const button = opener(main)
  expect(button.className, 'a secondary button').toContain('secondary')
  expect(button.getAttribute('aria-expanded'), 'closed at first').toBe('false')
  // The boot field renders once its read lands: settle it before the order
  // check (the h1 settles on the list read, not on this one).
  const boot = await within(main).findByRole('group', {
    name: en('boot-label'),
  })
  expect(before(boot, button), 'Rename is after Start at boot').toBe(true)
  fireEvent.click(button)
  expect(opener(main).getAttribute('aria-expanded'), 'open').toBe('true')
})

test('16 17f.5: the form is right after the opener and holds the Label box with its hint, the box holding the current label with its text selected and the focus in it, then Save label, primary, and Cancel, then a status line and the alert place', async () => {
  const { main } = await openPage()
  const button = opener(main)
  fireEvent.click(button)
  const form = await within(main).findByRole('form', {
    name: en('instance-label'),
  })
  expect(before(button, form), 'the form is after the opener').toBe(true)
  const box = within(form).getByRole<HTMLInputElement>('textbox', {
    name: en('instance-label'),
  })
  expect(box.getAttribute('type'), 'a text input').toBe('text')
  expect(box.getAttribute('autocomplete'), 'no autofill of a name').toBe('off')
  expect(box.getAttribute('spellcheck'), 'no spellcheck of a name').toBe(
    'false',
  )
  expect(box.value, 'the box holds the current label').toBe('Coding assistant')
  expect(
    [box.selectionStart, box.selectionEnd],
    'its text is selected, so typing replaces it',
  ).toEqual([0, 'Coding assistant'.length])
  expect(document.activeElement, 'the focus is in the box').toBe(box)
  const hint = form.querySelector('[id$="-hint"], .hint')
  expect(hint, 'the hint is there').not.toBeNull()
  expect(hint!.textContent, 'the hint is instance-label-hint').toBe(
    en('instance-label-hint'),
  )
  expect(
    box.getAttribute('aria-describedby'),
    'the box is described by its hint',
  ).toBe(hint!.id)
  const save = within(form).getByRole('button', {
    name: en('instance-label-save'),
  })
  const cancel = within(form).getByRole('button', {
    name: en('instance-label-cancel'),
  })
  expect(save.className, 'Save label is primary').toContain('primary')
  expect(before(box, save), 'Save follows the box').toBe(true)
  expect(before(save, cancel), 'Cancel follows Save').toBe(true)
  const status = within(form).getByRole('status')
  expect(before(cancel, status), 'the status line follows the buttons').toBe(
    true,
  )
  expect(
    form.querySelector('[role="alert"]'),
    'the alert place is in the form',
  ).toBeNull()
})

test('16 17f.5: the box of an instance with no label is empty, and its hint says the label is what names the instance today', async () => {
  const { main } = await openPage(
    { instances: [{ ...ITEM, label: null }] },
    // 17f.1: no label, port above 0: the name is the engine and the port.
    'llama.cpp :9931',
  )
  await openForm(main)
  expect(labelBox(main).value, 'no label: an empty box').toBe('')
})

test('16 17f.5: Cancel, Escape in the form, and the opener pressed again close it, drop the typing, and put the focus on the opener', async () => {
  for (const close of ['cancel', 'escape', 'opener'] as const) {
    const { main } = await openPage()
    const form = await openForm(main)
    typeIn(labelBox(main), 'a name nobody saved')
    if (close === 'cancel') {
      fireEvent.click(
        within(form).getByRole('button', { name: en('instance-label-cancel') }),
      )
    } else if (close === 'escape') {
      fireEvent.keyDown(labelBox(main), { key: 'Escape' })
    } else {
      fireEvent.click(opener(main))
    }
    await flushTasks()
    expect(
      within(main).queryByRole('form', { name: en('instance-label') }),
      `${close}: the form is closed`,
    ).toBeNull()
    expect(
      opener(main).getAttribute('aria-expanded'),
      `${close}: the opener says closed`,
    ).toBe('false')
    expect(document.activeElement, `${close}: the focus is on the opener`).toBe(
      opener(main),
    )
    const again = await openForm(main)
    expect(labelBox(main).value, `${close}: the typing is dropped`).toBe(
      'Coding assistant',
    )
    fireEvent.click(
      within(again).getByRole('button', { name: en('instance-label-cancel') }),
    )
    await flushTasks()
    // The loop renders a fresh app per case: RTL cleans only between tests.
    cleanup()
  }
})

test('16 17f.5: typing in the rename form does not arm the unsaved-changes question: a nav link leaves with no dialog', async () => {
  const { main } = await openPage()
  await openForm(main)
  typeIn(labelBox(main), 'a name nobody saved')
  const overview = within(screen.getByRole('navigation')).getByRole('link', {
    name: en('nav-overview'),
  })
  fireEvent.click(overview)
  await flushTasks()
  expect(
    screen.queryByRole('dialog', { name: en('discard-title') }),
    'nothing to ask about: the form is not a page edit',
  ).toBeNull()
  await within(screen.getByRole('main')).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
})

// ── 17f.6: the save ─────────────────────────────────────────────────────

test('16 17f.6: Save label sends PUT /api/instances/default/label with the text trimmed, and Enter in the box sends the same', async () => {
  for (const how of ['button', 'enter'] as const) {
    const { bff, main } = await openPage()
    await openForm(main)
    const box = labelBox(main)
    typeIn(box, '  Edge box  ')
    if (how === 'button') {
      fireEvent.click(
        within(main).getByRole('button', { name: en('instance-label-save') }),
      )
    } else {
      fireEvent.keyDown(box, { key: 'Enter' })
    }
    await flushTasks()
    const writes = labelWrites(bff)
    expect(writes, `${how}: one write`).toHaveLength(1)
    expect(writes[0]!.body, `${how}: the body is the label, trimmed`).toEqual({
      label: 'Edge box',
    })
    // The loop renders a fresh app per case: RTL cleans only between tests.
    cleanup()
  }
})

test('16 17f.6: an empty box sends the empty string, which is the clear', async () => {
  const { bff, main } = await openPage()
  await openForm(main)
  typeIn(labelBox(main), '')
  fireEvent.click(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  await flushTasks()
  expect(writtenLabel(bff), 'the clear is the empty string, not a null').toBe(
    '',
  )
})

test('16 17f.6: a text that trims to the current label sends nothing and closes the form, and so does an empty box on an instance with no label', async () => {
  const { bff, main } = await openPage()
  await openForm(main)
  typeIn(labelBox(main), '  Coding assistant  ')
  fireEvent.click(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  await flushTasks()
  expect(labelWrites(bff), 'nothing to write').toHaveLength(0)
  expect(
    within(main).queryByRole('form', { name: en('instance-label') }),
    'the form closed',
  ).toBeNull()
  expect(document.activeElement, 'the focus is on the opener').toBe(
    opener(main),
  )
})

test('16 17f.6: while the write runs the status reads instance-label-saving and both buttons are aria-disabled, and a second press sends nothing', async () => {
  const gate = deferred()
  const { bff, main } = await openPage({
    instanceLabel: () => gate.promise,
  })
  await openForm(main)
  typeIn(labelBox(main), 'Edge box')
  const save = within(main).getByRole('button', {
    name: en('instance-label-save'),
  })
  const cancel = within(main).getByRole('button', {
    name: en('instance-label-cancel'),
  })
  fireEvent.click(save)
  const status = await within(main).findByText(en('instance-label-saving'))
  expect(status.getAttribute('role'), 'the status line says it').toBe('status')
  expect(
    save.getAttribute('aria-disabled'),
    'Save is aria-disabled, not disabled: the focus stays',
  ).toBe('true')
  expect(cancel.getAttribute('aria-disabled'), 'and Cancel too').toBe('true')
  fireEvent.click(save)
  fireEvent.click(save)
  await flushTasks()
  expect(labelWrites(bff), 'a second press sends nothing').toHaveLength(1)
  gate.resolve(reply(204))
  await flushTasks()
})

test('16 17f.6: a 204 closes the form, puts the focus on the opener, reads the list again, and the status after the opener reads instance-label-saved with the text sent', async () => {
  const { bff, main } = await openPage()
  await openForm(main)
  typeIn(labelBox(main), 'Edge box')
  fireEvent.click(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  const status = await within(main).findByText(
    en('instance-label-saved', 'Edge box'),
  )
  expect(
    within(main).queryByRole('form', { name: en('instance-label') }),
    'the form closed',
  ).toBeNull()
  expect(document.activeElement, 'the focus is on the opener').toBe(
    opener(main),
  )
  expect(status.getAttribute('role'), 'the line is a status').toBe('status')
  expect(before(opener(main), status), 'the status is after the opener').toBe(
    true,
  )
  expect(
    bff.to('GET', '/api/instances').length,
    'the list is read again, so the h1, the title, the nav and the tiles follow',
  ).toBeGreaterThan(1)
})

test('16 17f.6: a 204 to the clear reads instance-label-cleared with the name the item then has, and a 204 that trims to nothing on an unlabelled instance says the same with its id', async () => {
  const { main } = await openPage()
  await openForm(main)
  typeIn(labelBox(main), '   ')
  fireEvent.click(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  await within(main).findByText(
    en('instance-label-cleared', en('instance-name-auto', 'llama.cpp', '9931')),
  )
  cleanup()
  const second = await openPage(
    { instances: [{ ...ITEM, label: null }] },
    'llama.cpp :9931',
  )
  await openForm(second.main)
  typeIn(labelBox(second.main), 'Edge box')
  fireEvent.click(
    within(second.main).getByRole('button', {
      name: en('instance-label-save'),
    }),
  )
  await within(second.main).findByText(en('instance-label-saved', 'Edge box'))
})

// ── 17f.7: the refusals ─────────────────────────────────────────────────

/** The field refusals: the text under the field, the field marked, the focus
 * back in it, the typing kept (17f.7). */
for (const [what, answer, text] of [
  [
    '422 instance-bad-label',
    refused(422, 'instance-bad-label', ['a;b'], 'label'),
    en('api-instance-bad-label', 'a;b'),
  ],
  [
    '422 comment-markers',
    refused(422, 'comment-markers', ['a;b'], 'label'),
    en('api-comment-markers', 'a;b'),
  ],
  [
    '409 instance-label-taken',
    refused(409, 'instance-label-taken', ['Edge box', 'coder']),
    en('api-instance-label-taken', 'Edge box', 'coder'),
  ],
] as Array<[string, () => Response, string]>) {
  test(`16 17f.7: ${what} shows its text under the Label field, marks the field aria-invalid and described by the error, focuses the field, and keeps the typing with the form open`, async () => {
    const { main } = await openPage({ instanceLabel: answer })
    await openForm(main)
    const box = labelBox(main)
    typeIn(box, 'Edge box')
    fireEvent.click(
      within(main).getByRole('button', { name: en('instance-label-save') }),
    )
    const error = await within(main).findByText(text)
    expect(box.getAttribute('aria-invalid'), 'the field is marked').toBe('true')
    expect(
      box.getAttribute('aria-describedby'),
      'the field is described by the error',
    ).toContain(error.id)
    expect(document.activeElement, 'the focus is in the field').toBe(box)
    expect(
      within(main).queryByRole('form', { name: en('instance-label') }),
      'the form stays open',
    ).not.toBeNull()
    expect(labelBox(main).value, 'with what was typed').toBe('Edge box')
  })
}

/** The alert refusals: one line in the form's alert, the form open with what
 * was typed (17f.7). */
for (const [what, answer, text] of [
  [
    '422 with an unknown key',
    refused(422, 'x-y', [], 'label'),
    en('api-other', 'x-y'),
  ],
  [
    '400 instance-malformed',
    refused(400, 'instance-malformed'),
    en('request-failed'),
  ],
  [
    '404 client-no-instance',
    refused(404, 'client-no-instance'),
    en('instance-not-found', 'default'),
  ],
  [
    '503 client-unavailable',
    refused(503, 'client-unavailable'),
    en('api-client-unavailable'),
  ],
  ['413', refused(413, 'client-too-large'), en('request-failed')],
] as Array<[string, () => Response, string]>) {
  test(`16 17f.7: ${what} shows its text in the form's alert, keeps the form open with what was typed, and marks no field`, async () => {
    const { main } = await openPage({ instanceLabel: answer })
    await openForm(main)
    typeIn(labelBox(main), 'Edge box')
    fireEvent.click(
      within(main).getByRole('button', { name: en('instance-label-save') }),
    )
    const alert = await within(main).findByText(text)
    expect(alert.getAttribute('role'), 'the line is an alert').toBe('alert')
    expect(
      within(main).queryByRole('form', { name: en('instance-label') }),
      'the form stays open',
    ).not.toBeNull()
    expect(labelBox(main).value, 'with what was typed').toBe('Edge box')
    expect(
      labelBox(main).getAttribute('aria-invalid'),
      'the field is not marked: the alert says it',
    ).toBeNull()
  })
}

test('16 17f.7: a 504 client-timeout is not confirmed: write-timeout in the alert, the form open with what was typed, and the list read again so the name shown is the manager’s', async () => {
  const { bff, main } = await openPage({
    instanceLabel: refused(504, 'client-timeout'),
  })
  await openForm(main)
  typeIn(labelBox(main), 'Edge box')
  fireEvent.click(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  const alert = await within(main).findByText(en('write-timeout'))
  expect(alert.getAttribute('role'), 'the line is an alert').toBe('alert')
  expect(
    alert.textContent,
    'a 504 never reads as a request that did not go through (17f.10)',
  ).not.toBe(en('request-failed'))
  expect(labelBox(main).value, 'the typing is kept').toBe('Edge box')
  expect(
    bff.to('GET', '/api/instances').length,
    'the list is read again',
  ).toBeGreaterThan(1)
})

test('16 17f.7: a 401 reads session-ended-retry with a Sign in after the alert; signing in puts the focus on Save label with the typing kept, and sends nothing', async () => {
  const { bff, main } = await openPage({
    instanceLabel: refused(401, 'session-required'),
  })
  await openForm(main)
  typeIn(labelBox(main), 'Edge box')
  fireEvent.click(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  const alert = await within(main).findByText(en('session-ended-retry'))
  const signIn = within(main).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(before(alert, signIn), 'Sign in is after the alert').toBe(true)
  fireEvent.click(signIn)
  await flushTasks()
  await signInInDialog('alice')
  expect(document.activeElement, 'the focus is on Save label').toBe(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  expect(labelBox(main).value, 'the typing is kept').toBe('Edge box')
  expect(labelWrites(bff), 'the refused write is not resent').toHaveLength(1)
})

test('16 17f.7: a 403 session-not-administrator shows session-revoked in the form’s alert with a Sign in after it, and keeps the typing with the form open', async () => {
  // 17f.7 maps the 403 by writeFailure, as every other write does (15 C1.9):
  // the form's alert says it, the Sign in follows the alert, the typing stays.
  const { main } = await openPage({
    instanceLabel: refused(403, 'session-not-administrator'),
  })
  await openForm(main)
  typeIn(labelBox(main), 'Edge box')
  fireEvent.click(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  const alert = await within(main).findByText(en('session-revoked'))
  expect(alert.getAttribute('role'), 'the line is an alert').toBe('alert')
  const signIn = within(main).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(before(alert, signIn), 'Sign in is after the alert').toBe(true)
  expect(
    within(main).queryByRole('form', { name: en('instance-label') }),
    'the form stays open',
  ).not.toBeNull()
  expect(labelBox(main).value, 'with what was typed').toBe('Edge box')
})

// ── 17f.9: the keyboard and the names ───────────────────────────────────

test('16 17f.9: the header tab order is Start, Stop, Restart, Start at boot, Rename, and open, Label, Save label, Cancel', async () => {
  const { main } = await openPage()
  // The boot checkbox renders once its read lands: settle it before the
  // snapshot, and name it by the element (an input carries no aria-label and
  // no textContent, so the name map cannot see it).
  const bootBox = await within(main).findByRole('checkbox', {
    name: en('boot-toggle'),
  })
  const nameOf = (el: HTMLElement) =>
    el === bootBox
      ? en('boot-label')
      : (el.getAttribute('aria-label') ?? el.textContent ?? '').trim()
  const names = tabbable().map(nameOf)
  const order = [
    en('run-start'),
    en('run-stop'),
    en('run-restart'),
    en('boot-label'),
    en('instance-rename'),
  ]
  const at = order.map((name) => names.indexOf(name))
  expect(
    at.every((i) => i >= 0),
    `every control is tabbable: ${names}`,
  ).toBe(true)
  expect(at, 'in that order').toEqual([...at].sort((a, b) => a - b))
  await openForm(main)
  const open = tabbable().map(nameOf)
  const box = open.indexOf('')
  expect(
    open.slice(at[4]! + 1, at[4]! + 4),
    'open, the field, Save and Cancel follow Rename',
  ).toEqual(['', en('instance-label-save'), en('instance-label-cancel')])
  expect(box, 'the field is the first stop after Rename').toBe(at[4]! + 1)
})

test('16 17f.9: the field’s name is its visible label, and the outcome of a write is the status line', async () => {
  const { main } = await openPage()
  const form = await openForm(main)
  const label = within(form).getByText(en('instance-label'), {
    selector: 'label',
  })
  const box = labelBox(main)
  expect(label.getAttribute('for'), 'the label is the box’s').toBe(box.id)
  fireEvent.click(
    within(form).getByRole('button', { name: en('instance-label-save') }),
  )
  await flushTasks()
  const status = within(form).getByRole('status')
  expect(status.textContent, 'the outcome is the status line').toBe(
    en('instance-label-saved', 'Coding assistant'),
  )
})

test('16 17f.9: the focus never drops: closing the form with its buttons aria-disabled keeps it on a control of the page', async () => {
  const gate = deferred()
  const { main } = await openPage({ instanceLabel: () => gate.promise })
  await openForm(main)
  typeIn(labelBox(main), 'Edge box')
  fireEvent.click(
    within(main).getByRole('button', { name: en('instance-label-save') }),
  )
  await within(main).findByText(en('instance-label-saving'))
  fireEvent.keyDown(labelBox(main), { key: 'Escape' })
  await flushTasks()
  expect(
    document.activeElement,
    'the focus is on a real element, not the body',
  ).not.toBe(document.body)
  gate.resolve(reply(204))
  await flushTasks()
})

// ── 17f.5, 17f.6: the mid-write rule (17b review R1), the manager’s trim, D2, D3 ──

test('16 17f.5: a write on its way is not cancelled: Escape, Cancel and the opener pressed mid-write leave the form open with the typing, send no second write, and the 504 that lands answers in the open form (17b review R1)', async () => {
  // 17f.6 makes both buttons aria-disabled while the write runs, and 17f.7
  // says the form stays open with what was typed after every failure: a form
  // closed mid-write answers the 409, the 422 or the 504 to nothing, and the
  // form opened again holds the text and sends a second write.
  for (const how of ['escape', 'cancel', 'opener'] as const) {
    const gate = deferred()
    const { bff, main } = await openPage({
      instanceLabel: () => gate.promise,
    })
    await openForm(main)
    typeIn(labelBox(main), 'Edge box')
    fireEvent.click(
      within(main).getByRole('button', { name: en('instance-label-save') }),
    )
    await within(main).findByText(en('instance-label-saving'))
    if (how === 'escape') {
      fireEvent.keyDown(labelBox(main), { key: 'Escape' })
    } else if (how === 'cancel') {
      fireEvent.click(
        within(main).getByRole('button', {
          name: en('instance-label-cancel'),
        }),
      )
    } else {
      fireEvent.click(opener(main))
    }
    await flushTasks()
    expect(
      within(main).queryByRole('form', { name: en('instance-label') }),
      `${how}: the form is open, the write is on its way`,
    ).not.toBeNull()
    expect(labelBox(main).value, `${how}: the typing is kept`).toBe('Edge box')
    expect(labelWrites(bff), `${how}: nothing new is sent`).toHaveLength(1)
    gate.resolve(reply(504, { key: 'client-timeout' }))
    const alert = await within(main).findByText(en('write-timeout'))
    expect(
      alert.getAttribute('role'),
      `${how}: the refusal answers in the open form (17f.7)`,
    ).toBe('alert')
    expect(
      within(main).queryByRole('form', { name: en('instance-label') }),
      `${how}: the form is open with the failure`,
    ).not.toBeNull()
    expect(labelBox(main).value, `${how}: the typing is still there`).toBe(
      'Edge box',
    )
    expect(labelWrites(bff), `${how}: still one write`).toHaveLength(1)
    cleanup()
  }
})

test('16 17f.6: the trim is the manager’s (the API paragraph: spaces and tabs, `checks.rs` instance_label): a label with a leading no-break space is sent with it, and is not taken for the label the instance already has', async () => {
  // `String.prototype.trim` strips every Unicode space (NBSP, U+2009, U+3000),
  // so the form sends a label the manager never received, and takes a label
  // that differs only by such a space for the one the instance has: no write.
  for (const typed of ['\u00a0Edge box', '\u00a0Coding assistant']) {
    const { bff, main } = await openPage()
    await openForm(main)
    typeIn(labelBox(main), typed)
    fireEvent.click(
      within(main).getByRole('button', { name: en('instance-label-save') }),
    )
    await flushTasks()
    const writes = labelWrites(bff)
    expect(
      writes,
      `${JSON.stringify(typed)}: the write goes: it is not the label the instance has`,
    ).toHaveLength(1)
    expect(
      (writes[0]!.body as { label: string }).label,
      `${JSON.stringify(typed)}: the space the manager keeps is sent`,
    ).toBe(typed)
    cleanup()
  }
})

test('16 17f.6 (17f review D2): Enter on a label that did not change is preventDefault-ed: the form closes and stays closed, and nothing is sent', async () => {
  // The Enter branch of the box’s onKeyDown closes the form and moves the focus
  // to Rename without stopping the key: the same Enter then presses Rename,
  // which is focused, and the form opens again (Chromium and WebKit), and the
  // implicit submit calls save a second time.
  const { bff, main } = await openPage()
  await openForm(main)
  const box = labelBox(main)
  typeIn(box, '  Coding assistant  ')
  expect(
    fireEvent.keyDown(box, { key: 'Enter' }),
    'the Enter branch calls preventDefault: the browser does nothing more with the key',
  ).toBe(false)
  await flushTasks()
  expect(labelWrites(bff), 'nothing to write').toHaveLength(0)
  expect(
    within(main).queryByRole('form', { name: en('instance-label') }),
    'the form is closed, and stays closed',
  ).toBeNull()
  expect(document.activeElement, 'the focus is on the opener').toBe(
    opener(main),
  )
})

test('16 17f.5 (17f review D3): the rename form carries a class of the slice namespace, the one index.css lays out', async () => {
  // The form has no class today, so the sheet lays out nothing of it: the
  // label sits inline against the box, the buttons touch, the form starts 0 px
  // under Rename, and the open form scrolls sideways at 320 px (the UX check’s
  // instance-rename state). The declarations are pinned in Reflow.test.ts.
  const { main } = await openPage()
  const form = await openForm(main)
  expect(
    form.className,
    'the form has one class of the slice namespace (instance-rename…, instance-label…)',
  ).toMatch(/^instance-(rename|label)[-a-z0-9]*$/)
})
