// Brief 14, sub-slice GD1, criteria GD1.9 to GD1.12 (fe/briefs/14-gpu-distribution.md):
// the Move up and Move down buttons of the checked rows (their names, the
// `aria-disabled` ends, the swap, the focus and the card's one visually hidden
// `role="status"` that says the move); saving `device` (the row order joined
// with `,`, `null` with none checked, unchanged spellings never sent); the 422
// `device-bad-name` refusal on the GPUs fieldset (GD1.11, decision 5 of 02h);
// the keyboard path through the card and the rule that nothing takes focus on
// arrival or when the read lands (GD1.12). The 40 s limit of the device read
// and the rule that nothing polls (GD1.3, GD1.7) are here too, on a fake clock.
// GD1.1 to GD1.4 are in ModelProfileGpu1.test.tsx, GD1.5 to GD1.8 in
// ModelProfileGpu2, GD2 in ModelProfileGpu4. The helpers are those of
// ModelProfileSave15 and ModelProfileSave16.test.tsx, repeated here (a test
// file imports no other test file).
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
  before,
  DEVICES,
  en,
  fakeBff,
  held,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const DEVICES_ROUTE = '/api/node/devices'
const GPU = en('model-profile-gpu-distribution')
const GPUS = en('model-profile-gpus')
const MODE_LABEL = en('model-profile-split-mode')
const UP = en('model-profile-gpu-move-up')
const DOWN = en('model-profile-gpu-move-down')
const DETECTING = en('model-profile-gpu-detecting')
const FAILED = en('model-profile-gpu-failed')
/** The device read's time limit, from the brief (GD1.3). */
const LIMIT = 40_000

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  startAt('')
})

// ── Helpers (as ModelProfileSave15 and ModelProfileSave16.test.tsx) ────────────────

/** Waits for the page read with the values `values` and the GPU probe
 * `DEVICES` answered, and for two GPUs to be checked (`CUDA0,ROCm0` by
 * default), which is when GD1.9's move buttons exist.
 *
 * `clock` is for GD1.3's and GD1.7's timed tests: the fake clock goes on
 * BEFORE the render, because the 40 s limit of the device read is armed by the
 * mount, and the mount is then flushed and read with synchronous queries: a
 * `findBy*` polls on a timer the fake clock never advances, so it would never
 * settle (the clocked-mount shape of `LeaveGuard.test.tsx`). */
async function form(
  values: Record<string, string> = { device: 'CUDA0,ROCm0' },
  options: BffOptions = {},
  clock = false,
) {
  if (clock) fakeClock()
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
  if (clock) {
    await flush()
    expect(
      within(main).queryByRole('heading', { level: 1, name: ID }),
      'the page is mounted on the fake clock',
    ).not.toBeNull()
    expect(
      within(main).queryByRole('heading', { level: 3, name: GPU }),
      'the GPU distribution card is there on the fake clock',
    ).not.toBeNull()
    return { bff, main }
  }
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: GPU })
  await ready(main)
  return { bff, main }
}

