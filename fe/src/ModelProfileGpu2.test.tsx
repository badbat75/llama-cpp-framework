// Brief 14, sub-slice GD1, criteria GD1.5 to GD1.8 (fe/briefs/14-gpu-distribution.md):
// the row order (checked rows in the file's `device` order, then the listed
// GPUs unchecked, then the file ids the reply does not list), the forgiving
// reading of `device` (split on `,` only, so a `/` belongs to the id; spaces,
// tabs, empty and repeated entries, ids matching a listed GPU case-insensitively
// and showing the file's spelling); a GPU the
// list does not have, with `(not detected)` only when a probe time says the
// probe ran; the reply's states (the as-of line with its `<time>`, no GPUs, no
// probe, and every failure as a plain paragraph that never ends the session,
// decision 5), with items lacking an id and `CPU` left out; and checking and
// unchecking, which keeps the focus on the checkbox and keeps an unchecked
// file id's row. GD1.1 to GD1.4 are in ModelProfileGpu1.test.tsx, GD1.9 to
// GD1.12 in ModelProfileGpu3, GD2 in ModelProfileGpu4. The helpers are those of
// ModelProfileSave15 and ModelProfileSave16.test.tsx, repeated here (a test
// file imports no other test file).
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import {
  before,
  DEVICES,
  en,
  fakeBff,
  held,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
  type Device,
  type DeviceProbe,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const GPU = en('model-profile-gpu-distribution')
const GPUS = en('model-profile-gpus')
const FAILED = en('model-profile-gpu-failed')
const NO_PROBE = en('model-profile-gpu-no-probe')
const NONE_FOUND = en('model-profile-gpu-none-found')
const NOT_DETECTED = en('model-profile-gpu-not-detected')
/** The brief's section 0 reply, as the fake bff answers it by default. */
const AT = DEVICES.probedAt!

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers (as ModelProfileSave15 and ModelProfileSave16.test.tsx) ────────────────

/** Waits for the page read with the values `values` and the GPU probe
 * `DEVICES` answered. With `merge`, a save answers 200 and applies the sent
 * values to the next read, as the bff does, so a save re-reads what was sent. */
async function form(
  values: Record<string, string> = {},
  options: BffOptions = {},
) {
  const loaded = { ...values }
  const bff = fakeBff({
    devices: DEVICES,
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...loaded } }),
      ),
    modelProfileSave: (body) => {
      const sent = (body as { values?: Record<string, unknown> })?.values ?? {}
      for (const [key, value] of Object.entries(sent)) {
        if (value === null || value === undefined) delete loaded[key]
        else loaded[key] = String(value)
      }
      return reply(200, { revision: 'r2', running: false })
    },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: GPU })
  return { bff, main }
}

/** The GPUs fieldset, named by its legend (GD1.4). */
function fieldset(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: GPUS })
}

/** Waits for the device read to land with the brief's probe time (the as-of
 * line is the last thing a good reply writes). */
async function ready(main: HTMLElement): Promise<HTMLElement> {
  const fs = await within(main).findByRole('group', { name: GPUS })
  await within(fs).findByText(asOfDate(AT))
  return fs
}

/** Waits for a read state that writes no as-of line: `text` is its paragraph. */
async function landed(main: HTMLElement, text: string): Promise<HTMLElement> {
  const fs = await within(main).findByRole('group', { name: GPUS })
  await within(fs).findByText(text)
  return fs
}

/** The as-of line the page must write for `probedAt` (GD1.7). */
function asOf(probedAt: string): string {
  return en('model-profile-gpu-probed-at', asOfDate(probedAt))
}

/** The moment the as-of line shows for `probedAt`: the locale date and short
 * time. GD1.7 puts it in the sentence's `{0}` slot inside a `time` element, so
 * it is that element's text. */
