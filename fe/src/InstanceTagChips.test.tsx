// Brief 15, slice C1 (fe/briefs/15-tag-chips.md): the instance page's tags as
// chips, one "+" that opens a pill for one label, each add and each removal
// saved at once by `PUT /api/instances/<id>/tags`. The whole app renders at
// #/instances/default against a fake bff (mocked fetch); "15 C1.n" is the
// brief's criterion n. The tile criteria of brief 12 stay in
// InstanceTiles.test.tsx; App.test.tsx ("1a strings") checks the EN table of
// test/bff.ts against strings/en_US.json, so the keys of section 2 are pinned
// from both sides.
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
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  en,
  fakeBff,
  held,
  instanceName,
  isHidden,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

// The sheet as text: jsdom lays out and paints nothing, so the declarations of
// section 10's rulings R3 and R4 are read off index.css the way Reflow.test.ts
// and HearthH2b.test.ts read it (comments dropped, the same rule regex, the
// last value of a property in a block); those files export no helper, and
// importing a test file would register its tests here.
const CSS = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
  .replace(/\/\*[\s\S]*?\*\//g, '')
  .replace(/\s+/g, ' ')

/** The body of the one rule whose selector is exactly `sel`. */
function block(sel: string): string {
  const hits = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter(
    (m) => (m[1] ?? '').trim() === sel,
  )
  expect(
    hits.map((m) => (m[1] ?? '').trim()),
    `exactly one rule whose selector is "${sel}"`,
  ).toEqual([sel])
  return hits[0]![2] ?? ''
}

/** The value of `name` in a block, the last one when it is declared twice. */
function decl(body: string, name: string): string {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

/** Every `name: value` of a block, as written, in order. */
function decls(body: string): string[] {
  return body
    .split(';')
    .map((d) => d.trim())
    .filter((d) => d.includes(':'))
    .map((d) => d.replace(/\s*:\s*/, ': '))
}

/** The sheet with every at-rule block, header included, taken out: the
 *  top-level rules alone, so a selector that also appears inside a media query
 *  is read where it stands on its own. */
function dropAt(css: string): string {
  const out: string[] = []
  let i = 0
  for (;;) {
    const at = css.indexOf('@media', i)
    if (at < 0) {
      out.push(css.slice(i))
      break
    }
    let depth = 0
    let end = css.indexOf('{', at)
    for (; end < css.length; end++) {
      if (css[end] === '{') depth++
      else if (css[end] === '}' && --depth === 0) break
    }
    out.push(css.slice(i, at))
    i = end + 1
  }
  return out.join('')
}

/** The body of the one rule whose selector is exactly `sel`, in the sheet
 *  without media queries. */
function topBlock(sel: string): string {
  const hits = [...dropAt(CSS).matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter(
    (m) => (m[1] ?? '').trim() === sel,
  )
  expect(
    hits.map((m) => (m[1] ?? '').trim()),
    `exactly one rule whose selector is "${sel}" outside any media query`,
  ).toEqual([sel])
  return hits[0]![2] ?? ''
}

/** Every at-rule of the sheet: its header (without the brace) and its body. */
function atRules(): { header: string; body: string }[] {
  const out: { header: string; body: string }[] = []
  const re = /@media[^{}]*\{/g
  for (let m = re.exec(CSS); m !== null; m = re.exec(CSS)) {
    // The match ends at the block's opening brace, so the walk starts inside
    // it: the depth is 1 there, and a nested brace (the rule the block holds)
    // counts on top of it. The body ends at the `}` that closes the block,
    // not at the first inner one.
    let depth = 1
    let end = m.index + m[0].length
    for (; end < CSS.length; end++) {
      if (CSS[end] === '{') depth++
      else if (CSS[end] === '}' && --depth === 0) break
    }
    out.push({
      header: m[0].replace(/\{\s*$/, '').trim(),
      body: CSS.slice(m.index + m[0].length, end),
    })
  }
  return out
}

/** Every top-level rule's selector, in the sheet without media queries. */
const TOP_SELECTORS = [...dropAt(CSS).matchAll(/([^{}]+)\{[^{}]*\}/g)].map(
  (m) => (m[1] ?? '').trim(),
)

/** The fixture of section 4: the union carries one profile-only tag, `edge`. */
const TAG_ITEM = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 8080,
  tags: ['edge', 'v2', 'gpu'],
  instanceTags: ['v2', 'gpu'],
}

/** The tags route (section 0). */
const TAGS = '/api/instances/default/tags'

/** The server profile's save route (brief 16 slice 17a): the chips never
 *  read or write it, and the instance page has no form to save. */
const SERVER = '/api/profiles/server/default'

/** The list route the page polls, and re-reads after every 204. */
const LIST = '/api/instances'

/** The keys of section 2 that C1 keeps and adds, as the EN table spells them. */
const KEYS = [
  'instance-tags-own',
  'instance-tags-hint',
  'instance-tags-from-profile',
  'instance-tag-add',
  'instance-tag-new',
  'instance-tag-confirm',
  'instance-tag-cancel',
  'instance-tag-remove',
  'instance-tag-shown',
  'instance-tags-saving',
  'instance-tag-added',
  'instance-tag-removed',
]

/** The keys of section 2 the brief removes with the text input. */
const GONE = [
  'instance-tags-save',
  'instance-tags-saved',
  'instance-tags-profile',
]

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/instances/default')
  dialogs = stubDialog()
})

afterEach(async () => {
  // The fake clock goes first: a queued task of a clock test is a real task
  // for the next test, and the flushes below run with real timers.
  vi.useRealTimers()
  // A hash set lands as a hashchange in a task of its own: let it finish on
  // this test's page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
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

/**
 * Lets jsdom's queued tasks (a hashchange, a history traversal) run. Under a
 * fake clock (`15 C1.11`) a real `setTimeout` never fires, so the same flush
 * goes through the fake one: the tests of a clock test would otherwise hang.
 */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) {
      if (vi.isFakeTimers()) await vi.advanceTimersByTimeAsync(0)
      else await new Promise((r) => setTimeout(r, 0))
    }
  })
}

/** Fake timers for the page's 5 s read; microtasks stay real. */
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

async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** The app at the instance page, with the fixture, loaded. */
async function openPage(
  options: BffOptions = {},
  clock = false,
): Promise<Page> {
  if (clock) fakeClock()
  const bff = fakeBff({
    instances: [TAG_ITEM],
    boot: { read: () => reply(200, { enabled: true }) },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  if (clock) {
    // Under the fake clock a findBy never resolves: the flush runs the reads
    // due now, then the look is synchronous.
    await flush()
    serverTitle(main)
  } else {
    await screen.findByRole('heading', {
      level: 1,
      name: instanceName(TAG_ITEM),
    })
    await within(main).findByRole('heading', {
      level: 2,
      name: en('instance-server-title'),
    })
  }
  return { bff, main, field: group(main) }
}

/** Lists of items answered as bytes, so a test can answer any shape. */
function itemsAnswer(items: () => unknown[]): BffOptions['instances'] {
  return () => reply(200, JSON.stringify(items()))
}

/** An answer of the tags route: the bff answers `{"key", "args"}` (plus `field`). */
function refused(
  status: number,
  key: string,
  args: string[] = [],
  field?: string,
) {
  return () =>
    reply(status, field === undefined ? { key, args } : { key, args, field })
}

const SESSION_ENDED = refused(401, 'session-required')

/** The rule's refusal, as the manager spells it (section 0). */
const BAD_TAGS = refused(
  422,
  'server-bad-tags',
  ['bad characters', 'v2,gpu,Fast', '8', '1', '32'],
  'tags',
)

/** The 422 of section 4 criterion 7, as the fe words it. */
const BAD_TAGS_TEXT = en(
  'api-server-bad-tags',
  'bad characters',
  'v2,gpu,Fast',
  '8',
  '1',
  '32',
)

/** The tags fieldset. */
function group(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: en('instance-tags-own') })
}

