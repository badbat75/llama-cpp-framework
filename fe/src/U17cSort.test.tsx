// Brief 17, slice U17c (fe/briefs/17-ux-pass.md): the Model profiles list sorted by
// its Model profile and Engine headers, the APG sortable table
// (https://www.w3.org/WAI/ARIA/apg/patterns/table/examples/sortable-table/): a native
// button in the header cell holding the header's text and an icon, `aria-sort` on the
// sorted `th` alone, clicks cycling ascending, descending, file order, and the
// filter's status line announcing the change. Comparison is `localeCompare` with
// `{ sensitivity: 'base', numeric: true }`, ties in file order. The sort is kept for
// the tab's life with the filter, and cleared at sign-out as the filter is.
//
// The whole app renders against the fake bff (mocked fetch) on the real clock, as
// ModelProfilesFilter.test.tsx does. "17c.3" is the brief's criterion 3. The look of
// the header buttons (the arrow's size, its gap, the focus ring) is the designer's UX
// pass, not jsdom.
//
// What is NOT here: the smoothness of the deferred filter text (the brief's
// `useDeferredValue`). jsdom sees no frame, and the brief pins no criterion for it
// ("No criterion pins it; jsdom cannot see it"). What is checkable is that the rows
// shown are the filter AND the sort together (17c.6), so every read after typing
// waits: a deferred value lands in a later render.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { createElement, type ReactElement } from 'react'
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
import strings from './strings/en_US.json'
import {
  en,
  fakeBff,
  navigate,
  startAt,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

beforeEach(() => startAt('#/profiles/model'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── The fixture ─────────────────────────────────────────────────────────────
// The four names of the stub's fixture (fe/ux/stub.py), in the file order the list
// shows (switched-on profiles first, the GUI's order), which is NOT the collation
// order: ascending is Old-Coder, Qwen3-27B, Qwen3.6-…, Swift-… (numeric: 3 before 6).
// The engines repeat so a sort by Engine has ties: `llama.cpp` on rows 0, 1 and 3,
// `Strata` on row 2.
function modelProfile(
  id: string,
  engine: string,
  enabled: boolean,
): ModelProfile {
  return {
    id,
    engine,
    engineAvailable: true,
    enabled,
    model: `/m/${id.toLowerCase()}/${id}.gguf`,
  }
}

const SWIFT = 'Swift-Flash-Next-Draft'
const QWEN36 = 'Qwen3.6-35B-A3B-UD-Q4_K_XL'
const QWEN27 = 'Qwen3-27B'
const CODER = 'Old-Coder'

const LIST: ModelProfile[] = [
  modelProfile(SWIFT, 'llama.cpp', true),
  modelProfile(QWEN36, 'llama.cpp', true),
  modelProfile(QWEN27, 'Strata', true),
  modelProfile(CODER, 'llama.cpp', false),
]

/** The file order of the fixture, what the list shows with no sort. */
const FILE_ORDER = LIST.map((p) => p.id)
/** The name column A to Z, the numeric collation the brief names. */
const NAME_ASC = [CODER, QWEN27, QWEN36, SWIFT]
/** The name column Z to A. */
const NAME_DESC = [...NAME_ASC].reverse()
/** The Engine column A to Z: the `llama.cpp` ties in file order, then Strata. */
const ENGINE_ASC = [SWIFT, QWEN36, CODER, QWEN27]
/** The Engine column Z to A: Strata, then the `llama.cpp` ties in file order. */
const ENGINE_DESC = [QWEN27, SWIFT, QWEN36, CODER]

/** The three column names, as the strings file writes them. */
const NAME = en('model-profiles-column-name')
const ENGINE = en('model-profiles-column-engine')
const STATUS = en('model-profiles-column-status')

/** Every column's `aria-sort`, by name: '' where it declares none. */
const sorted = (states: Record<string, string>) => ({
  [NAME]: '',
  [ENGINE]: '',
  [STATUS]: '',
  ...states,
})

// ── Helpers ─────────────────────────────────────────────────────────────────

function open(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfiles: LIST, ...options })
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

async function table(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('table', { name: en('model-profiles-title') })
}

/** The data rows: the rows with no columnheader in them. */
function dataRows(t: HTMLElement): HTMLElement[] {
  return within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

/** The data rows' ids, in the order the table shows them. */
function idsIn(t: HTMLElement): string[] {
  return dataRows(t).map((row) =>
    (row.querySelector('td, th')?.textContent ?? '').trim(),
  )
}

/** The `th` of the column named `label`. */
function headerOf(t: HTMLElement, label: string): HTMLElement {
  return within(t).getByRole('columnheader', { name: label })
}

/** The header cell's sort button, by the column's name. */
function headerButton(t: HTMLElement, label: string): HTMLElement {
  return within(headerOf(t, label)).getByRole('button', { name: label })
}

/** Every column's `aria-sort`, read off the cells. */
function sortStates(t: HTMLElement): Record<string, string> {
  const out: Record<string, string> = {}
  for (const label of [NAME, ENGINE, STATUS])
    out[label] = headerOf(t, label).getAttribute('aria-sort') ?? ''
  return out
}

/** The filter's status line, the element the brief names (17c). */
function filterStatus(main: HTMLElement): HTMLElement {
  const p = main.querySelector('p.model-profiles-filter-status')
  expect(
    p,
    'the filter status line, .model-profiles-filter-status, is on the page',
  ).not.toBeNull()
  expect(p!.getAttribute('role'), 'it is the live region of the list').toBe(
    'status',
  )
  return p as HTMLElement
}

/** The switch status line, the second one of the page (2c.50). */
function switchStatus(main: HTMLElement): HTMLElement {
  const p = main.querySelector('p.model-profiles-switch-status')
  expect(p, 'the switch status line is on the page').not.toBeNull()
  return p as HTMLElement
}

/** The row's on/off switch, by the profile's id (2c: named by the name cell). */
function switchOf(t: HTMLElement, id: string): HTMLInputElement {
  return within(t).getByRole('switch', { name: id }) as HTMLInputElement
}

/** A click where a click really is: focused, then clicked (a keyboard press). */
function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

/** The visible text of an element, spaces collapsed. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The `d` of every shape of the one `.icon` inside `host`, in order. */
function shapeOfIcon(host: Element): string[] {
  const icons = host.querySelectorAll('svg.icon')
  expect(icons.length, 'the button holds one svg.icon').toBe(1)
  const icon = icons[0]!
  expect(
    icon.getAttribute('aria-hidden'),
    'the arrow is decorative: the words say it',
  ).toBe('true')
  expect(text(icon), 'an icon draws, it holds no text').toBe('')
  return [...icon.children].map((el) => el.getAttribute('d') ?? el.tagName)
}

/** Signs in as `user` in the form standing in for the protected page. */
async function signInto(main: HTMLElement, user = 'alice') {
  const username = await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })
  fireEvent.change(username, { target: { value: user } })
  fireEvent.change(
    within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
}

/** The banner's Sign out, waited for: the app lands on the sign-in page. */
async function signOutFromBanner() {
  fireEvent.click(screen.getByRole('button', { name: en('session-sign-out') }))
  await within(screen.getByRole('main')).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
}

// The lucide-static 1.74.0 shapes of the three arrows (17c.1), typed from the
// package's `icons/<name>.svg`, the way the H8a table types the shapes of brief 08:
// the path data is the fact, and a copy of it here reds the case that compares it
// against the component.
const ARROWS: Record<string, string[]> = {
  'arrow-up': ['m5 12 7-7 7 7', 'M12 19V5'],
  'arrow-down': ['M12 5v14', 'm19 12-7 7-7-7'],
  'arrow-up-down': ['m21 16-4 4-4-4', 'M17 20V4', 'm3 8 4-4 4 4', 'M7 4v16'],
}

// The eight attributes of brief 08 section 0 every icon carries. A size, a name and
// a place in the tab order are banned, so the component sets none: H8a.8's sweep over
// every svg of the shell pins the same rule for the shell's icons.
const ICON_ATTRS: Record<string, string> = {
  'aria-hidden': 'true',
  focusable: 'false',
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': '2',
  'stroke-linecap': 'round',
  'stroke-linejoin': 'round',
}

/**
 * `Icon.tsx` through a plain `string` prop: the module's own prop names the
 * `IconName` union, so the cast is the tester's. The path stays in a variable, so the
 * bundler cannot analyse it at transform time: a module that does not load reds the
 * cases that need it, naming the module, never the whole file.
 */
type IconModule = { Icon: (props: { name: string }) => ReactElement }
const ICON_MODULE = './Icon.tsx'
const iconModule = async (): Promise<IconModule> =>
  (await import(ICON_MODULE)) as unknown as IconModule

// ── 17c.1: the strings and the shapes ───────────────────────────────────────

test('17c.1: en_US.json has the three sort texts, exactly as the brief writes them', () => {
  const table_of_strings: Record<string, string> = strings
  for (const [key, english] of [
    ['model-profiles-sorted-asc', 'Sorted by {0}, A to Z.'],
    ['model-profiles-sorted-desc', 'Sorted by {0}, Z to A.'],
    ['model-profiles-sorted-file', 'In file order.'],
  ] as const)
    expect(table_of_strings[key], `${key}: the English of the brief`).toBe(
      english,
    )
})

test.each(Object.keys(ARROWS))(
  '17c.1: Icon.tsx draws %s, the lucide-static 1.74.0 shapes, decorative as every other icon',
  async (name) => {
    const { Icon } = await iconModule()
    const { container } = render(createElement(Icon, { name }))
    const svg = container.querySelector('svg')
    expect(svg, `${name}: the icon is an svg`).not.toBeNull()
    expect(
      svg!.getAttribute('class'),
      `${name}: the class the sheet sizes`,
    ).toBe('icon')
    for (const [key, value] of Object.entries(ICON_ATTRS))
      expect(svg!.getAttribute(key), `${name}: the svg's ${key}`).toBe(value)
    expect(
      [...svg!.children].map((el) => el.getAttribute('d') ?? el.tagName),
      `${name}: the path data of the package, in order`,
    ).toEqual(ARROWS[name])
    expect(text(svg!), `${name}: an icon holds no text`).toBe('')
  },
)

// ── 17c.2: the headers, before anything is clicked ──────────────────────────

test('17c.2: Model profile and Engine each hold one native button named exactly the column, Status holds none, nothing is sorted and the rows are in file order', async () => {
  const { main } = open()
  const t = await table(main)

  for (const label of [NAME, ENGINE]) {
    const th = headerOf(t, label)
    expect(th.tagName, `${label}: a th, the columnheader`).toBe('TH')
    expect(th.getAttribute('scope'), `${label}: scope kept (5b.5)`).toBe('col')
    const buttons = within(th).getAllByRole('button')
    expect(buttons, `${label}: one button in the header cell`).toHaveLength(1)
    const button = buttons[0]!
    expect(button.tagName, `${label}: a native button`).toBe('BUTTON')
    expect(
      button.getAttribute('type'),
      `${label}: type=button, it submits no form`,
    ).toBe('button')
    expect(
      th.querySelector('button'),
      `${label}: the header's text is inside the button`,
    ).toBe(button)
    expect(text(button), `${label}: named exactly the header's text`).toBe(
      label,
    )
    expect(text(th), `${label}: the cell adds no text`).toBe(label)
    expect(
      button.getAttribute('class'),
      `${label}: no new class (17c.9): the rule selects the element`,
    ).toBeNull()
    expect(
      shapeOfIcon(button),
      `${label}: unsorted, so the up-and-down arrow`,
    ).toEqual(ARROWS['arrow-up-down'])
  }

  const statusTh = headerOf(t, STATUS)
  expect(
    within(statusTh).queryByRole('button'),
    'Status is not sortable: switched-on first is already the file order',
  ).toBeNull()
  expect(text(statusTh), 'Status reads as today').toBe(STATUS)

  expect(
    t.querySelectorAll('th[aria-sort]').length,
    'nothing is sorted at first: no th carries aria-sort',
  ).toBe(0)
  expect(sortStates(t)).toEqual(sorted({}))
  expect(idsIn(t), 'the rows are in the order they were read').toEqual(
    FILE_ORDER,
  )
  expect(filterStatus(main).textContent, 'and nothing has been announced').toBe(
    '',
  )
})

// ── 17c.3, 17c.4: the cycle on one column ──────────────────────────────────

test('17c.3: one click on Model profile: the rows A to Z, aria-sort=ascending on its th, the status says it, the arrow up, focus on the button', async () => {
  const { main } = open()
  const t = await table(main)
  const button = headerButton(t, NAME)

  press(button)
  await waitFor(() => expect(idsIn(t)).toEqual(NAME_ASC))
  expect(
    idsIn(t),
    'numeric collation: Qwen3-27B before Qwen3.6 (3 before 6)',
  ).toEqual(NAME_ASC)
  expect(
    sortStates(t),
    'the sorted column says ascending, the others nothing',
  ).toEqual(sorted({ [NAME]: 'ascending' }))
  expect(
    filterStatus(main).textContent,
    'the filter status line announces it',
  ).toBe(en('model-profiles-sorted-asc', NAME))
  expect(shapeOfIcon(button), 'the arrow points up').toEqual(ARROWS['arrow-up'])
  expect(
    document.activeElement,
    'the click left the focus on the button: nothing is moved (1.4.13)',
  ).toBe(button)
})

test('17c.4: the second click is Z to A with the arrow down; the third is file order, no aria-sort, the up-and-down arrow: each step announces itself', async () => {
  const { main } = open()
  const t = await table(main)
  const button = headerButton(t, NAME)

  press(button)
  await waitFor(() => expect(idsIn(t)).toEqual(NAME_ASC))

  press(button)
  await waitFor(() => expect(idsIn(t)).toEqual(NAME_DESC))
  expect(sortStates(t)).toEqual(sorted({ [NAME]: 'descending' }))
  expect(filterStatus(main).textContent).toBe(
    en('model-profiles-sorted-desc', NAME),
  )
  expect(shapeOfIcon(button), 'the arrow points down').toEqual(
    ARROWS['arrow-down'],
  )
  expect(document.activeElement, 'focus stays on the button').toBe(button)

  press(button)
  await waitFor(() => expect(idsIn(t)).toEqual(FILE_ORDER))
  expect(
    t.querySelectorAll('th[aria-sort]').length,
    'file order carries no aria-sort: the list reads as it was read',
  ).toBe(0)
  expect(filterStatus(main).textContent).toBe(en('model-profiles-sorted-file'))
  expect(shapeOfIcon(button), 'the arrows go back to up-and-down').toEqual(
    ARROWS['arrow-up-down'],
  )
  expect(document.activeElement, 'and the focus is still there').toBe(button)
})

// ── 17c.5: the sort moves to the other column ───────────────────────────────

test('17c.5: clicking Engine while sorted by name moves aria-sort to Engine: ascending keeps the ties in file order, descending leads with the last group', async () => {
  const { main } = open()
  const t = await table(main)

  press(headerButton(t, NAME))
  await waitFor(() => expect(idsIn(t)).toEqual(NAME_ASC))

  const engineButton = headerButton(t, ENGINE)
  press(engineButton)
  await waitFor(() => expect(idsIn(t)).toEqual(ENGINE_ASC))
  expect(
    sortStates(t),
    'aria-sort moved to Engine, and the name column lost it',
  ).toEqual(sorted({ [ENGINE]: 'ascending' }))
  expect(
    idsIn(t),
    'the three llama.cpp rows keep the order they were read in',
  ).toEqual(ENGINE_ASC)
  expect(filterStatus(main).textContent).toBe(
    en('model-profiles-sorted-asc', ENGINE),
  )

  press(engineButton)
  await waitFor(() => expect(idsIn(t)).toEqual(ENGINE_DESC))
  expect(sortStates(t)).toEqual(sorted({ [ENGINE]: 'descending' }))
  expect(
    idsIn(t),
    'descending: the ties keep file order (the comparator turns, the rows do not)',
  ).toEqual(ENGINE_DESC)
  expect(filterStatus(main).textContent).toBe(
    en('model-profiles-sorted-desc', ENGINE),
  )
})

// ── 17c.6: sort and filter together ─────────────────────────────────────────

test('17c.6: the sort and the filter combine, and typing in the filter shows the count as today', async () => {
  const { main } = open()
  const t = await table(main)

  const button = headerButton(t, NAME)
  press(button)
  press(button)
  await waitFor(() => expect(idsIn(t)).toEqual(NAME_DESC))

  const field = within(main).getByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  }) as HTMLInputElement
  fireEvent.change(field, { target: { value: 'qwen' } })

  // The deferred text lands in a later render, so every read waits.
  await waitFor(() => expect(idsIn(t)).toEqual([QWEN36, QWEN27]))
  expect(idsIn(t), 'descending, so Qwen3.6 before Qwen3-27B').toEqual([
    QWEN36,
    QWEN27,
  ])
  await waitFor(() =>
    expect(filterStatus(main).textContent).toBe(
      en('model-profiles-filter-count', '2', '4'),
    ),
  )
  expect(sortStates(t)[NAME], 'the column stays sorted').toBe('descending')

  // Clearing the filter leaves the sort alone: every row, in the sorted order.
  fireEvent.change(field, { target: { value: '' } })
  await waitFor(() => expect(idsIn(t)).toEqual(NAME_DESC))
  expect(sortStates(t)[NAME], 'still descending').toBe('descending')
})

