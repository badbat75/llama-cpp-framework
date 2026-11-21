// Brief 15, slice C2 (fe/briefs/15-tag-chips.md): the "New tag" box of the
// instance page suggests the tags the node already uses, as the APG combobox
// with a list popup (decision 10). The whole app renders at #/instances/default
// against a fake bff (mocked fetch); "15 C2.n" is the brief's criterion n. The
// chips, the pill and the writes are pinned by InstanceTagChips.test.tsx (C1);
// that file exports no helper, so the few this slice reads are its own. The
// strings half of 15 C2.1 (`instance-tag-suggestions` = "Used tags") is pinned
// by App.test.tsx ("1a strings"), which reads the EN table of test/bff.ts.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
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
  en,
  fakeBff,
  isHidden,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

// The sheet as text: jsdom lays out nothing, so decision 10's "the list is in
// the page's flow, not floating" is read off index.css the way
// InstanceTagChips.test.tsx reads it (comments dropped).
const CSS = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
  .replace(/\/\*[\s\S]*?\*\//g, '')
  .replace(/\s+/g, ' ')

/** Every rule of the sheet whose selector mentions `.tag-options`. */
function optionRules(): { sel: string; body: string }[] {
  return [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .map((m) => ({ sel: (m[1] ?? '').trim(), body: m[2] ?? '' }))
    .filter((r) => r.sel.includes('.tag-options'))
}

/** The C1 item of section 4: it shows `v2`, `gpu`, `edge`. */
const TAG_ITEM = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 8080,
  tags: ['edge', 'v2', 'gpu'],
  instanceTags: ['v2', 'gpu'],
}

/** Section 5's other instance: its tags bring `fast`. */
const ITEM_B = {
  id: 'b',
  engine: 'llama.cpp',
  state: 'inactive',
  port: 8081,
  tags: ['fast', 'gpu'],
  instanceTags: ['fast'],
}

/** Section 5's profiles: `edge` is shown, so `arm` and `zz-top` are suggested. */
const PROFILES: BffOptions['serverProfiles'] = [
  {
    id: 'default',
    engine: 'llama.cpp',
    port: 8080,
    tags: ['edge'],
    instance: 'default',
  },
  {
    id: 'big',
    engine: 'llama.cpp',
    port: 8090,
    tags: ['arm', 'zz-top'],
    instance: null,
  },
]

/** The list route of the server profiles (section 5). */
const SERVER_PROFILES = '/api/profiles/server'

/** The tags write route (section 0). */
const TAGS = '/api/instances/default/tags'

/** The list route the page polls: the polled instance is what moves a label
 *  in or out of the suggestions (decision 10). */
const LIST = '/api/instances'

/** The fixture of section 5. */
const FIXTURE: BffOptions = {
  instances: [TAG_ITEM, ITEM_B],
  serverProfiles: PROFILES,
}

beforeEach(() => {
  startAt('#/instances/default')
})

afterEach(async () => {
  // The fake clock goes first (the polls test): a queued task of a clock test
  // is a real task for the next test, and the flushes below run real timers.
  vi.useRealTimers()
  // A hash set lands as a hashchange in a task of its own: let it finish on
  // this test's page, not on the next test's.
  await flushTasks()
  cleanup()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** A page: the fake bff that answers it, its `<main>`, the tags fieldset. */
type Page = {
  bff: ReturnType<typeof fakeBff>
  main: HTMLElement
  /** The field's group: the legend "Instance tags" names it (section 3). */
  field: HTMLElement
}

/** Lets jsdom's queued tasks (a hashchange, a read's answer) run. Under a
 *  fake clock (`15 C2.3 review`) a real `setTimeout` never fires, so the same
 *  flush goes through the fake one. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) {
      if (vi.isFakeTimers()) await vi.advanceTimersByTimeAsync(0)
      else await new Promise((r) => setTimeout(r, 0))
    }
  })
}

/** Fake timers for the page's 5 s read of `GET /api/instances`; the
 *  microtasks stay real. */
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

/** Lets every answer and render that is due now land; the clock stays. */
async function flush() {
  await act(async () => {
    for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

/** Moves the fake clock on, so the page's poll runs, and lands its answer. */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** The app at the instance page, with section 5's fixture, loaded. Under
 *  `clock` the page's polls run on the fake clock. */
async function openPage(
  options: BffOptions = {},
  clock = false,
): Promise<Page> {
  if (clock) fakeClock()
  const bff = fakeBff({
    boot: { read: () => reply(200, { enabled: true }) },
    ...FIXTURE,
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  if (clock) {
    // Under the fake clock a findBy never resolves: the flush runs the reads
    // due now, then the look is synchronous.
    await flush()
    within(main).getByRole('heading', { level: 1, name: 'default' })
    within(main).getByRole('button', { name: en('save-button') })
  } else {
    await screen.findByRole('heading', { level: 1, name: 'default' })
    await within(main).findByRole('button', { name: en('save-button') })
  }
  return { bff, main, field: group(main) }
}

/** An answer of the profiles list: the bff answers `{"key", "args"}`. */
function refused(status: number, key: string): BffOptions['serverProfiles'] {
  return () => reply(status, { key, args: [] })
}

/** The tags fieldset. */
function group(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: en('instance-tags-own') })
}

/** The Server section's heading: the field's feedback sits before it. */
function serverTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
}

/** True when `el` is the tags field's own feedback: section 3 puts the status
 *  and the alerts with the fieldset, after it and before the Server section. */
function inField(p: Page, el: HTMLElement): boolean {
  return (
    p.field.contains(el) ||
    (before(p.field, el) && before(el, serverTitle(p.main)))
  )
}

/** The field's status line. */
function tagStatus(p: Page): HTMLElement {
  const all = within(p.main)
    .getAllByRole('status')
    .filter((s) => inField(p, s))
  expect(all, 'one status line in the tags field').toHaveLength(1)
  return all[0]!
}

/** The texts of the field's alerts. */
function tagAlertTexts(p: Page): string[] {
  return within(p.main)
    .queryAllByRole('alert')
    .filter((a) => inField(p, a))
    .map((a) => a.textContent ?? '')
}

/** The chips of the instance's own tags: the list items outside the profile's list. */
function ownChips(p: Page): HTMLElement[] {
  const profile = within(p.field).queryByRole('list', {
    name: en('instance-tags-from-profile'),
  })
  return within(p.field)
    .getAllByRole('listitem')
    .filter((li) => profile === null || !profile.contains(li))
}

/** The tag an own chip says: its text, without its Remove button's words. */
function chipText(li: HTMLElement): string {
  const b = within(li).queryByRole('button')
  const all = li.textContent ?? ''
  return (b === null ? all : all.replace(b.textContent ?? '', '')).trim()
}

/** The field's `+` button. */
function addButton(p: Page): HTMLElement {
  return within(p.field).getByRole('button', { name: en('instance-tag-add') })
}

/** The pill's Add and Cancel buttons. */
function pillButton(
  p: Page,
  key: 'instance-tag-confirm' | 'instance-tag-cancel',
): HTMLElement {
  return within(p.field).getByRole('button', { name: en(key) })
}

/** The pill's box: the combobox named by its label "New tag" (15 C2.2). */
function newBox(p: Page): HTMLInputElement {
  return within(p.field).getByRole('combobox', {
    name: en('instance-tag-new'),
  }) as HTMLInputElement
}

/** Clicks `Add tag` and hands back the box of the pill that opened. */
async function openPill(p: Page): Promise<HTMLInputElement> {
  fireEvent.click(addButton(p))
  await flushTasks()
  const box = newBox(p)
  box.focus()
  return box
}

/** Types in the box. */
function typeIn(box: HTMLInputElement, value: string) {
  fireEvent.change(box, { target: { value } })
}

/** Enter in the box, as the keyboard does it. */
function pressEnter(box: HTMLInputElement) {
  pressKey(box, 'Enter')
}

/** A key in the box, as the keyboard does it. */
function pressKey(el: HTMLElement, key: string) {
  fireEvent.keyDown(el, { key, code: key })
  fireEvent.keyUp(el, { key, code: key })
}

/** The listbox the box points at with aria-controls. */
function optionList(box: HTMLElement): HTMLElement {
  const ids = (box.getAttribute('aria-controls') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
  const lists = ids
    .map((id) => document.getElementById(id))
    .filter((el): el is HTMLElement => el !== null)
    .filter((el) => el.matches('[role="listbox"]'))
  expect(
    lists.map((el) => el.id),
    `the box's aria-controls names exactly one listbox (${ids.join(' ') || 'nothing'})`,
  ).toHaveLength(1)
  return lists[0]!
}

/** The options of a shown list. */
function optionsOf(box: HTMLElement): HTMLElement[] {
  const list = optionList(box)
  expect(
    isHidden(list),
    `the list is shown (at ${list.tagName.toLowerCase()}#${list.id})`,
  ).toBe(false)
  return within(list).getAllByRole('option')
}

/** The labels of the options of a shown list, in order. */
function optionTexts(box: HTMLElement): string[] {
  return optionsOf(box).map((o) => (o.textContent ?? '').trim())
}

/** The active option: the one the box's aria-activedescendant names, none when
 *  the box names no id. */
function activeOption(box: HTMLElement): HTMLElement | null {
  const id = (box.getAttribute('aria-activedescendant') ?? '').trim()
  if (id === '') return null
  const el = document.getElementById(id)
  expect(
    el,
    `aria-activedescendant names #${id}: no such element`,
  ).not.toBeNull()
  return el
}

/** The label of the active option, `""` when none is active. */
function activeText(box: HTMLElement): string {
  const el = activeOption(box)
  return el === null ? '' : (el.textContent ?? '').trim()
}

/** Every tab stop of the page, in DOM order. */
function tabbable(main: HTMLElement): HTMLElement[] {
  return [
    ...main.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

/** The PUTs the page sent to the tags route, as bodies. */
function writtenTags(p: Page): string[][] {
  return p.bff.calls
    .filter((c) => c.method === 'PUT' && c.path === TAGS)
    .map((c) => (c.body as { tags?: string[] }).tags ?? [])
}

/** The `GET /api/profiles/server` calls. */
function profileReads(p: Page) {
  return p.bff.to('GET', SERVER_PROFILES)
}

// ── 15 C2.1: the list is read once per opening, and by nothing else ─────

test('15 C2.1: the page reads /api/profiles/server no time before the pill opens, exactly once for each click of Add tag, and no time on typing, adding or the polls', async () => {
  const p = await openPage()
  expect(
    profileReads(p),
    'the list is read when the pill opens, not when the page loads',
  ).toHaveLength(0)

  const box = await openPill(p)
  expect(profileReads(p), 'one read for the opening').toHaveLength(1)
  typeIn(box, 'a')
  pressKey(box, 'ArrowDown')
  pressEnter(box)
  await flushTasks()
  expect(
    profileReads(p),
    'typing, walking and an add read the list again',
  ).toHaveLength(1)

  fireEvent.click(addButton(p))
  await flushTasks()
  expect(
    profileReads(p),
    'a second opening reads it once more (decision 10: once at each opening)',
  ).toHaveLength(2)
})

// ── 15 C2.2: the combobox and its list, and the filter ──────────────────

test('15 C2.2: the box is a combobox with aria-autocomplete=list controlling a hidden listbox named Used tags, expanded by a, by A the same, collapsed by q, and opened by ArrowDown on an empty box', async () => {
  const p = await openPage()
  const box = await openPill(p)

  expect(
    box.getAttribute('aria-autocomplete'),
    'the popup filter pattern (decision 10)',
  ).toBe('list')
  expect(box.getAttribute('aria-expanded'), 'closed on opening').toBe('false')
  const list = optionList(box)
  // Hidden while collapsed, so the role-and-name query below is out of reach:
  // a hidden element is off the accessibility tree (ByRole `hidden`). The
  // hidden state is read as the field's one [role=listbox], its name as the
  // role-and-name match once it is shown.
  const lists = [...p.field.querySelectorAll('[role="listbox"]')]
  expect(lists, 'the field holds exactly one listbox (section 3)').toHaveLength(
    1,
  )
  expect(lists[0], 'aria-controls names the listbox the field holds').toBe(list)
  expect(list.tagName.toLowerCase(), 'the list is a ul (section 3)').toBe('ul')
  expect(isHidden(list), 'hidden while collapsed').toBe(true)
  expect(p.field.contains(list), 'the list is in the tags field').toBe(true)
  expect(
    before(box, list),
    'decision 10: the list follows the chip row the pill sits in, so it shows under it',
  ).toBe(true)
  for (const r of optionRules())
    expect(
      /position:\s*(absolute|fixed)/.test(r.body),
      `decision 10: the list sits in the page's flow under the chip row, not floating: rule "${r.sel}"`,
    ).toBe(false)

  typeIn(box, 'a')
  await flushTasks()
  expect(box.getAttribute('aria-expanded'), 'shown by a').toBe('true')
  expect(
    within(p.field).getByRole('listbox', {
      name: en('instance-tag-suggestions'),
    }),
    'the shown listbox is named Used tags, and is the one aria-controls names',
  ).toBe(list)
  expect(optionTexts(box), 'a matches arm and fast, sorted').toEqual([
    'arm',
    'fast',
  ])
  typeIn(box, 'A')
  await flushTasks()
  expect(optionTexts(box), 'compared lower-cased: A shows the same').toEqual([
    'arm',
    'fast',
  ])
  typeIn(box, 'q')
  await flushTasks()
  expect(isHidden(optionList(box)), 'no match: the list is hidden').toBe(true)
  expect(box.getAttribute('aria-expanded'), 'no match: collapsed').toBe('false')

  typeIn(box, '')
  pressKey(box, 'ArrowDown')
  await flushTasks()
  expect(box.getAttribute('aria-expanded'), 'ArrowDown opens').toBe('true')
  expect(optionTexts(box), 'the empty box shows all three').toEqual([
    'arm',
    'fast',
    'zz-top',
  ])
})

// ── 15 C2.3: walking the list ───────────────────────────────────────────

test('15 C2.3: ArrowDown makes arm active with aria-selected and the box keeping focus, walks to the next and from the last back to the first, ArrowUp back and from the first to the last, and typing leaves nothing active', async () => {
  const p = await openPage()
  const box = await openPill(p)
  typeIn(box, 'a')
  await flushTasks()
  expect(activeText(box), 'nothing active after typing').toBe('')

  pressKey(box, 'ArrowDown')
  const arm = activeOption(box)
  expect(arm, 'ArrowDown makes the first option active').not.toBeNull()
  expect(arm!.textContent?.trim(), 'the active one').toBe('arm')
  expect(
    box.getAttribute('aria-activedescendant'),
    'the box names the active option',
  ).toBe(arm!.id)
  expect(
    arm!.getAttribute('aria-selected'),
    'the active option is selected',
  ).toBe('true')
  expect(document.activeElement, 'focus stays in the box').toBe(box)
  const shown = optionTexts(box)
  expect(shown.length, 'a shows two options').toBeGreaterThan(1)

  pressKey(box, 'ArrowDown')
  expect(activeText(box), 'the next option').toBe('fast')
  pressKey(box, 'ArrowDown')
  expect(activeText(box), 'from the last back to the first').toBe('arm')
  pressKey(box, 'ArrowUp')
  expect(activeText(box), 'back').toBe('fast')
  pressKey(box, 'ArrowUp')
  expect(activeText(box), 'from the first to the last').toBe('arm')
  expect(optionTexts(box), 'walking the list never filters it').toEqual(shown)

  typeIn(box, 'zz')
  await flushTasks()
  expect(activeText(box), 'typing a character leaves nothing active').toBe('')
  expect(box.getAttribute('aria-activedescendant') ?? '', 'no id named').toBe(
    '',
  )
})

// ── 15 C2.4: choosing a suggestion ──────────────────────────────────────

test('15 C2.4: Enter on the active fast sends one PUT with v2, gpu, fast whatever the box holds and lands as 15 C1.6, a click on zz-top sends it, and Enter with nothing active adds the box text as 15 C1.4', async () => {
  const p = await openPage()

  const box = await openPill(p)
  // `f` keeps `fast` in the shown set (decision 10: labels containing the
  // typed text) and is not the tag, so Enter-on-active proves the write takes
  // the active option, not the box text.
  typeIn(box, 'f')
  pressKey(box, 'ArrowDown')
  expect(activeText(box), 'fast active, the box holding f').toBe('fast')
  expect(box.value, 'the box does not hold the active tag').toBe('f')
  pressEnter(box)
  await waitFor(() => expect(writtenTags(p)).toHaveLength(1))
  expect(writtenTags(p), 'the active option, not the box text').toEqual([
    ['v2', 'gpu', 'fast'],
  ])
  expect(
    p.bff.calls.filter((c) => c.method === 'PUT' && c.path === TAGS).length,
    'one write',
  ).toBe(1)
  await waitFor(() =>
    expect(tagStatus(p).textContent).toBe(en('instance-tag-added', 'fast')),
  )
  expect(
    ownChips(p).map(chipText),
    '15 C1.6: the new chip joins the list',
  ).toEqual(['v2', 'gpu', 'fast'])
  expect(
    within(p.field).queryByRole('combobox'),
    '15 C1.6: the pill is gone',
  ).toBeNull()

  const box2 = await openPill(p)
  typeIn(box2, 'zz')
  const zz = optionsOf(box2).find((o) => o.textContent?.trim() === 'zz-top')
  expect(zz, 'zz-top is offered').not.toBeNull()
  fireEvent.click(zz!)
  await waitFor(() => expect(writtenTags(p)).toHaveLength(2))
  expect(writtenTags(p)[1], 'a click adds as Enter does').toEqual([
    'v2',
    'gpu',
    'fast',
    'zz-top',
  ])
  await waitFor(() =>
    expect(ownChips(p).map(chipText)).toEqual(['v2', 'gpu', 'fast', 'zz-top']),
  )

  const box3 = await openPill(p)
  typeIn(box3, 'new-tag')
  pressEnter(box3)
  await waitFor(() => expect(writtenTags(p)).toHaveLength(3))
  expect(
    writtenTags(p)[2],
    '15 C1.4: with nothing active Enter adds what was typed',
  ).toEqual(['v2', 'gpu', 'fast', 'zz-top', 'new-tag'])
})

// ── 15 C2.5: Escape and Tab ────────────────────────────────────────────

test('15 C2.5: Escape with the list shown hides it and keeps the box with its text, a second Escape closes the box, and Tab to Add hides the list, adds nothing and sends the text on the next Tab', async () => {
  const p = await openPage()
  const box = await openPill(p)
  typeIn(box, 'a')
  await flushTasks()
  expect(optionTexts(box), 'the list is shown').toEqual(['arm', 'fast'])

  pressKey(box, 'Escape')
  await flushTasks()
  expect(isHidden(optionList(box)), 'Escape hides the list').toBe(true)
  expect(
    within(p.field).queryByRole('combobox', { name: en('instance-tag-new') }),
    'the box stays open',
  ).toBe(box)
  expect(box.value, 'with its text').toBe('a')
  expect(writtenTags(p), 'Escape adds nothing').toEqual([])

  pressKey(box, 'Escape')
  await flushTasks()
  expect(
    within(p.field).queryByRole('combobox'),
    '15 C1.3: a second Escape closes the box',
  ).toBeNull()
  expect(writtenTags(p), 'nothing was written').toEqual([])

  const box2 = await openPill(p)
  typeIn(box2, 'a')
  await flushTasks()
  const stops = tabbable(p.main)
  expect(stops[stops.indexOf(box2) + 1], 'Tab from the box goes to Add').toBe(
    pillButton(p, 'instance-tag-confirm'),
  )
  pressKey(box2, 'Tab')
  await flushTasks()
  expect(isHidden(optionList(box2)), 'Tab hides the list').toBe(true)
  expect(writtenTags(p), 'Tab adds nothing').toEqual([])
  expect(box2.value, 'the text stays').toBe('a')
  fireEvent.click(pillButton(p, 'instance-tag-confirm'))
  await waitFor(() => expect(writtenTags(p)).toEqual([['v2', 'gpu', 'a']]))
})

// ── 15 C2.6: a profiles read that fails says nothing ───────────────────

test('15 C2.6: on 401, 503, a body that is not JSON, a network failure and while the read is pending, a shows only fast from the instances, and no alert and no status text appear', async () => {
  const failures: Array<[string, BffOptions['serverProfiles']]> = [
    ['401', refused(401, 'session-required')],
    ['503', refused(503, 'api-manager-unavailable')],
    ['not JSON', () => reply(200, '<html>not json</html>')],
    ['no connection', () => Promise.reject(new TypeError('no connection'))],
    ['pending', () => new Promise<Response>(() => {})],
  ]
  for (const [what, answer] of failures) {
    const p = await openPage({ serverProfiles: answer })
    const box = await openPill(p)
    typeIn(box, 'a')
    await flushTasks()
    expect(
      optionTexts(box),
      `${what}: the instances' labels are suggested meanwhile`,
    ).toEqual(['fast'])
    expect(tagAlertTexts(p), `${what}: no alert`).toEqual([])
    expect(
      tagStatus(p).textContent?.trim(),
      `${what}: the status says nothing`,
    ).toBe('')
    expect(
      within(p.main).queryByRole('heading', { level: 1, name: 'default' }),
      `${what}: the page stays`,
    ).not.toBeNull()
    cleanup()
  }
})

// ── 15 C2.7: at most 8 options ─────────────────────────────────────────

test('15 C2.7: with ten labels to offer, an empty box and ArrowDown show 8 options: arm, fast then t0 to t5', async () => {
  const p = await openPage({
    serverProfiles: [
      {
        id: 'big',
        tags: ['arm', 't0', 't1', 't2', 't3', 't4', 't5', 't6', 't7'],
      },
    ],
  })
  const box = await openPill(p)
  pressKey(box, 'ArrowDown')
  await flushTasks()
  // Decision 10's union: the profiles' arm and t0 to t7 plus `fast` from the
  // fixture's instance b (section 5, and 15 C2.6), sorted, capped at 8.
  expect(optionTexts(box), 'sorted union, capped at 8').toEqual([
    'arm',
    'fast',
    't0',
    't1',
    't2',
    't3',
    't4',
    't5',
  ])
})

// ── C2 review: the active option ends with the list, and with the list ───
// The two halves of one rule (decision 10, APG combobox): an option is active
// only while the list that shows it shows. Escape hides the list, so it leaves
// no active option; a poll that changes the labels leaves none either, since
// what the box names by id is a different label after it.

test("15 C2.5 (C2 review): Escape with fast active hides the list and leaves the box naming no option, so the next Enter adds the box's f and not fast", async () => {
  const p = await openPage()
  const box = await openPill(p)
  // `f` keeps `fast` shown (decision 10) and is not the label, so the write
  // says which of the two Enter took.
  typeIn(box, 'f')
  pressKey(box, 'ArrowDown')
  expect(activeText(box), 'fast active, the box holding f').toBe('fast')

  pressKey(box, 'Escape')
  await flushTasks()
  expect(isHidden(optionList(box)), 'Escape hides the list').toBe(true)
  // The list is gone, so no option may stay active: an id naming an option
  // inside a hidden list is a lie to the screen reader, and the next Enter
  // would add the label behind that id.
  const named = activeOption(box)
  expect(
    named,
    `Escape on a hidden list leaves the box naming the hidden option "${named?.textContent?.trim() ?? ''}"`,
  ).toBeNull()
  expect(box.value, 'the box keeps its text').toBe('f')
  expect(document.activeElement, 'focus stays in the box').toBe(box)

  pressEnter(box)
  await waitFor(() => expect(writtenTags(p)).toHaveLength(1))
  expect(writtenTags(p), "the box's text, not the label the list hid").toEqual([
    ['v2', 'gpu', 'f'],
  ])
})

test("15 C2.3 (C2 review): a poll that changes the labels clears the active option, so Enter adds the box's text and never a label that shifted under the id", async () => {
  // The suggestions leave out the labels this instance shows (decision 10),
  // and the instance is polled, so a poll that gives it a tag takes that label
  // out of the list: every option after it changes the label behind its id, and
  // the id the box names goes to a label nobody walked onto.
  const p = await openPage({}, true)
  const box = await openPill(p)
  typeIn(box, 'f')
  pressKey(box, 'ArrowDown')
  expect(activeText(box), 'fast active').toBe('fast')
  expect(
    optionTexts(box),
    'f shows only fast, the list arm, fast, zz-top',
  ).toEqual(['fast'])

  // The poll: the instance gains `arm`, so `fast` becomes the first option and
  // `zz-top` takes the second id, the one the box names.
  p.bff.setInstances([
    {
      ...TAG_ITEM,
      tags: ['edge', 'v2', 'gpu', 'arm'],
      instanceTags: ['v2', 'gpu'],
    },
    ITEM_B,
  ])
  await advance(5_000)
  expect(
    p.bff.to('GET', LIST).length,
    'the page read the list again',
  ).toBeGreaterThanOrEqual(2)
  expect(optionTexts(box), 'the list the poll leaves for f').toEqual(['fast'])

  const named = activeOption(box)
  expect(
    named,
    `the poll changed the labels and the box still names the option now "${named?.textContent?.trim() ?? ''}"`,
  ).toBeNull()
  expect(
    [...optionList(box).querySelectorAll('[role="option"]')]
      .filter((o) => o.getAttribute('aria-selected') === 'true')
      .map((o) => (o.textContent ?? '').trim()),
    'no option of the list is left marked selected',
  ).toEqual([])
  expect(box.value, 'the box keeps its text').toBe('f')

  pressEnter(box)
  // The clock is fake, so no waitFor: the flush lands the write's answer and
  // the render, then the look is synchronous.
  await flush()
  expect(
    writtenTags(p),
    "Enter adds the box's text, not the label that moved under the id",
  ).toEqual([['v2', 'gpu', 'f']])
  expect(
    p.bff.calls.filter((c) => c.method === 'PUT' && c.path === TAGS).length,
    'one write',
  ).toBe(1)
})