/** The heading of the section brief 16 slice 17a left where the Server form
 *  was: the field's feedback sits before it. */
function serverTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('instance-server-title'),
  })
}

/**
 * True when `el` is the tags field's own Action feedback: section 3 puts the
 * status, the alerts and the Sign in with the fieldset, after it and before the
 * Server section, never in the header's run controls'.
 */
function inField(p: Page, el: HTMLElement): boolean {
  return (
    p.field.contains(el) ||
    (before(p.field, el) && before(el, serverTitle(p.main)))
  )
}

/** The field's status line, in the DOM from the first render. */
function tagStatus(p: Page): HTMLElement {
  const all = within(p.main)
    .getAllByRole('status')
    .filter((s) => inField(p, s))
  expect(all, 'one status line in the tags field').toHaveLength(1)
  return all[0]!
}

/** The field's alerts: `queryAllByRole`, an answered refusal adds none. */
function tagAlerts(p: Page): HTMLElement[] {
  return within(p.main)
    .queryAllByRole('alert')
    .filter((a) => inField(p, a))
}

/** The texts of the field's alerts. */
function tagAlertTexts(p: Page): string[] {
  return tagAlerts(p).map((a) => a.textContent ?? '')
}

/** The field's Sign in buttons, after its alert: `queryAllByRole`, a
 *  sign-in that keeps the page leaves none (15 C1.10). */
function tagSignIns(p: Page): HTMLElement[] {
  return within(p.main)
    .queryAllByRole('button', { name: en('session-sign-in') })
    .filter((b) => inField(p, b))
}

/** The chips of the tags the server profile brings: the list items of the list
 *  named "From the server profile". */
function profileChips(p: Page): HTMLElement[] {
  const list = within(p.field).queryByRole('list', {
    name: en('instance-tags-from-profile'),
  })
  return list === null ? [] : within(list).getAllByRole('listitem')
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

/** The field's `Remove tag <tag>` button. */
function removeButton(p: Page, tag: string): HTMLElement {
  return within(p.field).getByRole('button', {
    name: en('instance-tag-remove', tag),
  })
}

/** The pill's box, named by its visually hidden label "New tag". Brief 15 C2
 *  makes it the combobox of the suggestion list, so it is found by that role. */
function newBox(p: Page): HTMLInputElement {
  return within(p.field).getByRole('combobox', {
    name: en('instance-tag-new'),
  }) as HTMLInputElement
}

/** Clicks the `+` and hands back the box of the pill that opened. */
async function openPill(p: Page): Promise<HTMLInputElement> {
  fireEvent.click(addButton(p))
  await flushTasks()
  return newBox(p)
}

/** The pill's Add and Cancel buttons. */
function pillButton(
  p: Page,
  key: 'instance-tag-confirm' | 'instance-tag-cancel',
): HTMLElement {
  return within(p.field).getByRole('button', { name: en(key) })
}

/** Types in the box. */
function typeIn(box: HTMLInputElement, value: string) {
  fireEvent.change(box, { target: { value } })
}

/** Enter in the box, as the keyboard does it. */
function pressEnter(box: HTMLInputElement) {
  fireEvent.keyDown(box, { key: 'Enter', code: 'Enter' })
  fireEvent.keyUp(box, { key: 'Enter', code: 'Enter' })
}

/** Escape in the box. */
function pressEscape(box: HTMLInputElement) {
  fireEvent.keyDown(box, { key: 'Escape', code: 'Escape' })
  fireEvent.keyUp(box, { key: 'Escape', code: 'Escape' })
}

/** Moves the focus from the box into `into`, as a click on a control does. */
function blurBox(box: HTMLInputElement, into: HTMLElement) {
  into.focus()
  fireEvent.focusOut(box, { relatedTarget: into })
  fireEvent.blur(box, { relatedTarget: into })
}

/** The span a chip's text is in (15 section 10 R3). */
function textSpan(li: HTMLElement): HTMLElement {
  const span = li.querySelector(':scope > span')
  expect(
    span,
    'the chip text is in a span of its own: an ellipsis needs a block box of its own (15 section 10 R3)',
  ).not.toBeNull()
  return span as HTMLElement
}

/** States how much of the chip's text shows and how wide it is: R5 measures the
 *  cut with `span.scrollWidth > span.clientWidth`, and jsdom lays out nothing,
 *  so the test says it. `shown` is the box, `needed` the text: cut when
 *  `needed` is the wider, whole when they are equal. */
function measure(li: HTMLElement, shown: number, needed: number) {
  const span = textSpan(li)
  for (const [name, value] of [
    ['clientWidth', shown],
    ['scrollWidth', needed],
  ] as Array<[string, number]>)
    Object.defineProperty(span, name, { configurable: true, value })
}

/** The expanded pill of 15 section 10 R5, inside the chip, while it shows:
 *  taken away, hidden or `hidden`, all read as not showing. */
function fullPill(li: HTMLElement): HTMLElement | null {
  const el = li.querySelector('.chip-full')
  return el === null || isHidden(el) ? null : (el as HTMLElement)
}

/** The pointer entering and leaving a chip. React 19 builds onMouseEnter and
 *  onMouseLeave from pointerover/pointerout and mouseover/mouseout; a listener
 *  of its own hears the plain mouseenter/mouseleave, so all three go. */
function pointerEnter(li: HTMLElement) {
  fireEvent.pointerOver(li)
  fireEvent.mouseOver(li)
  fireEvent.mouseEnter(li)
}
function pointerLeave(li: HTMLElement, to: HTMLElement) {
  fireEvent.pointerOut(li, { relatedTarget: to })
  fireEvent.mouseOut(li, { relatedTarget: to })
  fireEvent.mouseLeave(li, { relatedTarget: to })
}

/** Escape, where a document-level listener hears it too. */
function pressEscapeEverywhere() {
  fireEvent.keyDown(document.body, { key: 'Escape', code: 'Escape' })
}

/** The texts of the elements `el` points at with aria-describedby. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
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

/** Asks the browser whether leaving the page would be held. */
function asksBrowser(): boolean {
  const e = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(e)
  return e.defaultPrevented
}

/** The PUTs the page sent to the tags route. */
function tagWrites(bff: ReturnType<typeof fakeBff>) {
  return bff.calls.filter((c) => c.method === 'PUT' && c.path === TAGS)
}

/** A `PUT /api/instances/default/tags` answer that records body and headers. */
function recorder() {
  const bodies: unknown[] = []
  const types: (string | null)[] = []
  const answer = (
    body: unknown,
    _id: string,
    _n: number,
    init?: RequestInit,
  ) => {
    bodies.push(body)
    types.push(new Headers(init?.headers).get('content-type'))
    return new Response(null, { status: 204 })
  }
  return { bodies, types, answer }
}

/** Types the password in the open sign-in dialog and submits it. */
async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
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

/** The nav's Overview link. */
function navLink(name: string): HTMLElement {
  return within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).getByRole('link', { name })
}