// ── The rule beside 17c.6: the announcement behind an active filter ─────────
// The brief's rule ("The filter's status line announces the change; a later
// filter change replaces it with the count as today") is judged here with a
// filter ON, the case 17c.6 does not walk (it sorts first, then types). The
// sentence is matched, not compared: the brief says the count replaces it
// later, not that the announcement stands alone beside the count, so the
// status line reads either the sentence alone or the sentence ahead of the
// count.

test('17c rule: with the filter on, a click on Model profile announces the sort on the status line, and the next filter change shows the count again', async () => {
  const { main } = open()
  const t = await table(main)

  const field = within(main).getByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  }) as HTMLInputElement
  fireEvent.change(field, { target: { value: 'qwen' } })
  await waitFor(() => expect(idsIn(t)).toEqual([QWEN36, QWEN27]))
  await waitFor(() =>
    expect(filterStatus(main).textContent).toBe(
      en('model-profiles-filter-count', '2', '4'),
    ),
  )

  press(headerButton(t, NAME))
  await waitFor(() => expect(idsIn(t)).toEqual([QWEN27, QWEN36]))
  expect(sortStates(t)[NAME], 'the column is sorted ascending').toBe(
    'ascending',
  )
  expect(
    text(filterStatus(main)),
    'the sort is announced even with a filter on: the sentence, alone or ahead of the count',
  ).toContain(en('model-profiles-sorted-asc', NAME))

  // The next filter change replaces it with the count (17c.6, as today).
  fireEvent.change(field, { target: { value: 'qwen3-27' } })
  await waitFor(() => expect(idsIn(t)).toEqual([QWEN27]))
  await waitFor(() =>
    expect(text(filterStatus(main))).toBe(
      en('model-profiles-filter-count', '1', '4'),
    ),
  )
})