/** The card's `section`, from its h3 (Card.tsx). */
function cardOf(main: HTMLElement, title: string): HTMLElement {
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  const card = h3.closest('section')
  expect(card, `${title}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The GPUs fieldset, named by its legend (GD1.4). */
function fieldset(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: GPUS })
}

/** Waits for the device read to land with the brief's probe time. */
async function ready(main: HTMLElement): Promise<HTMLElement> {
  const fs = await within(main).findByRole('group', { name: GPUS })
  await within(fs).findByText(asOfDate(DEVICES.probedAt!))
  return fs
}

/** The device read landed, read the way a clocked test can: flush, then a
 * synchronous query (GD1.3, GD1.7, GD1.12). */
async function landed(main: HTMLElement): Promise<HTMLElement> {
  await flush()
  const fs = fieldset(main)
  expect(
    asOfLines(fs, DEVICES.probedAt!),
    'GD1.7: the read landed with its as-of line',
  ).toHaveLength(1)
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

/** The rows' ids, in page order. */
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

/** Every button of a row, with its accessible name. */
function rowButtons(main: HTMLElement, id: string): HTMLButtonElement[] {
  // `queryAllByRole`: an unchecked row holds no button at all (GD1.9), and
  // `getAllByRole` would throw the roles dump instead of naming the row.
  return within(rowOf(main, id).li).queryAllByRole(
    'button',
  ) as HTMLButtonElement[]
}

/** The Move up (or Move down) button of `id`, by its accessible name: the
 * visible text plus the id in a visually hidden span (GD1.9). */
function moveButton(
  main: HTMLElement,
  id: string,
  dir: typeof UP | typeof DOWN,
): HTMLButtonElement {
  const found = rowButtons(main, id).filter((b) => nameOf(b) === `${dir} ${id}`)
  expect(
    found,
    `GD1.9: ${dir} ${id}: the row's buttons are ${rowButtons(main, id)
      .map((b) => nameOf(b))
      .join(' | ')}`,
  ).toHaveLength(1)
  return found[0]!
}

/** The checked rows' ids, in page order: the state `device` is made of. */
function checkedIds(main: HTMLElement): string[] {
  return rows(main)
    .filter((r) => r.box.checked)
    .map((r) => idOf(r.name))
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

/** The text a sighted user reads on `el`: its content with the visually hidden
 * spans left out. jsdom applies no CSS, so `textContent` counts them too, and
 * a button that names itself "Move up CUDA0" shows only "Move up" (GD1.9,
 * WCAG 2.5.3 Label in Name). */
function visibleText(el: HTMLElement): string {
  const hidden = new Set(el.querySelectorAll('.visually-hidden'))
  let out = ''
  for (const node of el.childNodes)
    if (node.nodeType === 1 && hidden.has(node as Element)) out += ' '
    else out += node.textContent ?? ''
  return out.replace(/\s+/g, ' ').trim()
}

/** The text each id of `aria-describedby` gives, in order. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

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

/** The save bar's `role="alert"` text, `""` when there is none. */
function alertText(main: HTMLElement): string {
  const alerts = within(main).queryAllByRole('alert')
  expect(alerts, 'at most one alert in the bar').toHaveLength(1)
  return alerts[0]?.textContent ?? ''
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

/** Clicks a row's checkbox. */
function toggle(main: HTMLElement, id: string) {
  fireEvent.click(rowOf(main, id).box)
}

/** Presses a row's Move up / Move down. */
function move(
  main: HTMLElement,
  id: string,
  dir: typeof UP | typeof DOWN = UP,
) {
  const button = moveButton(main, id, dir)
  button.focus()
  fireEvent.click(button)
  return button
}

/** Every tabbable element of `main`, in document order: the ones with no
 * negative `tabindex`, no disabled, not hidden (GD1.12, as the other keyboard
 * tests compute it). */
function mainTabbable(main: HTMLElement): HTMLElement[] {
  const sel =
    'a[href], button, input, select, textarea, [tabindex], summary, [contenteditable="true"]'
  return [...main.querySelectorAll<HTMLElement>(sel)].filter((el) => {
    if (el.tabIndex < 0) return false
    if ((el as HTMLButtonElement).disabled) return false
    let node: HTMLElement | null = el
    while (node && node !== main) {
      if (
        node.hidden ||
        node.getAttribute('aria-hidden') === 'true' ||
        node.getAttribute('inert') !== null
      )
        return false
      node = node.parentElement
    }
    return true
  })
}

/** Fake timers, the microtask queue left real (ModelProfilePage.test.tsx). */
function fakeClock() {
  vi.useFakeTimers({
    toFake: [
      'setTimeout',
      'clearTimeout',
      'setInterval',
      'clearInterval',
      'Date',
    ],
  })
}

/** Lets every answer and render due now land. */
async function flush() {
  await act(async () => {
    for (let i = 0; i < 4; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

// ── GD1.9: Move up and Move down ───────────────────────────────────────────

test('GD1.9: with two rows checked each checked row holds Move up and Move down after its other controls, named by their text plus the id in a visually hidden span', async () => {
  const { main } = await form({ device: 'CUDA0,ROCm0' })
  const card = cardOf(main, GPU)
  for (const [n, id] of ['CUDA0', 'ROCm0'].entries()) {
    const row = rowOf(main, id)
    for (const dir of [UP, DOWN]) {
      const button = moveButton(main, id, dir)
      // GD1.9: the FIRST row's Move up and the LAST checked row's Move down
      // are aria-disabled="true", so no blanket "active" can be pinned here;
      // the ends are pinned by the two pins of that rule (three checked rows,
      // and two). What both directions of every row owe is D5.2 Inactive: the
      // `disabled` attribute stays off, so the button keeps the focus.
      expect(
        button.disabled,
        `GD1.9: D5.2 Inactive, not the disabled attribute, of ${dir} of row ${n + 1}`,
      ).toBe(false)
      expect(card.contains(button), `GD1.9: ${dir} ${id} is in the card`).toBe(
        true,
      )
      expect(
        before(row.box, button),
        `GD1.9: ${dir} ${id} is after the row's other controls`,
      ).toBe(true)
      // 2.5.3 Label in Name: the visible text starts the name.
      expect(
        visibleText(button),
        `GD1.9: the visible text of ${dir} ${id}`,
      ).toBe(dir)
      expect(nameOf(button), `GD1.9: the name of ${dir} ${id}`).toBe(
        `${dir} ${id}`,
      )
      const hidden = [...button.querySelectorAll('*')].filter((el) =>
        (el.getAttribute('class') ?? '').includes('visually-hidden'),
      )
      expect(
        hidden.map((el) => el.textContent),
        `GD1.9: the id is in a visually hidden span of ${dir} ${id}`,
      ).toContain(id)
    }
  }
  // After the row's other controls: the checkbox precedes both.
  for (const id of ['CUDA0', 'ROCm0']) {
    const row = rowOf(main, id)
    expect(
      before(row.box, moveButton(main, id, UP)),
      `GD1.9: Move up ${id} follows its checkbox`,
    ).toBe(true)
    expect(
      before(moveButton(main, id, UP), moveButton(main, id, DOWN)),
      `GD1.9: Move up ${id} precedes Move down`,
    ).toBe(true)
  }
})