function asOfDate(probedAt: string): string {
  return new Date(probedAt).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

/** The as-of paragraphs of `container`, read across that `time` child: the
 * text matcher reads an element's DIRECT text children, so once the moment
 * sits in a child, `getByText(asOf(..))` matches no element at all. The whole
 * sentence is read on `textContent`, which that matcher ignores. */
function asOfLines(container: HTMLElement, probedAt: string): HTMLElement[] {
  const line = asOf(probedAt)
  return [...container.querySelectorAll('p')].filter(
    (p) => (p.textContent ?? '') === line,
  )
}

/** The paragraphs that speak the as-of sentence with ANY moment, read the same
 * way: for an absence pin, where no moment is expected and so none is given. */
function asOfAny(container: HTMLElement): HTMLElement[] {
  const head = en('model-profile-gpu-probed-at', '{0}').split('{0}')[0]!
  return [...container.querySelectorAll('p')].filter((p) =>
    (p.textContent ?? '').includes(head),
  )
}

/** The memory line of GD1.4. */
function memory(totalMib: number, freeMib: number): string {
  return en(
    'model-profile-gpu-memory',
    (totalMib / 1024).toFixed(1),
    (freeMib / 1024).toFixed(1),
  )
}

/** Every device row, in the page's order. */
function rows(main: HTMLElement): {
  li: HTMLElement
  name: string
  box: HTMLInputElement
}[] {
  const fs = fieldset(main)
  const list = fs.querySelector('ul[role="list"]')
  expect(list, 'GD1.4: a ul role="list" of the rows').not.toBeNull()
  const lis = [...list!.children].filter(
    (el) => el.tagName === 'LI',
  ) as HTMLElement[]
  return lis.map((li) => {
    const boxes = within(li).getAllByRole('checkbox')
    expect(boxes, 'GD1.4: one checkbox per row').toHaveLength(1)
    const box = boxes[0] as HTMLInputElement
    return { li, name: nameOf(box), box }
  })
}

/** The rows' ids, in page order: each label starts with the id it shows. */
function rowIds(main: HTMLElement): string[] {
  return rows(main).map((r) => idOf(r.name))
}

/** The rows' state as `[id, checked]`, in page order. */
function rowState(main: HTMLElement): [string, boolean][] {
  return rows(main).map((r) => [idOf(r.name), r.box.checked])
}

/** The id a row's label starts with, up to the first space. */
function idOf(name: string): string {
  return name.split(' ')[0]!
}

/** The row whose label starts with `id`. */
function rowOf(main: HTMLElement, id: string) {
  const found = rows(main).find((r) => idOf(r.name) === id)
  expect(
    found,
    `the row of ${id}: the rows are ${rowIds(main).join(', ')}`,
  ).toBeDefined()
  return found!
}

/** The name a screen reader reads (ModelProfileSave16.test.tsx's computation,
 * with `aria-label` added). */
function nameOf(el: HTMLElement): string {
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((part) => document.getElementById(part)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const label = el.getAttribute('aria-label')
  if (label) return label.replace(/\s+/g, ' ').trim()
  const id = el.getAttribute('id')
  const tied = id ? document.querySelector(`label[for="${id}"]`) : null
  const wrap = el.closest('label')
  // No label of its own, the element names itself with its content, the way an
  // assistive technology reads a button (the About toggle, GD1.9's Move
  // buttons, GD1.1's jump buttons).
  return (tied?.textContent ?? wrap?.textContent ?? el.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

/** The text each id of `aria-describedby` gives, in order. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** A probe answered 200 with `devices` and a `probedAt` of `at`. */
function probe(devices: Device[], at: string | null = AT) {
  return reply(200, { probedAt: at, devices } satisfies DeviceProbe)
}

/** Clicks a row's checkbox. */
function toggle(main: HTMLElement, id: string) {
  fireEvent.click(rowOf(main, id).box)
}

// ── GD1.5: the row order ──────────────────────────────────────────────────────

test('GD1.5: with device CUDA0,rocm0 the rows are CUDA0 and rocm0 checked in the file order, then the listed GPU not checked, rocm0 showing the R9700 name and memory', async () => {
  const { main } = await form({ device: 'CUDA0,rocm0' })
  await ready(main)
  expect(
    rows(main).map((r) => r.name),
    'GD1.5: checked rows first, in the file order, then the reply order',
  ).toEqual([
    'CUDA0 NVIDIA GeForce RTX 5080',
    'rocm0 AMD Radeon AI PRO R9700',
    'Vulkan0 Intel Arc Graphics',
  ])
  expect(rowState(main), 'GD1.5: the checks').toEqual([
    ['CUDA0', true],
    ['rocm0', true],
    ['Vulkan0', false],
  ])
  expect(
    rowOf(main, 'rocm0').li.textContent,
    'GD1.5: the file spelling keeps the GPU memory line',
  ).toContain(memory(32624, 32420))
})

test.each([
  ['spaces and tabs around the ids', ' CUDA0 ,\tROCm0 , ', ['CUDA0', 'ROCm0']],
  ['empty entries', 'CUDA0,,ROCm0,', ['CUDA0', 'ROCm0']],
  ['an id repeated', 'CUDA0,ROCm0,CUDA0', ['CUDA0', 'ROCm0']],
  ['an id repeated in another case', 'CUDA0,rocM0,cuda0', ['CUDA0', 'rocM0']],
  // GD1.5 reads `device` split on `,` only: a `/` belongs to the id.
  ['an id with a slash in it', 'CUDA0/ROCm0', ['CUDA0/ROCm0']],
])(
  'GD1.5: reading device %s: one row per id, in the file order',
  async (_what, device, ids) => {
    const { main } = await form({ device })
    await ready(main)
    expect(rowIds(main).slice(0, ids.length), `GD1.5: ${device}`).toEqual(ids)
    expect(
      rowState(main)
        .filter(([, on]) => on)
        .map(([id]) => id),
      `GD1.5: ${device}: the checked rows, in the file order`,
    ).toEqual(ids)
  },
)

test('GD1.5: a checked file id the reply does not list keeps its place, and the listed GPUs not checked follow it in the reply order', async () => {
  const { main } = await form({ device: 'ROCm0,zzz' })
  await ready(main)
  expect(rowState(main), 'GD1.5: the rows in order').toEqual([
    ['ROCm0', true],
    ['zzz', true],
    ['CUDA0', false],
    ['Vulkan0', false],
  ])
})

test('GD1.5: an id matching a listed GPU case-insensitively shows the file spelling, once, with the GPU name', async () => {
  const { main } = await form({ device: 'rocm0' })
  await ready(main)
  expect(
    rowIds(main),
    'GD1.5: one row for rocm0, and the others unchecked',
  ).toEqual(['rocm0', 'CUDA0', 'Vulkan0'])
  expect(rowOf(main, 'rocm0').name, 'GD1.5: the file spelling').toBe(
    'rocm0 AMD Radeon AI PRO R9700',
  )
})

// ── GD1.6: a GPU the list does not have ───────────────────────────────

test.each([
  ['a probe time', DEVICES as BffOptions['devices'], ' (not detected)'],
  [
    'no probe time',
    { probedAt: null, devices: DEVICES.devices } as BffOptions['devices'],
    '',
  ],
])(
  'GD1.6: a file id the list does not have is a checked row of %s: the id alone, with no name and no memory line',
  async (what, devices, suffix) => {
    const { main } = await form({ device: 'CUDA9' }, { devices })
    await (suffix === '' ? landed(main, NO_PROBE) : ready(main))
    const row = rowOf(main, 'CUDA9')
    expect(row.name, `GD1.6: the label with ${what}`).toBe(`CUDA9${suffix}`)
    expect(row.box.checked, 'GD1.6: the row is checked').toBe(true)
    expect(row.li.textContent, 'GD1.6: no memory line').not.toContain('GiB')
    expect(described(row.box), 'GD1.6: nothing describes it').toEqual([])
  },
)

test('GD1.6: while the read is pending the file id row shows the id alone, with no (not detected)', async () => {
  const h = held()
  const { main } = await form({ device: 'CUDA9' }, { devices: h.answer })
  const row = rowOf(main, 'CUDA9')
  expect(row.name, 'GD1.6: pending: the id alone').toBe('CUDA9')
  h.resolve(probe(DEVICES.devices!))
  await within(fieldset(main)).findByText(asOfDate(AT))
  expect(rowOf(main, 'CUDA9').name, 'GD1.6: the probe adds the word').toBe(
    'CUDA9 (not detected)',
  )
})

test('GD1.6: a failed read shows the file id row as the id alone, with no (not detected)', async () => {
  const { main } = await form({ device: 'CUDA9' }, { devices: reply(503, {}) })
  const fs = await landed(main, FAILED)
  expect(rowOf(main, 'CUDA9').name, 'GD1.6: failed: the id alone').toBe('CUDA9')
  expect(within(fs).queryByText(NOT_DETECTED), 'GD1.6: no word').toBeNull()
})

test('GD1.6: device none is a checked row of an id the list lacks, as in the GUI', async () => {
  const { main } = await form({ device: 'none' })
  await ready(main)
  expect(rowOf(main, 'none').name, 'GD1.6: the none id').toBe(
    'none (not detected)',
  )
  expect(rowOf(main, 'none').box.checked, 'GD1.6: it is checked').toBe(true)
})

test('GD1.6: a well-formed id the probe does not list saves: checking CUDA0 sends device CUDA9,CUDA0 with no fe complaint', async () => {
  const { bff, main } = await form({ device: 'CUDA9' })
  await ready(main)
  expect(statusText(main), 'GD1.6: the load is clean').toBe('')
  toggle(main, 'CUDA0')
  await save(main, bff)
  expect(
    sentValues(bff)['device'],
    'GD1.6: the unknown id is kept, the new check appended',
  ).toBe('CUDA9,CUDA0')
  await waitFor(() =>
    expect(statusText(main), 'GD1.6: the save was accepted').toBe(
      en('save-saved'),
    ),
  )
})

// ── GD1.7: the read states ────────────────────────────────────────────────────

test('GD1.7: a 200 with GPUs shows Free memory as of, with a time element carrying probedAt as sent, after the legend', async () => {
  const { main } = await form()
  const fs = await ready(main)
  const legend = fs.querySelector('legend')!
  // GD1.7, read with the ruling of 2026-11-21 (the moment named once): the
  // locale date is the `time` element's text, in the sentence's `{0}` slot, so
  // the paragraph's DIRECT text is "Free memory as of ." and `getByText` of
  // the whole sentence, which reads direct text children only, sees nothing.
  // The paragraph is found through the time, as the pin beside it does.
  const time = fs.querySelector('time')
  expect(time, 'GD1.7: a time element').not.toBeNull()
  const line = time!.closest('p')
  expect(line, 'GD1.7: a paragraph').not.toBeNull()
  expect(line!.tagName, 'GD1.7: a paragraph').toBe('P')
  expect(before(legend, line!), 'GD1.7: after the legend').toBe(true)
  expect(
    time!.getAttribute('datetime'),
    'GD1.7: datetime is probedAt as sent',
  ).toBe(AT)
  expect(
    time!.textContent,
    'GD1.7: the text is the locale date and short time',
  ).toBe(
    new Date(AT).toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    }),
  )
})

test('GD1.7: the as-of line names the moment once: the time element sits inside the sentence, and the paragraph holds no second copy of the date', async () => {
  const { main } = await form()
  const fs = await ready(main)
  const time = fs.querySelector('time')
  expect(time, 'GD1.7: a time element carrying the moment').not.toBeNull()
  const line = time!.closest('p')
  expect(line, 'GD1.7: the time sits in the as-of paragraph').not.toBeNull()
  expect(
    line!.textContent,
    'GD1.7: the sentence, the date in its {0} slot, once',
  ).toBe(asOf(AT))
  expect(
    time!.getAttribute('datetime'),
    'GD1.7: datetime is probedAt as sent',
  ).toBe(AT)
})

test.each(['not a date', '2026-13-45T99:99:99Z'])(
  'GD1.7: a probedAt of %s that Date does not parse writes no as-of line',
  async (at) => {
    const { main } = await form({}, { devices: probe(DEVICES.devices!, at) })
    const fs = await within(main).findByRole('group', { name: GPUS })
    await waitFor(() =>
      expect(rowIds(main), 'GD1.7: the rows land, so the reply landed').toEqual(
        ['CUDA0', 'ROCm0', 'Vulkan0'],
      ),
    )
    expect(asOfLines(fs, at), 'GD1.7: no line').toHaveLength(0)
    expect(fs.querySelector('time'), 'GD1.7: no time element').toBeNull()
  },
)

test('GD1.7: a 200 with devices [] and a time shows No GPU found on this host, then the as-of line, and no rows', async () => {
  const { main } = await form({}, { devices: probe([]) })
  const fs = await landed(main, NONE_FOUND)
  const asof = asOfLines(fs, AT)
  expect(asof, 'GD1.7: one as-of paragraph').toHaveLength(1)
  expect(
    before(within(fs).getByText(NONE_FOUND), asof[0]!),
    'GD1.7: the word first',
  ).toBe(true)
  expect(rows(main), 'GD1.7: no rows').toHaveLength(0)
})

test('GD1.7: a 200 with probedAt null shows the no-probe line, no as-of line, and no rows', async () => {
  const { main } = await form(
    {},
    { devices: { probedAt: null, devices: DEVICES.devices } },
  )
  const fs = await landed(main, NO_PROBE)
  expect(asOfAny(fs), 'GD1.7: no as-of').toHaveLength(0)
  expect(rows(main), 'GD1.7: no rows').toHaveLength(0)
})

test('GD1.7: a 200 with probedAt null and devices [] says the no-probe line alone: the card never says No GPU found on this host beside a probe that never answered', async () => {
  const { main } = await form({}, { devices: { probedAt: null, devices: [] } })
  const fs = await landed(main, NO_PROBE)
  expect(
    within(fs).queryByText(NONE_FOUND),
    'GD1.7: a probe with no answer says no probe, not that the probe found no GPU',
  ).toBeNull()
  expect(rows(main), 'GD1.7: no rows').toHaveLength(0)
})

test.each([
  ['401', reply(401, { key: 'session-required' })],
  ['403', reply(403, { key: 'node-not-administrator' })],
  ['503', reply(503, { key: 'node-unavailable' })],
  ['200 with no devices member', reply(200, { probedAt: AT })],
  ['200 with devices not an array', reply(200, { probedAt: AT, devices: 'x' })],
  ['200 with a body that is not an object', reply(200, [])],
  ['200 that is not JSON', reply(200, 'nope')],
])(
  'GD1.7: a device read of %s shows the failure text as a plain paragraph, keeps the session and the page, and writes no alert',
  async (_what, devices) => {
    const { main } = await form({ device: 'CUDA0' }, { devices })
    const fs = await landed(main, FAILED)
    const line = within(fs).getByText(FAILED)
    expect(line.tagName, 'GD1.7: a paragraph').toBe('P')
    expect(line.getAttribute('role'), 'GD1.7: not a status').toBeNull()
    expect(line.closest('[role="alert"]'), 'GD1.7: not an alert').toBeNull()
    expect(
      within(main).queryAllByRole('alert'),
      'GD1.7: no alert anywhere',
    ).toHaveLength(0)
    // decision 5: the session stays, the page stays, its fields are editable.
    expect(
      within(main).queryByText(en('signin-title')),
      'GD1.7: not signed out',
    ).toBeNull()
    expect(
      // The session bar is in the app header, outside the page's `main`.
      within(document.body).queryByRole('button', {
        name: en('session-sign-out'),
      }),
      'GD1.7: the session is still there',
    ).not.toBeNull()
    expect(
      within(main).getByRole('heading', { level: 1, name: ID }),
      'GD1.7: the page',
    ).not.toBeNull()
    expect(
      rowOf(main, 'CUDA0').box.checked,
      'GD1.7: the profile GPU is shown',
    ).toBe(true)
  },
)

test('GD1.7: a device read that fails with a network error shows the failure text as a plain paragraph, and the page saves beside it', async () => {
  const { bff, main } = await form(
    {},
    { devices: () => Promise.reject(new TypeError('network down')) },
  )
  const fs = await landed(main, FAILED)
  expect(within(fs).getByText(FAILED), 'GD1.7: the failure text').not.toBeNull()
  expect(within(main).queryAllByRole('alert'), 'GD1.7: no alert').toHaveLength(
    0,
  )
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '65536' },
  })
  await save(main, bff)
  expect(sentValues(bff)['ctx-size'], 'GD1.7: the page still saves').toBe(
    '65536',
  )
})