// ── 17c.7: a switch while sorted ────────────────────────────────────────────

test('17c.7: switching a profile while sorted keeps the row where it is and the focus on its switch', async () => {
  const { main } = open()
  const t = await table(main)

  const button = headerButton(t, NAME)
  press(button)
  press(button)
  await waitFor(() => expect(idsIn(t)).toEqual(NAME_DESC))

  const sw = switchOf(t, QWEN36)
  press(sw)
  await within(switchStatus(main)).findByText(
    en('model-profiles-switched-off', QWEN36),
  )
  expect(idsIn(t), 'the write did not move the row').toEqual(NAME_DESC)
  expect(
    switchOf(t, QWEN36),
    'the row that was switched is still the one in that place',
  ).toBe(sw)
  expect(document.activeElement, 'focus stayed on the switch').toBe(sw)
})

// ── 17c.8: kept for the tab's life, cleared at sign-out ─────────────────────

test('17c.8: leaving the list and coming back keeps the sort; signing out and signing in again starts in file order', async () => {
  const { main } = open()
  const t = await table(main)

  press(headerButton(t, NAME))
  await waitFor(() => expect(idsIn(t)).toEqual(NAME_ASC))

  // A profile page and back.
  navigate(`#/profiles/model/${QWEN27}`)
  await within(main).findByRole('heading', { level: 1, name: QWEN27 })
  navigate('#/profiles/model')
  const again = await table(main)
  expect(
    idsIn(again),
    'the sort is the shell state of the tab, as the filter is',
  ).toEqual(NAME_ASC)
  expect(headerOf(again, NAME).getAttribute('aria-sort')).toBe('ascending')

  await signOutFromBanner()
  await signInto(main)
  navigate('#/profiles/model')
  const back = await table(main)
  expect(idsIn(back), 'a new session starts in file order').toEqual(FILE_ORDER)
  expect(
    back.querySelectorAll('th[aria-sort]').length,
    'and unsorted: sign-out cleared the sort, as it clears the filter',
  ).toBe(0)
})