test.each([
  ['no GPU checked', ''],
  ['one GPU checked', 'CUDA0'],
])(
  'GD1.9: with %s the card has no move button at all',
  async (_what, device) => {
    const { main } = await form({ device })
    expect(
      within(cardOf(main, GPU))
        .queryAllByRole('button')
        .filter((b) => {
          const name = nameOf(b)
          return name.startsWith(UP) || name.startsWith(DOWN)
        }),
      `GD1.9: ${device}: no move button`,
    ).toHaveLength(0)
  },
)

test('GD1.9: an unchecked row has no move button, in a list of checked rows beside it', async () => {
  const { main } = await form({ device: 'CUDA0,ROCm0' })
  const names = rowButtons(main, 'Vulkan0').map((b) => nameOf(b))
  expect(
    names.filter((n) => n.startsWith(UP) || n.startsWith(DOWN)),
    `GD1.9: the unchecked row's buttons: ${names.join(' | ')}`,
  ).toHaveLength(0)
})

test('GD1.9: the first row Move up and the last checked row Move down are aria-disabled, the others are not, and a click on an aria-disabled one does nothing, the focus stays', async () => {
  const { main } = await form({ device: 'CUDA0,ROCm0,Vulkan0' })
  const ends: [string, typeof UP | typeof DOWN, string][] = [
    ['CUDA0', UP, 'true'],
    ['CUDA0', DOWN, 'false'],
    ['ROCm0', UP, 'false'],
    ['ROCm0', DOWN, 'false'],
    ['Vulkan0', UP, 'false'],
    ['Vulkan0', DOWN, 'true'],
  ]
  for (const [id, dir, want] of ends) {
    const button = moveButton(main, id, dir)
    expect(
      button.getAttribute('aria-disabled') === 'true',
      `GD1.9: ${dir} ${id} aria-disabled`,
    ).toBe(want === 'true')
    expect(
      button.disabled,
      `D5.2: not the disabled attribute of ${dir} ${id}`,
    ).toBe(false)
  }
  const first = moveButton(main, 'CUDA0', UP)
  first.focus()
  fireEvent.click(first)
  expect(rowIds(main), 'GD1.9: nothing moved').toEqual([
    'CUDA0',
    'ROCm0',
    'Vulkan0',
  ])
  expect(document.activeElement, 'GD1.9: the focus stayed').toBe(first)
  const last = moveButton(main, 'Vulkan0', DOWN)
  last.focus()
  fireEvent.click(last)
  expect(rowIds(main), 'GD1.9: nothing moved').toEqual([
    'CUDA0',
    'ROCm0',
    'Vulkan0',
  ])
  expect(document.activeElement, 'GD1.9: the focus stayed').toBe(last)
})

test('GD1.9: with exactly two rows checked both directions have an end: the first row Move up and the last row Move down are aria-disabled, the second row Move up and the first row Move down are not', async () => {
  const { main } = await form({ device: 'CUDA0,ROCm0' })
  expect(
    rowIds(main),
    'GD1.9: two checked rows, and the listed GPU unchecked beside them',
  ).toEqual(['CUDA0', 'ROCm0', 'Vulkan0'])
  const ends: [string, typeof UP | typeof DOWN, boolean][] = [
    ['CUDA0', UP, true],
    ['CUDA0', DOWN, false],
    ['ROCm0', UP, false],
    ['ROCm0', DOWN, true],
  ]
  for (const [id, dir, want] of ends) {
    const button = moveButton(main, id, dir)
    expect(
      button.getAttribute('aria-disabled') === 'true',
      `GD1.9: ${dir} of ${id}, one of two checked rows: aria-disabled`,
    ).toBe(want)
    expect(button.disabled, `D5.2: not the disabled attribute`).toBe(false)
  }
})