test.each([
  ['no id', { name: 'A', totalMib: 100, freeMib: 10 }],
  ['an empty id', { id: '', name: 'A', totalMib: 100, freeMib: 10 }],
  ['CPU', { id: 'CPU', name: 'A', totalMib: 100, freeMib: 10 }],
  ['cpu in lower case', { id: 'cpu', name: 'A', totalMib: 100, freeMib: 10 }],
] as [string, Device][])(
  'GD1.7: an item with %s inside a good reply is left out of the list',
  async (_what, device) => {
    const { main } = await form(
      {},
      {
        devices: probe([
          { id: 'CUDA0', name: 'A', totalMib: 100, freeMib: 10 },
          device,
        ]),
      },
    )
    await ready(main)
    expect(rowIds(main), 'GD1.7: only the well-formed item').toEqual(['CUDA0'])
  },
)

test('GD1.7: a name that is not a string reads as the empty string: the label is the id alone, and there is no memory line when totalMib is missing', async () => {
  const { main } = await form(
    {},
    {
      devices: probe([
        { id: 'CUDA0', name: 42, totalMib: 100, freeMib: 10 },
        { id: 'ROCm0', totalMib: 100, freeMib: 10 },
      ]),
    },
  )
  await ready(main)
  expect(rowOf(main, 'CUDA0').name, 'GD1.7: the id alone').toBe('CUDA0')
  expect(rowOf(main, 'ROCm0').name, 'GD1.7: the id alone, no name').toBe(
    'ROCm0',
  )
})