/** Clicks a nav link, so the router sees it. */
async function goLink(name: string) {
  await act(async () => {
    fireEvent.click(navLink(name))
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** The Unsaved changes dialog, open. */
async function discardDialog(): Promise<HTMLElement> {
  const d = await screen.findByRole('dialog', { name: en('discard-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  return d
}

/** Types a hash, so the browser gains a history entry (the LeaveGuard shape). */
async function typeHash(hash: string) {
  await act(async () => {
    window.location.hash = hash
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** The browser's Back button: jsdom traverses in a task. */
async function pressBack() {
  await act(async () => {
    window.history.back()
    await new Promise((r) => setTimeout(r, 0))
    await new Promise((r) => setTimeout(r, 0))
  })
}

/**
 * The page reached by a typed hash from the Overview, so the browser has an
 * entry to go Back to (the shape the leave-guard tests use).
 */
async function openPageFromOverview(): Promise<Page> {
  startAt('#/')
  const bff = fakeBff({
    instances: [TAG_ITEM],
    boot: { read: () => reply(200, { enabled: true }) },
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  await typeHash('#/instances/default')
  await within(main).findByRole('heading', {
    level: 1,
    name: instanceName(TAG_ITEM),
  })
  await within(main).findByRole('heading', {
    level: 2,
    name: en('instance-server-title'),
  })
  return { bff, main, field: group(main) }
}

/** Unmounts and clears the fetch stub, so the next `openPage` starts clean. */
async function closePage() {
  cleanup()
  await flushTasks()
  vi.unstubAllGlobals()
  startAt('#/instances/default')
}

// ── 15 C1.1: the texts ──────────────────────────────────────────────────

test('15 C1.1: en_US.json has the keys of section 2 with exactly its text, and no longer has the three of the text input', () => {
  const table: Record<string, string> = strings
  expect(
    KEYS.filter((key) => table[key] === undefined),
    'keys missing from strings/en_US.json',
  ).toEqual([])
  expect(
    KEYS.filter((key) => table[key] !== en(key)),
    'keys whose en_US.json text is not the English of the brief',
  ).toEqual([])
  expect(
    GONE.filter((key) => table[key] !== undefined),
    'keys the brief removed with the text input',
  ).toEqual([])
})

// ── 15 C1.2: the field as the page opens it ─────────────────────────────

test('15 C1.2: the group named Instance tags holds a list with the items v2 and gpu in that order, each with its Remove tag button, then a button Add tag, and no textbox and no Save tags button', async () => {
  const p = await openPage()
  const chips = ownChips(p)
  expect(chips, 'two own chips').toHaveLength(2)
  expect(chips.map(chipText), 'the tags, in the read order').toEqual([
    'v2',
    'gpu',
  ])
  for (const [li, tag] of [
    [chips[0]!, 'v2'],
    [chips[1]!, 'gpu'],
  ] as Array<[HTMLElement, string]>) {
    expect(
      within(li).getByRole('button', {
        name: en('instance-tag-remove', tag),
      }),
      `the chip ${tag} holds its own Remove button`,
    ).toBeTruthy()
  }
  expect(before(chips[1]!, addButton(p)), 'the + follows the last chip').toBe(
    true,
  )
  expect(within(p.field).queryByRole('textbox'), 'no textbox').toBeNull()
  expect(
    screen.queryByRole('button', { name: 'Save tags' }),
    'the text input Save tags button is gone',
  ).toBeNull()
})

test('15 C1.2: after the group chips, the text From the server profile names a list with the one item edge and no button in it', async () => {
  const p = await openPage()
  const chips = ownChips(p)
  const text = within(p.field).getByText(en('instance-tags-from-profile'))
  const list = within(p.field).getByRole('list', {
    name: en('instance-tags-from-profile'),
  })
  expect(
    before(chips[chips.length - 1]!, text),
    'the text follows the own chips',
  ).toBe(true)
  expect(before(text, list), 'the list follows its text').toBe(true)
  expect(
    within(list).getAllByRole('listitem').map(chipText),
    'the profile brings edge',
  ).toEqual(['edge'])
  expect(within(list).queryAllByRole('button'), 'no button in it').toEqual([])
})

test('15 C1.2 (as 15 section 10 R5 amends it): the tab order is Start at boot, About Instance tags, Remove tag v2, Remove tag gpu, Add tag, the profile chip edge, last (17a.10: the form left the page)', async () => {
  const p = await openPage()
  // Start at boot is read on its own after the page opens: the tab stop list
  // is taken once its checkbox is there, so the snapshot is never of a page
  // still loading the field.
  const boot = await within(p.main).findByRole('checkbox', {
    name: en('boot-toggle'),
  })
  const stops = tabbable(p.main)
  const order = [
    boot,
    // 16 17f2.8: the display name pencil is the page's first stop, before
    // Start, so it is outside this window, which starts at Start at boot.
    within(p.main).getByRole('button', {
      name: `${en('field-about')} ${en('instance-tags-own')}`,
    }),
    removeButton(p, 'v2'),
    removeButton(p, 'gpu'),
    addButton(p),
    // R5: a profile chip has no button, so the keyboard reaches its text
    // through the chip itself, after the +.
    profileChips(p)[0]!,
  ]
  const at = order.map((el) => stops.indexOf(el))
  expect(at, 'every one is a tab stop').not.toContain(-1)
  expect(
    stops.slice(at[0]! + 1, at[order.length - 1]! + 1),
    'nothing else between Start at boot and the Server section',
  ).toEqual(order.slice(1))
})

test('15 C1.2: tags equal to instanceTags: no From the server profile text and no second list; with both lists empty the group holds only Add tag', async () => {
  const same = await openPage({
    instances: itemsAnswer(() => [
      { ...TAG_ITEM, tags: ['v2', 'gpu'], instanceTags: ['v2', 'gpu'] },
    ]),
  })
  expect(
    within(same.field).queryByText(en('instance-tags-from-profile')),
    'the profile brings nothing',
  ).toBeNull()
  expect(
    within(same.field).queryAllByRole('list'),
    'one list, the own one',
  ).toHaveLength(1)
  await closePage()

  const empty = await openPage({
    instances: itemsAnswer(() => [{ ...TAG_ITEM, tags: [], instanceTags: [] }]),
  })
  expect(within(empty.field).queryAllByRole('list'), 'no chips at all').toEqual(
    [],
  )
  expect(
    within(empty.field).queryAllByRole('button'),
    'the group holds only Add tag',
  ).toHaveLength(1)
  expect(addButton(empty).textContent, 'its words').toBe(en('instance-tag-add'))
  expect(
    empty.field.querySelector('input'),
    'no box while the pill is closed',
  ).toBeNull()
})

// ── 15 C1.3: opening and closing the pill ───────────────────────────────

test('15 C1.3: Add tag opens a box named New tag, empty and focused, replacing the +, followed by Add (aria-disabled while the box is empty or spaces) and Cancel', async () => {
  const p = await openPage()
  const box = await openPill(p)
  expect(box.value, 'empty').toBe('')
  expect(document.activeElement, 'focused on opening').toBe(box)
  expect(
    within(p.field).queryByRole('button', { name: en('instance-tag-add') }),
    'the + is replaced in place',
  ).toBeNull()
  expect(
    ownChips(p).map(chipText),
    // The profile's `edge` is a listitem of the field too: the own chips.
    'the chips stay',
  ).toEqual(['v2', 'gpu'])
  const add = pillButton(p, 'instance-tag-confirm')
  expect(add.getAttribute('aria-disabled'), 'empty: Add reads disabled').toBe(
    'true',
  )
  typeIn(box, '   ')
  expect(
    add.getAttribute('aria-disabled'),
    'spaces only: Add reads disabled',
  ).toBe('true')
  typeIn(box, 'fast')
  expect(add.getAttribute('aria-disabled'), 'text: Add is live').toBe(null)
  expect(pillButton(p, 'instance-tag-cancel')).toBeTruthy()
})

test('15 C1.3: Escape, Cancel, and blurring an empty box close the pill with Add tag back and focused; blurring a box holding fast keeps it open holding fast; nothing is sent', async () => {
  const p = await openPage()
  const boot = await within(p.main).findByRole('checkbox', {
    name: en('boot-toggle'),
  })
  for (const close of ['Escape', 'Cancel', 'blur'] as const) {
    const box = await openPill(p)
    if (close === 'Escape') pressEscape(box)
    else if (close === 'Cancel')
      fireEvent.click(pillButton(p, 'instance-tag-cancel'))
    else blurBox(box, boot)
    await flushTasks()
    expect(
      p.field.querySelector('input'),
      `${close}: the box is gone`,
    ).toBeNull()
    const add = addButton(p)
    expect(add, `${close}: the + is back`).toBeTruthy()
    expect(document.activeElement, `${close}: focus on the +`).toBe(add)
  }
  const open = await openPill(p)
  typeIn(open, 'fast')
  blurBox(open, pillButton(p, 'instance-tag-confirm'))
  await flushTasks()
  expect(newBox(p).value, 'text kept: the pill stays open').toBe('fast')
  expect(tagWrites(p.bff), 'none of this sends a request').toEqual([])
})

// ── 15 C1.4: what a confirmed label sends ───────────────────────────────

test('15 C1.4: Enter in a box holding " fast " sends exactly one PUT with the tag appended; so does Add; an empty or blank box sends nothing; Fast is sent as Fast', async () => {
  const rec = recorder()
  const p = await openPage({ instanceTags: rec.answer })
  const enter = await openPill(p)
  typeIn(enter, ' fast ')
  pressEnter(enter)
  await waitFor(() => expect(tagWrites(p.bff)).toHaveLength(1))
  expect(
    rec.bodies[0],
    'the latched list with the trimmed tag appended',
  ).toEqual({ tags: ['v2', 'gpu', 'fast'] })
  expect(rec.types[0], 'the body is JSON').toMatch(/application\/json/)

  const box = await openPill(p)
  typeIn(box, ' fast ')
  fireEvent.click(pillButton(p, 'instance-tag-confirm'))
  await waitFor(() => expect(tagWrites(p.bff)).toHaveLength(2))
  expect(rec.bodies[1], 'Add sends the same').toEqual({
    tags: ['v2', 'gpu', 'fast'],
  })

  for (const blank of ['', '   ']) {
    const box = await openPill(p)
    typeIn(box, blank)
    pressEnter(box)
    fireEvent.click(pillButton(p, 'instance-tag-confirm'))
    await flushTasks()
    expect(
      tagWrites(p.bff),
      `a box holding ${JSON.stringify(blank)} sends nothing`,
    ).toHaveLength(2)
  }

  const upper = await openPill(p)
  typeIn(upper, 'Fast')
  pressEnter(upper)
  await waitFor(() => expect(tagWrites(p.bff)).toHaveLength(3))
  expect(rec.bodies[2], 'the case is kept, the word is not touched').toEqual({
    tags: ['v2', 'gpu', 'Fast'],
  })
})

test('15 C1.4: typing gpu, or edge, and Enter sends nothing and shows gpu is already a tag of this instance. under the box, which is aria-invalid, described by that text, and focused', async () => {
  const p = await openPage()
  for (const tag of ['gpu', 'edge']) {
    const text = en('instance-tag-shown', tag)
    const box = await openPill(p)
    typeIn(box, tag)
    pressEnter(box)
    await flushTasks()
    const error = within(p.field).getByText(text)
    const now = newBox(p)
    expect(
      within(p.main).queryByText(/^Saving/),
      'the status line says nothing',
    ).toBeNull()
    expect(tagWrites(p.bff), 'nothing is sent').toEqual([])
    expect(before(now, error), 'the text is under the box').toBe(true)
    expect(now.getAttribute('aria-invalid'), 'the box is invalid').toBe('true')
    expect(described(now), 'the box is described by the text').toContain(text)
    expect(document.activeElement, 'the focus is on the box').toBe(now)
    expect(now.value, 'the box keeps what was typed').toBe(tag)
    // The pill stays open, so the user can fix the label.
    fireEvent.click(pillButton(p, 'instance-tag-cancel'))
    await flushTasks()
  }
})

// ── 15 C1.5: one write at a time ────────────────────────────────────────

test('15 C1.5 (as 15 section 10 R6 amends it): while a PUT is pending the new chip is already listed, the status reads Saving tags, the box is readonly with its text and keeps focus, Add and every Remove button read aria-disabled, and a second Enter, Add or Remove sends nothing', async () => {
  const h = held()
  let seen = 0
  const p = await openPage({
    instanceTags: (_body, _id, _n, init) => {
      seen += 1
      return h.answer(init)
    },
  })
  const box = await openPill(p)
  typeIn(box, 'fast')
  pressEnter(box)
  await waitFor(() => expect(seen).toBe(1))
  expect(
    ownChips(p).map(chipText),
    'R6: the add is optimistic, the chip is listed while the write is on its way',
  ).toEqual(['v2', 'gpu', 'fast'])
  expect(tagStatus(p).textContent, 'the status announces the send').toBe(
    en('instance-tags-saving'),
  )
  expect(box.readOnly, 'the box is readonly').toBe(true)
  expect(box.value, 'R6: the pill keeps its text until the answer').toBe('fast')
  expect(document.activeElement, 'the box keeps focus').toBe(box)
  for (const [what, el] of [
    ['Add', pillButton(p, 'instance-tag-confirm')],
    ['Remove tag v2', removeButton(p, 'v2')],
    ['Remove tag gpu', removeButton(p, 'gpu')],
    ['Remove tag fast', removeButton(p, 'fast')],
  ] as Array<[string, HTMLElement]>) {
    expect(el.getAttribute('aria-disabled'), `${what} reads disabled`).toBe(
      'true',
    )
  }
  pressEnter(box)
  fireEvent.click(pillButton(p, 'instance-tag-confirm'))
  fireEvent.click(removeButton(p, 'v2'))
  fireEvent.click(removeButton(p, 'gpu'))
  expect(seen, 'a second Enter, Add or Remove sends nothing').toBe(1)
  h.resolve(new Response(null, { status: 204 }))
  await flushTasks()
})

test('15 C1.5: during a removal the + reads aria-disabled, its status reads Saving tags, and a second click on it and a second Remove send nothing', async () => {
  const h = held()
  let seen = 0
  const p = await openPage({
    instanceTags: (_body, _id, _n, init) => {
      seen += 1
      return h.answer(init)
    },
  })
  fireEvent.click(removeButton(p, 'v2'))
  await waitFor(() => expect(seen).toBe(1))
  expect(tagStatus(p).textContent, 'the status announces the send').toBe(
    en('instance-tags-saving'),
  )
  const add = addButton(p)
  expect(add.getAttribute('aria-disabled'), 'the + reads disabled').toBe('true')
  fireEvent.click(add)
  fireEvent.click(removeButton(p, 'gpu'))
  expect(seen, 'nothing else is sent').toBe(1)
  h.resolve(new Response(null, { status: 204 }))
  await flushTasks()
})

// ── 15 C1.6: the answer of an add ───────────────────────────────────────

test('15 C1.6: on 204 for fast the list holds v2, gpu, fast in that order, the box is gone, Add tag is back and focused, the status reads Tag fast added., and the list is read again', async () => {
  const p = await openPage()
  const box = await openPill(p)
  typeIn(box, 'fast')
  pressEnter(box)
  await waitFor(() =>
    expect(tagStatus(p).textContent).toBe(en('instance-tag-added', 'fast')),
  )
  expect(p.field.querySelector('input'), 'the box is gone').toBeNull()
  expect(ownChips(p).map(chipText), 'the new chip is last').toEqual([
    'v2',
    'gpu',
    'fast',
  ])
  const add = addButton(p)
  expect(document.activeElement, 'focus on the +').toBe(add)
  expect(tagStatus(p).textContent, 'the status says the tag').toBe(
    en('instance-tag-added', 'fast'),
  )
  const calls = p.bff.calls
  const put = calls.findIndex((c) => c.method === 'PUT' && c.path === TAGS)
  expect(
    calls.findIndex((c, i) => i > put && c.method === 'GET' && c.path === LIST),
    'the page asks a fresh read of the list',
  ).toBeGreaterThan(put)
})

// ── 15 C1.7: the rule refuses an add ────────────────────────────────────

test('15 C1.7: a 422 server-bad-tags is the box error: under it, the box aria-invalid, described by it and focused, holding Fast, the status empty, no alert; one keystroke clears it', async () => {
  const p = await openPage({ instanceTags: BAD_TAGS })
  const box = await openPill(p)
  typeIn(box, 'Fast')
  pressEnter(box)
  const error = await within(p.field).findByText(BAD_TAGS_TEXT)
  const now = newBox(p)
  expect(before(now, error), 'the text is under the box').toBe(true)
  expect(now.getAttribute('aria-invalid'), 'the box is invalid').toBe('true')
  expect(described(now), 'the box is described by the text').toContain(
    BAD_TAGS_TEXT,
  )
  expect(document.activeElement, 'the focus is on the box').toBe(now)
  expect(now.value, 'the box keeps Fast').toBe('Fast')
  expect(tagStatus(p).textContent, 'the status line is empty').toBe('')
  expect(tagAlerts(p), 'no alert: the refusal is the input error').toEqual([])
  fireEvent.change(now, { target: { value: 'fast' } })
  expect(
    now.getAttribute('aria-invalid'),
    'one keystroke clears the invalid mark',
  ).toBe(null)
  expect(described(now), 'and the error').not.toContain(BAD_TAGS_TEXT)
})

// ── 15 C1.8: removing a chip ────────────────────────────────────────────

test('15 C1.8 (as 15 section 10 R6 amends it): Remove tag v2 sends one PUT with the rest; while it is pending v2 is already gone and focus is on Remove tag gpu; on 204 the status reads Tag v2 removed., and the list is read again', async () => {
  const h = held()
  const bodies: unknown[] = []
  const p = await openPage({
    instanceTags: (body, _id, _n, init) => {
      bodies.push(body)
      return h.answer(init)
    },
  })
  fireEvent.click(removeButton(p, 'v2'))
  await flushTasks()
  expect(bodies, 'the latched list without v2').toEqual([{ tags: ['gpu'] }])
  expect(tagWrites(p.bff).length, 'one PUT').toBe(1)
  expect(
    ownChips(p).map(chipText),
    'R6: the removal is optimistic, the chip leaves at once',
  ).toEqual(['gpu'])
  expect(
    document.activeElement,
    'R6: focus moves to the next x at once, as decision 5 says',
  ).toBe(removeButton(p, 'gpu'))
  expect(
    tagStatus(p).textContent,
    'the status announces the send, with the chip already gone',
  ).toBe(en('instance-tags-saving'))
  h.resolve(new Response(null, { status: 204 }))
  await flushTasks()
  expect(ownChips(p).map(chipText), 'v2 is gone').toEqual(['gpu'])
  expect(document.activeElement, 'focus on the next Remove').toBe(
    removeButton(p, 'gpu'),
  )
  expect(tagStatus(p).textContent, 'the status says the tag').toBe(
    en('instance-tag-removed', 'v2'),
  )
  const calls = p.bff.calls
  const put = calls.findIndex((c) => c.method === 'PUT' && c.path === TAGS)
  expect(
    calls.findIndex((c, i) => i > put && c.method === 'GET' && c.path === LIST),
    'the page asks a fresh read of the list',
  ).toBeGreaterThan(put)
})

test('15 C1.8: removing gpu, the last chip, moves focus to Remove tag v2; removing the only chip moves it to Add tag', async () => {
  const p = await openPage()
  fireEvent.click(removeButton(p, 'gpu'))
  await waitFor(() =>
    expect(tagStatus(p).textContent).toBe(en('instance-tag-removed', 'gpu')),
  )
  expect(ownChips(p).map(chipText), 'gpu is gone').toEqual(['v2'])
  expect(document.activeElement, 'focus on the previous Remove').toBe(
    removeButton(p, 'v2'),
  )
  await closePage()

  const alone = await openPage({
    instances: itemsAnswer(() => [
      { ...TAG_ITEM, tags: ['solo'], instanceTags: ['solo'] },
    ]),
  })
  fireEvent.click(removeButton(alone, 'solo'))
  await waitFor(() => expect(ownChips(alone), 'the chip is gone').toEqual([]))
  expect(document.activeElement, 'focus on the +').toBe(addButton(alone))
})

// ── 15 C1.9: each other answer ──────────────────────────────────────────

describe('15 C1.9: each other answer shows in the field alert, the status stays empty, and the page keeps what the user had', () => {
  const shapes: [string, () => Response | Promise<Response>, string][] = [
    ['401 session-required', SESSION_ENDED, en('session-ended-retry')],
    [
      '403 session-not-administrator',
      refused(403, 'session-not-administrator'),
      en('session-revoked'),
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
    [
      '403 client-not-authorized',
      refused(403, 'client-not-authorized'),
      en('api-client-not-authorized'),
    ],
    [
      '422 with an unknown key',
      refused(422, 'x-y', [], 'tags'),
      en('api-other', 'x-y'),
    ],
    [
      '400 tags-malformed',
      refused(400, 'tags-malformed'),
      en('request-failed'),
    ],
    ['502 client-other', refused(502, 'client-other'), en('request-failed')],
    [
      'a body that is not JSON',
      () => reply(500, 'not json'),
      en('request-failed'),
    ],
    [
      'a network failure',
      () => Promise.reject(new TypeError('Failed to fetch')),
      en('request-failed'),
    ],
  ]
  for (const [what, answer, text] of shapes) {
    test(`15 C1.9: ${what} -> ${text}, on an add and on a removal`, async () => {
      // An add: the pill stays open with its text, the alert in the field.
      const add = await openPage({ instanceTags: answer })
      const box = await openPill(add)
      typeIn(box, 'new-tag')
      pressEnter(box)
      const el = await within(add.main).findByText(text)
      expect(
        el.closest('[role="alert"]'),
        `${what}: the text is in a role="alert"`,
      ).not.toBeNull()
      expect(inField(add, el), `${what}: the alert is the field's`).toBe(true)
      expect(tagStatus(add).textContent, `${what}: the status is empty`).toBe(
        '',
      )
      expect(newBox(add).value, `${what}: the box stays open`).toBe('new-tag')
      await closePage()

      // A removal (R6): the chip leaves at once, and the refusal puts it back
      // in its place, with the focus on its x.
      const away = await openPage({ instanceTags: answer })
      fireEvent.click(removeButton(away, 'v2'))
      const alert = await within(away.main).findByText(text)
      expect(
        alert.closest('[role="alert"]'),
        `${what}: the removal text is in a role="alert"`,
      ).not.toBeNull()
      expect(
        ownChips(away).map(chipText),
        `${what}: the chip is back, from the latched list, in its place`,
      ).toEqual(['v2', 'gpu'])
      expect(
        document.activeElement,
        `${what}: focus back on the restored chip's x`,
      ).toBe(removeButton(away, 'v2'))
      expect(tagStatus(away).textContent, `${what}: the status is empty`).toBe(
        '',
      )
      await closePage()
    })
  }

  test('15 C1.9: a 422 server-bad-tags to a removal, with no box to put it under, is the field alert with the manager text', async () => {
    const p = await openPage({ instanceTags: BAD_TAGS })
    fireEvent.click(removeButton(p, 'v2'))
    const el = await within(p.main).findByText(BAD_TAGS_TEXT)
    expect(el.closest('[role="alert"]'), 'role="alert"').not.toBeNull()
    expect(inField(p, el), 'the field alert').toBe(true)
    expect(tagStatus(p).textContent, 'the status line is empty').toBe('')
  })

  test('15 C1.9: the 401 and the 403 session-not-administrator give the field one Sign in, after its alert', async () => {
    for (const [answer, text] of [
      [SESSION_ENDED, en('session-ended-retry')],
      [refused(403, 'session-not-administrator'), en('session-revoked')],
    ] as Array<[() => Response, string]>) {
      const p = await openPage({ instanceTags: answer })
      fireEvent.click(removeButton(p, 'v2'))
      await within(p.main).findByText(text)
      const signIns = tagSignIns(p)
      expect(signIns, 'one Sign in in the field').toHaveLength(1)
      expect(
        before(tagAlerts(p)[0]!, signIns[0]!),
        'Sign in after the alert',
      ).toBe(true)
      await closePage()
    }
  })
})

// ── 15 C1.10: the sign-in from the field ────────────────────────────────

test('15 C1.10: after a 401, Sign in opens the dialog; a sign-in that keeps the page closes it, removes the alert, and focuses the New tag box with its text kept, or Add tag when the pill is closed; nothing is resent', async () => {
  // The add: the pill is open, its text kept.
  const add = await openPage({ instanceTags: SESSION_ENDED })
  const box = await openPill(add)
  typeIn(box, 'fast')
  pressEnter(box)
  await within(add.main).findByText(en('session-ended-retry'))
  fireEvent.click(tagSignIns(add)[0]!)
  await flushTasks()
  await signInInDialog('alice')
  expect(tagAlertTexts(add), 'the alert is gone').toEqual([])
  expect(tagSignIns(add), 'and its Sign in').toEqual([])
  const kept = newBox(add)
  expect(kept.value, 'the text typed is kept').toBe('fast')
  expect(document.activeElement, 'focus on the box').toBe(kept)
  expect(tagWrites(add.bff), 'the refused write is not resent').toHaveLength(1)
  await closePage()

  // The removal: no box, so the + takes focus.
  const away = await openPage({ instanceTags: SESSION_ENDED })
  fireEvent.click(removeButton(away, 'v2'))
  await within(away.main).findByText(en('session-ended-retry'))
  fireEvent.click(tagSignIns(away)[0]!)
  await flushTasks()
  await signInInDialog('alice')
  expect(tagAlertTexts(away), 'the alert is gone').toEqual([])
  expect(document.activeElement, 'the pill is closed: focus on the +').toBe(
    addButton(away),
  )
  expect(tagWrites(away.bff), 'the refused write is not resent').toHaveLength(1)
})

test('15 C1.10: Cancel closes the dialog and puts the focus back on Sign in', async () => {
  const p = await openPage({ instanceTags: SESSION_ENDED })
  fireEvent.click(removeButton(p, 'v2'))
  await within(p.main).findByText(en('session-ended-retry'))
  const sign = tagSignIns(p)[0]!
  fireEvent.click(sign)
  await flushTasks()
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  fireEvent.click(within(d).getByRole('button', { name: en('signin-cancel') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
  expect(document.activeElement, 'focus back on Sign in').toBe(
    tagSignIns(p)[0]!,
  )
  expect(tagAlertTexts(p), 'the alert is kept').toEqual([
    en('session-ended-retry'),
  ])
})

// ── 15 C1.11: the polls never move a chip ───────────────────────────────

test('15 C1.11: a poll answering other tags changes neither the chips nor the open box', async () => {
  let items: unknown[] = [TAG_ITEM]
  const bff = fakeBff({
    instances: itemsAnswer(() => items),
    boot: { read: () => reply(200, { enabled: true }) },
  })
  fakeClock()
  render(<App />)
  await flush()
  const p: Page = {
    bff,
    main: screen.getByRole('main'),
    field: group(screen.getByRole('main')),
  }
  const box = await openPill(p)
  typeIn(box, 'new-tag')
  items = [{ ...TAG_ITEM, instanceTags: ['other'], tags: ['edge', 'other'] }]
  await advance(5_000)
  expect(
    p.bff.to('GET', LIST).length,
    'the page read the list again',
  ).toBeGreaterThanOrEqual(2)
  expect(
    ownChips(p).map(chipText),
    'the chips are the list the page opened with',
  ).toEqual(['v2', 'gpu'])
  expect(box.value, 'the read does not rewrite what was typed').toBe('new-tag')
})

test('15 C1.11: an add while the read after a 204 is still pending is built from the chips, not from the read', async () => {
  // Decision 3: the list sent is the field's own list, latched, with the new
  // tag appended. The page re-reads GET /api/instances after every 204, and
  // the read answers late: the chips are already right, so the next write
  // carries them, and the tag added first is not lost.
  let reads = 0
  const bodies: unknown[] = []
  const p = await openPage({
    instances: () =>
      reads++ === 0
        ? reply(200, [TAG_ITEM])
        : new Promise<Response>(() => {
            /* the re-read the page asked for, never answered */
          }),
    instanceTags: (body) => {
      bodies.push(body)
      return reply(204)
    },
  })
  const first = await openPill(p)
  typeIn(first, 'fast')
  pressEnter(first)
  await waitFor(() => expect(bodies).toHaveLength(1))
  expect(bodies[0], 'the read with fast appended').toEqual({
    tags: ['v2', 'gpu', 'fast'],
  })
  await waitFor(() =>
    expect(ownChips(p).map(chipText), 'fast is a chip').toEqual([
      'v2',
      'gpu',
      'fast',
    ]),
  )
  const second = await openPill(p)
  typeIn(second, 'x')
  pressEnter(second)
  await waitFor(() => expect(bodies).toHaveLength(2))
  expect(bodies[1], 'the chips with x appended: fast is kept').toEqual({
    tags: ['v2', 'gpu', 'fast', 'x'],
  })
})

test('15 C1.11: an add while the read after a removal is still pending does not send the removed tag back', async () => {
  let reads = 0
  const bodies: unknown[] = []
  const p = await openPage({
    instances: () =>
      reads++ === 0
        ? reply(200, [TAG_ITEM])
        : new Promise<Response>(() => {
            /* the re-read the page asked for, never answered */
          }),
    instanceTags: (body) => {
      bodies.push(body)
      return reply(204)
    },
  })
  fireEvent.click(removeButton(p, 'v2'))
  await waitFor(() => expect(bodies).toHaveLength(1))
  expect(bodies[0], 'the read without v2').toEqual({ tags: ['gpu'] })
  await waitFor(() =>
    expect(ownChips(p).map(chipText), 'v2 is gone').toEqual(['gpu']),
  )
  const box = await openPill(p)
  typeIn(box, 'x')
  pressEnter(box)
  await waitFor(() => expect(bodies).toHaveLength(2))
  expect(bodies[1], 'the chips with x appended: v2 stays removed').toEqual({
    tags: ['gpu', 'x'],
  })
})

test('15 C1.11: after a poll answers other tags, an add sends the chips with the new tag, not the read', async () => {
  let items: unknown[] = [TAG_ITEM]
  const bodies: unknown[] = []
  const bff = fakeBff({
    instances: itemsAnswer(() => items),
    boot: { read: () => reply(200, { enabled: true }) },
    instanceTags: (body) => {
      bodies.push(body)
      return reply(204)
    },
  })
  fakeClock()
  render(<App />)
  await flush()
  const p: Page = {
    bff,
    main: screen.getByRole('main'),
    field: group(screen.getByRole('main')),
  }
  const box = await openPill(p)
  typeIn(box, 'x')
  items = [{ ...TAG_ITEM, instanceTags: ['other'], tags: ['edge', 'other'] }]
  await advance(5_000)
  expect(
    p.bff.to('GET', LIST).length,
    'the page read the list again',
  ).toBeGreaterThanOrEqual(2)
  expect(ownChips(p).map(chipText), 'the chips are untouched').toEqual([
    'v2',
    'gpu',
  ])
  pressEnter(box)
  await flush()
  expect(bodies, 'the chips with x appended, not the read of the poll').toEqual(
    [{ tags: ['v2', 'gpu', 'x'] }],
  )
})

// ── 15 C1.12: an unsaved label guards the page ──────────────────────────

test('15 C1.12: a pill holding new-tag guards the page: a nav link and the browser Back open the Unsaved changes dialog, a beforeunload is cancelled, and the text is kept', async () => {
  const p = await openPageFromOverview()
  expect(asksBrowser(), 'no pill: the browser is not asked').toBe(false)
  const box = await openPill(p)
  expect(asksBrowser(), 'an empty pill: the browser is not asked').toBe(false)
  typeIn(box, 'new-tag')
  expect(asksBrowser(), 'a pill holding text: the unload is held').toBe(true)
  fireEvent.click(navLink(en('nav-overview')))
  await flushTasks()
  const d = await discardDialog()
  expect(window.location.hash, 'the address stayed on the instance').toBe(
    '#/instances/default',
  )
  expect(newBox(p).value, 'the text is kept').toBe('new-tag')
  fireEvent.click(within(d).getByRole('button', { name: en('discard-keep') }))
  await flushTasks()
  expect(asksBrowser(), 'still open: the browser is still asked').toBe(true)
  await pressBack()
  const back = await discardDialog()
  fireEvent.click(
    within(back).getByRole('button', { name: en('discard-confirm') }),
  )
  await within(p.main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(asksBrowser(), 'left: nothing left to ask').toBe(false)
})

test('15 C1.12: a closed pill, an empty pill, and any add or removal 204 leave nothing to ask about; the page has no save bar (17a.10)', async () => {
  const p = await openPage()
  expect(
    p.main.querySelector('.save-bar'),
    '17a.10: this page has no save bar (the form is on the profile page)',
  ).toBeNull()
  const box = await openPill(p)
  typeIn(box, 'new-tag')
  typeIn(box, '')
  expect(asksBrowser(), 'an emptied pill: nothing is asked').toBe(false)
  fireEvent.click(pillButton(p, 'instance-tag-cancel'))
  await flushTasks()
  expect(asksBrowser(), 'a closed pill: nothing is asked').toBe(false)

  const add = await openPill(p)
  typeIn(add, 'fast')
  pressEnter(add)
  await waitFor(() =>
    expect(tagStatus(p).textContent).toBe(en('instance-tag-added', 'fast')),
  )
  expect(asksBrowser(), 'saved: nothing is asked').toBe(false)
  expect(
    within(p.main).queryByRole('dialog'),
    'leaving asks nothing',
  ).toBeNull()

  fireEvent.click(removeButton(p, 'v2'))
  await waitFor(() =>
    expect(tagStatus(p).textContent).toBe(en('instance-tag-removed', 'v2')),
  )
  expect(asksBrowser(), 'a removal is saved at once: nothing is asked').toBe(
    false,
  )
  expect(
    p.main.querySelector('.save-bar'),
    'a tag write marks nothing: there is still no save bar',
  ).toBeNull()
  await goLink(en('nav-overview'))
  expect(screen.queryByRole('dialog'), 'no Unsaved changes dialog').toBeNull()
  await within(p.main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(tagWrites(p.bff).length, 'two writes: the add and the removal').toBe(2)
  expect(
    p.bff.to('PUT', SERVER).length,
    'the server profile is never written from here',
  ).toBe(0)
})

// ── 15 C1.13 to C1.15: the expanded pill of a cut chip (15 section 10 R5) ─

test('15 C1.13: with the text of the v2 chip cut, the pointer entering the chip opens a span.chip-full, aria-hidden, holding v2 whole, and leaving hides it; a chip whose text is whole opens nothing', async () => {
  const p = await openPage()
  const [v2, gpu] = ownChips(p)
  measure(v2!, 40, 120)
  measure(gpu!, 120, 120)
  pointerEnter(v2!)
  const pill = fullPill(v2!)
  expect(pill, 'the cut chip opens the pill under the pointer').not.toBeNull()
  expect(
    pill!.getAttribute('aria-hidden'),
    'the list item already reads the full tag, so the pill is not read twice',
  ).toBe('true')
  expect(pill!.textContent, 'the whole tag, not the cut one').toBe('v2')
  expect(fullPill(gpu!), 'a whole tag opens nothing').toBeNull()
  pointerLeave(v2!, p.main)
  expect(
    fullPill(v2!),
    'the pointer left the chip: the pill is gone',
  ).toBeNull()
})

test('15 C1.14: focus on the x of a cut chip opens the pill, and so does focus on a cut profile chip; focus leaving the chip hides it; Escape hides it with the focus where it was, and it stays hidden until a new entry', async () => {
  const p = await openPage()
  const v2 = ownChips(p)[0]!
  measure(v2, 40, 120)

  // Focus entering the chip: the x of an own chip.
  const x = removeButton(p, 'v2')
  x.focus()
  expect(fullPill(v2), 'focus on the x opens the pill').not.toBeNull()

  // Focus leaving the chip hides it.
  const boot = await within(p.main).findByRole('checkbox', {
    name: en('boot-toggle'),
  })
  boot.focus()
  expect(fullPill(v2), 'focus left the chip: the pill is gone').toBeNull()

  // Dismissible: Escape, with the focus untouched, and it stays gone.
  x.focus()
  expect(fullPill(v2), 'the pill is open again').not.toBeNull()
  pressEscapeEverywhere()
  await flushTasks()
  expect(fullPill(v2), 'Escape dismissed the pill').toBeNull()
  expect(document.activeElement, 'the focus did not move').toBe(x)
  await flushTasks()
  expect(fullPill(v2), 'it does not come back by itself').toBeNull()
  pointerEnter(v2)
  expect(fullPill(v2), 'a new pointer entry opens it again').not.toBeNull()
  pointerLeave(v2, p.main)
  x.blur()
  await flushTasks()

  // The keyboard reaches a profile chip's text through the chip itself.
  const edge = profileChips(p)[0]!
  measure(edge, 40, 120)
  edge.focus()
  expect(
    document.activeElement,
    'a profile chip is focusable (R5: tabindex="0"), so the keyboard reaches its text',
  ).toBe(edge)
  expect(
    fullPill(edge),
    'a cut profile chip opens its pill on focus',
  ).not.toBeNull()
  boot.focus()
  expect(fullPill(edge), 'focus leaving it hides the pill').toBeNull()
})

test('15 C1.15: a profile chip carries tabindex="0" and follows Add tag in the tab order, no chip carries a title, and the sheet draws the expanded pill over the chip', async () => {
  const p = await openPage()
  const edge = profileChips(p)[0]!
  expect(
    edge.getAttribute('tabindex'),
    'the profile chip is focusable, its text is what the keyboard reaches',
  ).toBe('0')
  for (const li of ownChips(p))
    expect(
      li.getAttribute('tabindex'),
      'an own chip is reached through its x, so the tab order gains nothing',
    ).toBe(null)
  const stops = tabbable(p.field)
  expect(
    stops.slice(stops.indexOf(addButton(p))),
    'the profile chips follow Add tag (C1.2, as R5 amends it)',
  ).toEqual([addButton(p), edge])

  // R3: no title. The full tag is in the list item's text and the x's name, and
  // sighted users get the pill: a title would repeat R5 for the mouse only.
  for (const li of [...ownChips(p), edge]) {
    expect(li.hasAttribute('title'), 'the chip carries no title').toBe(false)
    expect(li.querySelector('[title]'), 'nor anything inside it').toBeNull()
  }

  // R5: the pill is over the chip, not in the flow, and wide as the chip.
  expect(
    decl(block('.chip'), 'position'),
    'the chip is the box the pill sits on',
  ).toBe('relative')
  // R7: the pill is a span child of `li.chip`, so a bare `.chip-full` (0,1,0)
  // loses to `.chip > span` (0,1,1) and the expanded pill shows the ellipsis
  // of the cut it was made to escape. The rule is the child selector, 0,2,0,
  // with the declarations R5 gave it; `.chip > span` keeps its own rule.
  const pill = block('.chip > .chip-full')
  for (const [name, value] of [
    ['position', 'absolute'],
    ['top', '0'],
    ['inset-inline-start', '0'],
    ['z-index', '1'],
    ['min-height', '100%'],
    ['white-space', 'normal'],
    ['overflow-wrap', 'anywhere'],
  ] as Array<[string, string]>)
    expect(decl(pill, name), `.chip > .chip-full: ${name}`).toBe(value)
  expect(
    decl(pill, 'inset-inline-end'),
    'on an own chip the pill leaves the x uncovered (24px + a space)',
  ).toMatch(/^calc\(\s*24px\s*\+\s*var\(--space-1\)\s*\)$/)
  expect(
    decl(pill, 'background'),
    'the chip fill, so it reads as a chip',
  ).toContain('var(--field)')
  expect(
    decl(pill, 'border-radius'),
    'the chip radius: it can be several lines',
  ).toContain('var(--radius)')
  const profile = TOP_SELECTORS.filter(
    (r) => r.includes('.chips-profile') && r.includes('.chip > .chip-full'),
  )
  expect(
    profile.map((r) => r.trim()),
    'one rule for the pill inside the profile row, and it is the child selector, 0,3,0, so it outranks `.chip > span` there too (R7)',
  ).toEqual(['.chips-profile .chip > .chip-full'])
  expect(
    decl(block(profile[0]!.trim()), 'background'),
    'raised: the surface, not the field',
  ).toContain('var(--surface)')
})

// ── 15 section 10 R3 and R4: the chips in the stylesheet ─────────────────

test('15 section 10 R3: every chip text is a span cut with an ellipsis, the wrap is the 15em last resort, and the pill and its box shrink to the row', () => {
  // R3, first bullet: the text of every chip, own and profile, is in a span
  // with its own block box, because an ellipsis needs one. Exactly these four,
  // no more: a fifth declaration here is a look the brief did not rule.
  expect(
    decls(topBlock('.chip > span')).sort(),
    'the top-level .chip > span rule',
  ).toEqual([
    'min-width: 0',
    'overflow: hidden',
    'text-overflow: ellipsis',
    'white-space: nowrap',
  ])
  const chip = block('.chip')
  expect(
    decl(chip, 'overflow-wrap'),
    "no overflow-wrap on .chip itself: the cut is the span's ellipsis, and the wrap is the last resort of the media query below, on the span",
  ).toBe('')
  expect(
    decl(chip, 'max-width'),
    'the cap is the row: a 32-character tag is about 320 px and fits whole wherever the row is wider',
  ).toBe('100%')
  // R3, last resort: the media query fires only where the user's own text
  // setting makes the viewport under 15 text widths (a media query's em is the
  // browser's text size, not the page's), where an ellipsis would leave a dozen
  // characters. It holds only that rule.
  const medias = atRules().filter((r) => r.header.includes('15em'))
  expect(
    medias.map((r) => r.header),
    'exactly one @media (max-width: 15em) block',
  ).toEqual(['@media (max-width: 15em)'])
  const rules = [...medias[0]!.body.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
  expect(
    rules.map((m) => (m[1] ?? '').trim()),
    'the one rule it holds',
  ).toEqual(['.chip > span'])
  expect(
    // The declarations of that rule, read from its own body: the media block's
    // body is a rule, not a list of declarations.
    decls(rules[0]![2] ?? '').sort(),
    'and its declarations: wrapping, on the span',
  ).toEqual(['overflow-wrap: anywhere', 'white-space: normal'])
  // R3, the pill (defect 2): it and its box are flex children that shrink.
  const pill = block('.tag-new')
  expect(decl(pill, 'min-width'), 'the pill shrinks to the row').toBe('0')
  expect(decl(pill, 'max-width'), 'and is never wider than it').toBe('100%')
  const box = block('.tag-new input')
  expect(decl(box, 'min-width'), 'the box shrinks below its 16 ch').toBe('0')
  expect(
    // `flex: 0 1 16ch` as R3 writes it, or the longhand of the same.
    decl(box, 'flex').split(/\s+/)[1] || decl(box, 'flex-shrink'),
    'the middle of the flex shorthand is the shrink factor: 1, so a long tag in the box does not push the row sideways (15 px at 320 px, 152 px at 200 % text with the pill open)',
  ).toBe('1')
})

test('15 section 10 R4: the profile line, its chips and the input error each take a row of their own under the chip row', () => {
  // The group is `.field-control`, a wrapping flex row, so an item that takes
  // the rest of the line sits beside the chips at 1280 px: 100% is what starts
  // the row under them, the way `.boot-status` and `.boot-alert` do.
  for (const sel of [
    '.tags-profile',
    '.chips-profile',
    '.field-control .field-error',
  ])
    expect(
      decl(block(sel), 'flex-basis'),
      `${sel}: a row of its own under the chips`,
    ).toBe('100%')
})