test('GD1.9: Move up swaps the row with its neighbour among the checked rows, and the pressed button keeps the focus in the moved row', async () => {
  const { main } = await form({ device: 'CUDA0,ROCm0,Vulkan0' })
  const button = move(main, 'ROCm0', UP)
  expect(rowIds(main), 'GD1.9: rocm0 is first').toEqual([
    'ROCm0',
    'CUDA0',
    'Vulkan0',
  ])
  const moved = moveButton(main, 'ROCm0', UP)
  expect(moved, 'GD1.9: the same button element, keyed by id').toBe(button)
  expect(document.activeElement, 'GD1.9: the focus is on it').toBe(moved)
  expect(
    rowOf(main, 'ROCm0').li.contains(moved),
    'GD1.9: the button is in the moved row',
  ).toBe(true)
  // After the swap the row is first, so its Move up is an end (GD1.9): the way
  // back is the row's Move down.
  expect(
    moveButton(main, 'ROCm0', UP).getAttribute('aria-disabled'),
    'GD1.9: the pressed button is now an end',
  ).toBe('true')
  const back = move(main, 'ROCm0', DOWN)
  expect(rowIds(main), 'GD1.9: Move down again, ROCm0 is back').toEqual([
    'CUDA0',
    'ROCm0',
    'Vulkan0',
  ])
  expect(document.activeElement, 'GD1.9: the focus is on it').toBe(back)
})

test('GD1.9: Move down on the last checked row of a list where an unchecked row follows does nothing: the move is among the checked rows', async () => {
  const { main } = await form({ device: 'CUDA0,ROCm0' })
  expect(rowIds(main), 'the rows').toEqual(['CUDA0', 'ROCm0', 'Vulkan0'])
  const button = move(main, 'ROCm0', DOWN)
  expect(rowIds(main), 'GD1.9: Vulkan0 stays last, ROCm0 stays second').toEqual(
    ['CUDA0', 'ROCm0', 'Vulkan0'],
  )
  expect(document.activeElement, 'GD1.9: the focus stayed').toBe(button)
})

test('GD1.9: the card has one visually hidden role=status that says the move: id, new position and checked count, and nothing is announced before a move', async () => {
  const { main } = await form({ device: 'CUDA0,ROCm0,Vulkan0' })
  const card = cardOf(main, GPU)
  const live = () => within(card).getAllByRole('status')
  expect(live(), 'GD1.9: one move status in the card').toHaveLength(1)
  expect(live()[0]!.textContent, 'GD1.9: nothing announced before a move').toBe(
    '',
  )
  move(main, 'ROCm0', UP)
  expect(live(), 'GD1.9: still one').toHaveLength(1)
  expect(live()[0]!.textContent, 'GD1.9: the move is announced').toBe(
    en('model-profile-gpu-moved', 'ROCm0', '1', '3'),
  )
  // ROCm0's move left CUDA0 second of three, so its Move down lands it last.
  move(main, 'CUDA0', DOWN)
  expect(live()[0]!.textContent, 'GD1.9: the last move only').toBe(
    en('model-profile-gpu-moved', 'CUDA0', '3', '3'),
  )
  const el = live()[0]!
  expect(
    [...el.querySelectorAll('*')].some((n) =>
      (n.getAttribute('class') ?? '').includes('visually-hidden'),
    ) || (el.getAttribute('class') ?? '').includes('visually-hidden'),
    'GD1.9: the status is visually hidden',
  ).toBe(true)
})

test('GD1.9: the move status region is in the DOM and empty from the card first render, with no row checked and no move button', async () => {
  const { main } = await form({ device: '' })
  const card = cardOf(main, GPU)
  expect(checkedIds(main), 'GD1.9: no row checked').toEqual([])
  const live = within(card).queryAllByRole('status')
  expect(live, 'GD1.9: the region is there with no row checked').toHaveLength(1)
  expect(live[0]!.textContent, 'GD1.9: and empty').toBe('')
})

test('GD1.9: a move counts for the save bar, and the save sends the new order', async () => {
  const { bff, main } = await form({ device: 'CUDA0,ROCm0' })
  expect(statusText(main), 'GD1.9: the load is clean').toBe('')
  move(main, 'ROCm0', UP)
  expect(statusText(main), 'GD1.9: the move is a change').toBe(
    en('save-unsaved'),
  )
  await save(main, bff)
  expect(sentValues(bff)['device'], 'GD1.9: the new order is sent').toBe(
    'ROCm0,CUDA0',
  )
})