// ── GD1.8: checking and unchecking ───────────────────────────────────────────

test('GD1.8: checking a row makes it the last checked row, and unchecking one moves it back among the unchecked rows', async () => {
  const { main } = await form({ device: '' })
  await ready(main)
  expect(rowIds(main), 'GD1.8: nothing checked: the reply order').toEqual([
    'CUDA0',
    'ROCm0',
    'Vulkan0',
  ])
  toggle(main, 'CUDA0')
  toggle(main, 'Vulkan0')
  expect(
    rowState(main),
    'GD1.8: the checked rows, in the order checked',
  ).toEqual([
    ['CUDA0', true],
    ['Vulkan0', true],
    ['ROCm0', false],
  ])
  toggle(main, 'CUDA0')
  expect(rowState(main), 'GD1.8: the unchecked row moves back').toEqual([
    ['Vulkan0', true],
    ['CUDA0', false],
    ['ROCm0', false],
  ])
})

test('GD1.8: the checkbox keeps the focus through the move, and no other control moves focus', async () => {
  const { main } = await form({ device: 'CUDA0' })
  await ready(main)
  const box = rowOf(main, 'Vulkan0').box
  box.focus()
  fireEvent.click(box)
  const moved = rowOf(main, 'Vulkan0').box
  expect(moved, 'GD1.8: the same checkbox element, keyed by id').toBe(box)
  expect(document.activeElement, 'GD1.8: the focus stays on it').toBe(box)
  expect(moved.checked, 'GD1.8: the row is now checked').toBe(true)
})

