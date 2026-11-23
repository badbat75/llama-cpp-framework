// The server-document keys the Server form has no control for (PLAN.md "Fix
// first (found at R2's docs)"): the save writes a key absent from `values` as
// cleared (fe/briefs/api-contract.md, `PUT /api/profiles/server/{id}` "A key
// absent from `values` is read as `null`"), so a save of this page must send
// back every key it read and did not edit. The whole app renders at
// #/profiles/server/default against the fake bff of fe/src/test/bff.ts, as in
// ServerSave.test.tsx. `Tags` must go back as a list: the bff answers 400
// `server-malformed` for a null one, and for an absent one it writes the key
// cleared.
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
  en,
  fakeBff,
  reply,
  serverDoc,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/profiles/server/default')
  dialogs = stubDialog()
})

afterEach(async () => {
  // A link click queues its hashchange as a task: let it land on this test's
  // page, not on the next test's.
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

/** The keys of the read this form has no control for. */
const KEPT = { Tags: ['a', 'b'], Devices: 'ROCm0,ROCm1' }

const PUT = ['PUT', '/api/profiles/server/default'] as const

/** The page with the server form loaded for `options`. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

/** The text input of a field: in its group, or labelled. */
function box(main: HTMLElement, label: string): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: label })
  return (
    g
      ? within(g).getByRole('textbox', { name: label })
      : within(main).getByRole('textbox', { name: label })
  ) as HTMLInputElement
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

/** Clicks Save and waits until the PUT was sent (or `n` of them). */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to(...PUT)).toHaveLength(n))
}

/** Edits the port, the one key every test here changes. */
function editPort(main: HTMLElement, port = '8581') {
  fireEvent.change(box(main, en('server-port')), { target: { value: port } })
}

/** The `values` of the n-th PUT body. */
function sentValues(bff: ReturnType<typeof fakeBff>, n = 1) {
  const body = bff.to(...PUT)[n - 1]!.body as {
    values?: Record<string, unknown>
  }
  expect(body.values, 'the PUT body carries values').toBeTypeOf('object')
  return body.values!
}

// ── Failure paths first: the keys the form does not edit ────────────────

test('R2 fix: a save sends Tags and Devices back exactly as they were read, beside the edited Port', async () => {
  const { bff, main } = await loaded({
    serverProfile: () => reply(200, serverDoc(KEPT)),
  })
  editPort(main)
  await save(main, bff)
  const sent = sentValues(bff)
  expect(Object.keys(sent), 'Tags is sent').toContain('Tags')
  expect(Object.keys(sent), 'Devices is sent').toContain('Devices')
  expect(sent.Tags, 'Tags unchanged').toEqual(['a', 'b'])
  expect(sent.Devices, 'Devices unchanged').toBe('ROCm0,ROCm1')
  expect(sent.Port, 'the edit is sent').toBe(8581)
})

test('R2 fix: an empty Tags goes back as an empty list (a null one is 400 server-malformed) and an unset Devices as null', async () => {
  const { bff, main } = await loaded({
    serverProfile: () => reply(200, serverDoc({ Tags: [], Devices: null })),
  })
  editPort(main)
  await save(main, bff)
  const sent = sentValues(bff)
  const keys = Object.keys(sent)
  expect(keys, 'Tags is sent').toContain('Tags')
  expect(keys, 'Devices is sent').toContain('Devices')
  expect(Array.isArray(sent.Tags), 'Tags is a list, not null').toBe(true)
  expect(sent.Tags, 'no tags stays no tags').toEqual([])
  expect(sent.Devices, 'an unset Devices stays unset').toBeNull()
})

test('R2 fix: every key of the read goes back as it was read, only the edited Port differs', async () => {
  // The two off-by-default switches read as false, so every key of this
  // document round-trips by value through the form.
  const doc = serverDoc({
    ...KEPT,
    RocprofAttach: false,
    AttnRotDisable: false,
  })
  const { bff, main } = await loaded({ serverProfile: () => reply(200, doc) })
  editPort(main)
  await save(main, bff)
  const sent = sentValues(bff)
  for (const [key, value] of Object.entries(doc.values)) {
    expect(Object.keys(sent), `${key} is sent`).toContain(key)
    expect(sent[key], `${key} as read`).toEqual(key === 'Port' ? 8581 : value)
  }
})

test('R2 fix: the keys survive the save they just went in: a second save sends them as they were read', async () => {
  const { bff, main } = await loaded({
    serverProfile: () => reply(200, serverDoc(KEPT)),
  })
  editPort(main)
  await save(main, bff)
  editPort(main, '8582')
  await save(main, bff, 2)
  const sent = sentValues(bff, 2)
  expect(sent.Tags, 'Tags of the second save').toEqual(['a', 'b'])
  expect(sent.Devices, 'Devices of the second save').toBe('ROCm0,ROCm1')
  expect(sent.Port, 'the second edit').toBe(8582)
})

// A guard, green today: the keys the form does not edit are not an edit. The
// coder must keep it so while making the saves above carry them.
test('R2 fix: a document with Tags and Devices loads clean: the keys the form does not edit arm no save', async () => {
  const { bff, main } = await loaded({
    serverProfile: () => reply(200, serverDoc(KEPT)),
  })
  expect(saveButton(main).getAttribute('aria-disabled')).toBe('true')
  fireEvent.click(saveButton(main))
  fireEvent.click(box(main, en('server-port')))
  await act(async () => {})
  expect(bff.to(...PUT), 'a clean form sends nothing').toHaveLength(0)
})