// ── GD1.10: saving device ──────────────────────────────────────────────────

test('GD1.10: the checked ids in row order joined with a comma and no spaces, and null when none is checked', async () => {
  const { bff, main } = await form({ device: '' })
  toggle(main, 'ROCm0')
  toggle(main, 'CUDA0')
  toggle(main, 'Vulkan0')
  expect(checkedIds(main), 'the checks in row order').toEqual([
    'ROCm0',
    'CUDA0',
    'Vulkan0',
  ])
  await save(main, bff)
  expect(sentValues(bff)['device'], 'GD1.10: the row order, no spaces').toBe(
    'ROCm0,CUDA0,Vulkan0',
  )
})

test('GD1.10: unchecking every row sends null', async () => {
  const { bff, main } = await form({ device: 'CUDA0' })
  toggle(main, 'CUDA0')
  expect(
    rowState(main).filter(([, on]) => on),
    'nothing checked',
  ).toHaveLength(0)
  await save(main, bff)
  expect(sentValues(bff)['device'], 'GD1.10: none checked is null').toBeNull()
})

test.each([
  ['spaces after the comma', 'CUDA0, ROCm1'],
  ['a lower case id', 'cuda0'],
  ['one id', 'ROCm0'],
  ['the GUI none', 'none'],
])(
  'GD1.10: an untouched device spelled %s is never sent, whatever the probe answers',
  async (_what, device) => {
    const { bff, main } = await form({ device })
    expect(statusText(main), 'GD1.10: the load is clean').toBe('')
    expect(
      rowState(main)
        .filter(([, on]) => on)
        .map(([id]) => id),
    ).toEqual(idOfList(device))
    fireEvent.change(
      document.getElementById('model-profile-ctx-size-control')!,
      { target: { value: '65536' } },
    )
    await save(main, bff)
    expect(
      'device' in sentValues(bff),
      'GD1.10: the save sends ctx-size only',
    ).toBe(false)
  },
)

/** The ids GD1.5 reads out of a `device` line: split on commas, trimmed,
 * empty and repeated entries dropped. */
function idOfList(device: string): string[] {
  const seen: string[] = []
  for (const part of device.split(',')) {
    const id = part.trim()
    if (id && !seen.some((s) => s.toLowerCase() === id.toLowerCase()))
      seen.push(id)
  }
  return seen
}

test('GD1.10: changing the checked rows and changing them back leaves the form clean, and Discard restores the loaded rows, checks and order', async () => {
  const { main } = await form({ device: 'ROCm0,CUDA0' })
  toggle(main, 'Vulkan0')
  expect(statusText(main), 'GD1.10: a check is a change').toBe(
    en('save-unsaved'),
  )
  toggle(main, 'Vulkan0')
  expect(statusText(main), 'GD1.10: back is clean').toBe('')
  move(main, 'CUDA0', UP)
  expect(statusText(main), 'GD1.10: a move is a change').toBe(
    en('save-unsaved'),
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('save-discard') }),
  )
  await waitFor(() =>
    expect(rowState(main), 'GD1.10: Discard restored the rows').toEqual([
      ['ROCm0', true],
      ['CUDA0', true],
      ['Vulkan0', false],
    ]),
  )
  expect(statusText(main), 'GD1.10: the form is clean').toBe('')
})

test('GD1.10: after a save the rows are rebuilt from the new values, and the device read is not asked again', async () => {
  const { bff, main } = await form({ device: 'CUDA0' })
  toggle(main, 'ROCm0')
  await save(main, bff)
  await waitFor(() =>
    expect(rowState(main), 'GD1.10: the rows show the saved state').toEqual([
      ['CUDA0', true],
      ['ROCm0', true],
      ['Vulkan0', false],
    ]),
  )
  await waitFor(() =>
    expect(statusText(main), 'GD1.10: the save was accepted').toBe(
      en('save-saved'),
    ),
  )
  expect(
    bff.to('GET', DEVICES_ROUTE).length,
    'GD1.10: the save did not read the devices again',
  ).toBe(1)
})

// ── GD1.11: the device-bad-name refusal ────────────────────────────────────

/** The 422 body of GD1.11: the refusal names the entry it refused. */
function badName(field: string | undefined, entry = 'CUDA9') {
  const args = ['device', entry]
  return reply(422, {
    key: 'device-bad-name',
    args,
    ...(field === undefined ? {} : { field }),
  })
}

/** The refusal text of the fieldset. */
function badNameText(entry = 'CUDA9') {
  return en('api-device-bad-name', 'device', entry)
}