// ── 17c.9: the stylesheet ───────────────────────────────────────────────────

// The sheet read as text, the way Reflow.test.ts and U17aSkeletons.test.tsx do
// (comments dropped, the same rule regex). Reflow.test.ts exports no helper, and
// importing a test file would register its tests here, so the reader is a local copy.
const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

const RULES = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1]!.trim().replace(/\s+/g, ' '),
  body: m[2]!,
}))

/** The last value `name` declares in one rule. */
function declared(rule: { body: string }, name: string): string {
  let val = ''
  for (const d of rule.body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name)
      val = d
        .slice(i + 1)
        .trim()
        .replace(/\s+/g, ' ')
  }
  return val
}

/** The value `name` ends up with for the exact selector `sel`, across its rules. */
function valueFor(sel: string, name: string): string {
  let val = ''
  for (const rule of RULES.filter((r) => r.sel === sel))
    val = declared(rule, name) || val
  return val
}

test('17c.9: .model-profiles th button is borderless, takes the header font and colour, and is at least 24 px high, written as that selector: no new class', () => {
  const SEL = '.model-profiles th button'
  expect(
    RULES.filter((r) => r.sel === SEL).length,
    `${SEL} is in the sheet, written as a descendant rule (no class of its own)`,
  ).toBeGreaterThan(0)

  expect(
    ['0', '0px', 'none', '0 none', 'none none'].includes(
      valueFor(SEL, 'border'),
    ),
    'borderless: the border is 0 or none, not a drawn edge',
  ).toBe(true)
  expect(valueFor(SEL, 'color'), `${SEL} colour: the header's`).toBe('inherit')

  // The header's font, by the shorthand or by the three longhands that carry it.
  const longhands = ['font-family', 'font-size', 'font-weight'].map((name) =>
    valueFor(SEL, name),
  )
  expect(
    valueFor(SEL, 'font') === 'inherit' ||
      longhands.every((v) => v === 'inherit'),
    `${SEL} font: inherit, by the shorthand or by family, size and weight (it is text)`,
  ).toBe(true)

  expect(valueFor(SEL, 'min-height'), `${SEL}: H1a.12, a 24 px target`).toBe(
    '24px',
  )
})

test('17c review (fe/briefs/17-ux-pass.md, U17c review): .model-profiles th button declares white-space: normal and text-align: start, so the wrapped header fits the scroll box at 320 px', () => {
  const SEL = '.model-profiles th button'
  // The th keeps nowrap (H2b.3, pinned by Reflow R1.5); the icon and its gap
  // then widen the head row, and at 320 px the table overflows its scroll box
  // by 12 to 15 px in all three engines, cutting the Status switches. The
  // ruling: the button breaks the nowrap, so "Model profile" wraps above its
  // arrow when the column is narrow.
  expect(
    valueFor(SEL, 'white-space'),
    `${SEL}: the nowrap stops at the th: the button wraps, else the header cannot (U17c review)`,
  ).toBe('normal')
  expect(
    valueFor(SEL, 'text-align'),
    `${SEL}: a wrapped header reads from the inline start, as the header text does (U17c review)`,
  ).toBe('start')
})