test('GD1.8: a file id unchecked keeps its row, until a save re-reads the profile', async () => {
  const { bff, main } = await form({ device: 'CUDA9,CUDA0' })
  await ready(main)
  toggle(main, 'CUDA9')
  expect(
    // GD1.5's place for an unchecked id the reply does not list: after the
    // checked rows and the listed GPUs, in file order.
    rowIds(main),
    'GD1.8: the unchecked file id keeps its row, among the unchecked',
  ).toEqual(['CUDA0', 'ROCm0', 'Vulkan0', 'CUDA9'])
  expect(rowOf(main, 'CUDA9').name, 'GD1.8: still (not detected)').toBe(
    'CUDA9 (not detected)',
  )
  await save(main, bff)
  expect(sentValues(bff)['device'], 'GD1.8: the save drops it').toBe('CUDA0')
  await waitFor(() =>
    expect(
      rowIds(main).includes('CUDA9'),
      'GD1.8: the re-read rebuilds the rows from the new values',
    ).toBe(false),
  )
})

test('GD1.8: checking and unchecking is a change the save bar counts, and Discard restores the loaded rows, checks and order', async () => {
  const { main } = await form({ device: 'ROCm0,CUDA0' })
  await ready(main)
  expect(rowIds(main), 'the loaded order').toEqual([
    'ROCm0',
    'CUDA0',
    'Vulkan0',
  ])
  toggle(main, 'Vulkan0')
  expect(statusText(main), 'GD1.8: a check is a change').toBe(
    en('save-unsaved'),
  )
  toggle(main, 'Vulkan0')
  expect(statusText(main), 'GD1.8: changing it back is no change').toBe('')
  toggle(main, 'Vulkan0')
  toggle(main, 'ROCm0')
  expect(rowIds(main), 'GD1.8: the rows moved').toEqual([
    'CUDA0',
    'Vulkan0',
    'ROCm0',
  ])
  fireEvent.click(
    within(main).getByRole('button', { name: en('save-discard') }),
  )
  await waitFor(() =>
    expect(rowIds(main), 'GD1.8: Discard restored the order').toEqual([
      'ROCm0',
      'CUDA0',
      'Vulkan0',
    ]),
  )
  expect(rowState(main), 'GD1.8: and the checks').toEqual([
    ['ROCm0', true],
    ['CUDA0', true],
    ['Vulkan0', false],
  ])
  expect(statusText(main), 'GD1.8: the form is clean again').toBe('')
})

// ── The save bar helpers GD1.8 needs ─────────────────────────────────────────

/** The save bar status line's text. The GPUs card keeps its move status in the
 * DOM from its first render (GD1.9), so the bar's line is picked inside the
 * bar, not by counting the page's status regions (ruling of 2026-11-21). */
function statusText(main: HTMLElement): string {
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => s.closest('.save-bar') !== null)
  expect(all, 'the save bar has its status line').toHaveLength(1)
  return all[0]!.textContent ?? ''
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

/** Clicks Save changes and waits for the PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** The `values` of the PUT. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
  return body.values as Record<string, unknown>
}