test('GD1.11: a 422 device-bad-name with field device puts the text after the list as a D5.2 Error, marks every checked checkbox aria-invalid with the error in its aria-describedby, and focuses the first checked checkbox', async () => {
  const { bff, main } = await form(
    { device: 'CUDA9,CUDA0' },
    { modelProfileSave: () => badName('device') },
  )
  const fs = fieldset(main)
  const list = fs.querySelector('ul[role="list"]')!
  // CUDA0 unchecked: the device state changed, so Save sends it.
  toggle(main, 'CUDA0')
  fireEvent.click(saveButton(main))
  await waitFor(() =>
    expect(
      within(fs).queryByText(badNameText()),
      'GD1.11: the text',
    ).not.toBeNull(),
  )
  expect(bff.to('PUT', PUT), 'GD1.11: the save was sent').toHaveLength(1)
  const line = within(fs).getByText(badNameText())
  expect(line.tagName, 'GD1.11: a paragraph').toBe('P')
  expect(line.className, 'GD1.11: a D5.2 error').toContain('field-error')
  expect(before(list, line), 'GD1.11: the error is after the list').toBe(true)
  const boxes = rows(main).filter((r) => r.box.checked)
  expect(boxes.map((r) => r.name.split(' ')[0])).toEqual(['CUDA9'])
  for (const r of boxes) {
    expect(r.box.getAttribute('aria-invalid'), `${r.name}: aria-invalid`).toBe(
      'true',
    )
    expect(
      described(r.box).some((t) => t.includes(badNameText().slice(0, 20))),
      `${r.name}: the error describes it`,
    ).toBe(true)
  }
  expect(
    rowOf(main, 'Vulkan0').box.getAttribute('aria-invalid'),
    'GD1.11: the unchecked box is not marked',
  ).not.toBe('true')
  expect(
    document.activeElement,
    'GD1.11: focus on the first checked checkbox',
  ).toBe(boxes[0]!.box)
  expect(alertText(main), 'GD1.11: the bar alert').toContain(
    en('save-refused', badNameText()),
  )
})

test('GD1.11: with no row checked the focus goes to the first checkbox of the list', async () => {
  const { main } = await form(
    { device: 'CUDA9' },
    { modelProfileSave: () => badName('device') },
  )
  toggle(main, 'CUDA9')
  fireEvent.click(saveButton(main))
  await waitFor(() =>
    expect(
      within(fieldset(main)).queryByText(badNameText()),
      'the refusal',
    ).not.toBeNull(),
  )
  expect(document.activeElement, 'GD1.11: focus on the first checkbox').toBe(
    rows(main)[0]!.box,
  )
})

test('GD1.11: checking, unchecking or moving a row clears the fieldset error and keeps the bar alert; an edit that brings the rows back as loaded takes the alert away, silently and with no focus move', async () => {
  const { main } = await form(
    { device: 'CUDA9,CUDA0' },
    { modelProfileSave: () => badName('device') },
  )
  // Vulkan0 checked changes the device state, so Save sends it and the
  // refusal lands.
  toggle(main, 'Vulkan0')
  fireEvent.click(saveButton(main))
  await waitFor(() =>
    expect(
      within(fieldset(main)).queryByText(badNameText()),
      'the refusal',
    ).not.toBeNull(),
  )
  // The form is still changed (Vulkan0 stays checked), so every edit keeps the
  // alert (02h 198b, DESIGN.md 5.2 Save bar).
  for (const [what, act2] of [
    ['checking ROCm0', () => toggle(main, 'ROCm0')],
    ['unchecking ROCm0', () => toggle(main, 'ROCm0')],
    ['moving CUDA0 up', () => move(main, 'CUDA0', UP)],
    ['moving CUDA0 down', () => move(main, 'CUDA0', DOWN)],
  ] as [string, () => void][]) {
    act2()
    expect(
      within(fieldset(main)).queryByText(badNameText()),
      `GD1.11: ${what} clears the error`,
    ).toBeNull()
    for (const r of rows(main).filter((r) => r.box.checked))
      expect(
        r.box.getAttribute('aria-invalid'),
        `GD1.11: ${what}: ${r.name} is not marked`,
      ).not.toBe('true')
    expect(
      within(main).queryAllByRole('alert').length,
      `GD1.11: ${what}: the alert stays`,
    ).toBe(1)
    expect(
      checkedIds(main),
      `GD1.11: ${what}: Vulkan0 is still checked`,
    ).toContain('Vulkan0')
  }
  // The rows are back as loaded, so unchecking Vulkan0 returns the form to its
  // loaded state: the alert goes, silently and with no focus move (the focus
  // stays where the last move left it).
  const focus = document.activeElement
  toggle(main, 'Vulkan0')
  expect(checkedIds(main), 'GD1.11: the rows are as loaded').toEqual([
    'CUDA9',
    'CUDA0',
  ])
  expect(
    within(main).queryAllByRole('alert'),
    'GD1.11: the clean form takes the alert away',
  ).toHaveLength(0)
  expect(
    document.activeElement,
    'GD1.11: the alert goes without moving focus',
  ).toBe(focus)
  expect(
    saveButton(main).getAttribute('aria-disabled'),
    'GD1.11: the form is clean',
  ).toBe('true')
})

test.each(['device-draft', 'mmproj-device'])(
  'GD1.11: a 422 device-bad-name with field %s marks that device select and focuses it, and the GPUs fieldset shows no error',
  async (field) => {
    const id =
      field === 'device-draft'
        ? 'model-profile-device-draft'
        : 'model-profile-mmproj-device'
    // Each device select is enabled only with its own file set (ModelProfile
    // Cards' enable rules).
    const needed: Record<string, string> =
      field === 'device-draft'
        ? { 'model-draft': 'Qwen3-27B-Q8_0-Q4_K_M.gguf' }
        : {
            mmproj: 'mmprojs/mmproj.gguf',
            'mmproj-offload': 'true',
          }
    const { main } = await form(
      { device: 'CUDA9,CUDA0', [field]: 'CUDA9', ...needed },
      { modelProfileSave: () => badName(field) },
    )
    const control = document.getElementById(`${id}-control`)
    expect(control, `GD1.11: field ${field}: the control`).not.toBeNull()
    expect(
      (control as HTMLSelectElement).disabled,
      `GD1.11: field ${field}: the select is enabled`,
    ).toBe(false)
    fireEvent.change(
      document.getElementById('model-profile-ctx-size-control')!,
      {
        target: { value: '65536' },
      },
    )
    fireEvent.click(saveButton(main))
    await waitFor(() =>
      expect(
        within(main).queryByText(badNameText()),
        `GD1.11: field ${field}: the text is shown`,
      ).not.toBeNull(),
    )
    expect(
      control!.getAttribute('aria-invalid'),
      `GD1.11: field ${field}: aria-invalid`,
    ).toBe('true')
    expect(document.activeElement, `GD1.11: field ${field}: focus`).toBe(
      control,
    )
    expect(
      within(fieldset(main)).queryByText(badNameText()),
      'GD1.11: the GPUs fieldset is not the place',
    ).toBeNull()
    for (const r of rows(main).filter((r) => r.box.checked))
      expect(
        r.box.getAttribute('aria-invalid'),
        `GD1.11: field ${field}: the checkboxes are not marked`,
      ).not.toBe('true')
  },
)

test('GD1.11: a 422 device-bad-name with no field falls back to args[0]: the GPUs fieldset carries it', async () => {
  const { main } = await form(
    { device: 'CUDA9,CUDA0' },
    { modelProfileSave: () => badName(undefined) },
  )
  // A move changes the device state, so Save sends it.
  move(main, 'CUDA0', UP)
  fireEvent.click(saveButton(main))
  await waitFor(() =>
    expect(
      within(fieldset(main)).queryByText(badNameText()),
      'GD1.11: the fieldset shows it',
    ).not.toBeNull(),
  )
  expect(rowOf(main, 'CUDA9').box.getAttribute('aria-invalid'), 'marked').toBe(
    'true',
  )
})

test('GD1.11: an untouched device is never sent, so an id the probe does not list cannot be refused by itself', async () => {
  const { bff, main } = await form(
    { device: 'CUDA9' },
    { modelProfileSave: () => badName('device') },
  )
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '65536' },
  })
  await save(main, bff)
  expect(
    sentValues(bff),
    'GD1.11: the save sends the edited field only',
  ).toEqual({ 'ctx-size': '65536' })
})

// ── GD1.12: keyboard, and nothing else ────────────────────────────────────

test('GD1.12: the card tab order is the split mode About toggle, its select, then per row its checkbox, Move up and Move down, and nothing takes focus on arrival', async () => {
  // Real timers: this criterion reads no clock, and the mount helper's
  // `findBy*` needs them.
  const { main } = await form({ device: 'CUDA0,ROCm0' })
  const card = cardOf(main, GPU)
  expect(document.activeElement, 'GD1.12: nothing focused on arrival').toBe(
    document.body,
  )
  const inCard = mainTabbable(main).filter((el) => card.contains(el))
  const names = inCard.map((el) => nameOf(el))
  expect(names.slice(0, 2), 'GD1.12: the About toggle then the select').toEqual(
    [`${en('field-about')} ${MODE_LABEL}`, MODE_LABEL],
  )
  const firstOf = (id: string) => names.findIndex((n) => n.includes(id))
  expect(
    firstOf('CUDA0'),
    'GD1.12: the rows follow the split mode pair, in row order',
  ).toBeGreaterThan(1)
  expect(firstOf('CUDA0'), 'GD1.12: CUDA0 precedes ROCm0').toBeLessThan(
    firstOf('ROCm0'),
  )
  for (const id of ['CUDA0', 'ROCm0']) {
    // The row's own path: its checkbox first, its weight field beside it
    // (GD2), then Move up and Move down (GD1.12).
    const li = rowOf(main, id).li
    const inRow = mainTabbable(main)
      .filter((el) => li.contains(el))
      .map((el) => nameOf(el))
    expect(
      inRow[0],
      `GD1.12: ${id}: the row starts with its checkbox`,
    ).toContain(id)
    expect(
      inRow.slice(-2),
      `GD1.12: ${id}: Move up then Move down end the row`,
    ).toEqual([`${UP} ${id}`, `${DOWN} ${id}`])
  }
  // Auto and Even sit in the list, after the last checked row (GD2.6, GD2.7):
  // where exactly is GD2's, so this criterion pins the split mode pair and the
  // rows only.
})

test('GD1.12: nothing in the card takes focus when the device read lands', async () => {
  const h = held()
  const { main } = await form(
    { device: 'CUDA0,ROCm0' },
    { devices: h.answer },
    true,
  )
  expect(document.activeElement, 'GD1.12: pending: nothing focused').toBe(
    document.body,
  )
  h.resolve(reply(200, DEVICES))
  await landed(main)
  expect(
    document.activeElement,
    'GD1.12: the read landing focuses nothing',
  ).toBe(document.body)
})

test('GD1.12: the strings file gains GD1.1 and GD1.2 keys of section 2, and the page holds no text the strings file does not know', async () => {
  const { main } = await form()
  for (const key of [
    'model-profile-gpu-distribution',
    'model-profile-gpu-distribution-intro',
    'model-profile-gpus',
    'model-profile-gpu-detecting',
    'model-profile-gpu-memory',
    'model-profile-gpu-probed-at',
    'model-profile-gpu-none-found',
    'model-profile-gpu-no-probe',
    'model-profile-gpu-failed',
    'model-profile-gpu-not-detected',
    'model-profile-gpu-move-up',
    'model-profile-gpu-move-down',
    'model-profile-gpu-moved',
    'api-device-bad-name',
  ])
    expect(en(key), `GD1.12: ${key} is a string of the suite`).not.toBe(
      `(${key})`,
    )
  expect(
    within(main).getByRole('heading', { level: 3, name: GPU }),
    'GD1.12: the card is there',
  ).not.toBeNull()
})

// ── GD1.3 and GD1.7: the 40 s limit and no polling, on a fake clock ────────

test('GD1.3, GD1.7: the device read keeps Detecting GPUs at 39 999 ms of its 40 s limit, shows the failure text at 40 s, and the page saves beside it', async () => {
  const h = held()
  const { bff, main } = await form({}, { devices: h.answer }, true)
  const fs = fieldset(main)
  expect(
    within(fs).queryByText(DETECTING),
    'GD1.3: the read is pending',
  ).not.toBeNull()
  await advance(LIMIT - 1)
  expect(
    within(fs).queryByText(DETECTING),
    'GD1.7: 39 999 ms: still detecting',
  ).not.toBeNull()
  expect(within(fs).queryByText(FAILED), 'GD1.7: no failure yet').toBeNull()
  await advance(1)
  expect(
    within(fs).queryByText(DETECTING),
    'GD1.7: the status is gone at 40 s',
  ).toBeNull()
  expect(
    within(fs).queryByText(FAILED),
    'GD1.7: the time-out shows the failure',
  ).not.toBeNull()
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '65536' },
  })
  fireEvent.click(saveButton(main))
  await flush()
  expect(bff.to('PUT', PUT).length, 'GD1.7: the save went out').toBe(1)
  expect(sentValues(bff)['ctx-size'], 'GD1.7: with its value').toBe('65536')
})

test.each([
  ['the read is pending', false],
  ['the read landed', true],
])(
  'GD1.3: nothing polls the device read while %s: a 5 minute clock sends one GET',
  async (_what, land) => {
    const h = held()
    const { bff, main } = await form(
      {},
      { devices: land ? DEVICES : h.answer },
      true,
    )
    if (!land) h.resolve(reply(200, DEVICES))
    await landed(main)
    await advance(300_000)
    expect(
      bff.to('GET', DEVICES_ROUTE).length,
      'GD1.3: one read, no poll',
    ).toBe(1)
  },
)
