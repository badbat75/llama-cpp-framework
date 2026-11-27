// Brief 22, the 22D4 amendment (fe/briefs/22-model-sources.md, "22D4 amendment:
// the list's reason floats", 22 D4.16 to D4.22): the reason a list row's delete
// shows on hover and on focus is a small box floating over the table, not a line
// in the cell's flow, and the row's delete is a trash can in a round outlined
// button. This file is the amendment, read through both lists, whose delete is
// the shared RowDelete: the Model profiles list and the Server profiles list.
// The behaviour the amendment leaves alone (22 D4.3, D4.13, D4.18) is
// ModelProfilesDelete1.test.tsx and ServerProfilesDelete1.test.tsx, amended for
// the floating box, the trash can and the touch answers.
//
// D4.16's geometry (the −'s row height, every column's width, the −'s own rect,
// the scroll box's scrollWidth and scrollHeight, the document's sideways scroll,
// each before and after the tip shows) and D4.21's 40 px round button are
// measurements: jsdom lays nothing out, so the designer measures them in the
// three browsers. What jsdom pins is the structure (the tip is the −'s next
// sibling inside the .hint-box), the sheet (position: fixed, a numeric z-index,
// the shadow, the round shape, the two hovers), and the script's placement, read
// off the tip's inline style with a stubbed getBoundingClientRect and window
// size (22 D4.17). The helpers and the stylesheet reader are local copies: a
// test file imports no other test file.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { createElement, type ReactElement } from 'react'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  accName,
  type BffOptions,
  en,
  fakeBff,
  isHidden,
  type ModelProfile,
  type ServerProfile,
  startAt,
} from './test/bff.ts'

/** `trash-2` of lucide-static 1.74.0 (icons/trash-2.svg, ISC), the five paths in
 *  the order the package writes them (22 D4.21): the two strokes inside the can,
 *  the can, the lid's line, the handle. */
const TRASH = [
  'M10 11v6',
  'M14 11v6',
  'M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6',
  'M3 6h18',
  'M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2',
]
/** `plus`, which the lists' add keeps (22 D4.21). */
const PLUS = ['M5 12h14', 'M12 5v14']

/** The eight attributes of brief 08 section 0 every icon carries (the copy of
 *  U17c.1: the shape is data, and a copy of it here reds the case that compares
 *  it against the component). */
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

/** `Icon.tsx` through a plain `string` prop: the module's own prop names the
 *  `IconName` union, so the cast is the tester's. The path stays in a variable,
 *  so the bundler cannot analyse it at transform time. */
type IconModule = { Icon: (props: { name: string }) => ReactElement }
const ICON_MODULE = './Icon.tsx'
const iconModule = async (): Promise<IconModule> =>
  (await import(ICON_MODULE)) as unknown as IconModule

// ── The two lists ───────────────────────────────────────────────────────

function modelProfile(id: string, enabled: boolean): ModelProfile {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model: '' }
}

/** Two on, one off: only the last row's delete can act. */
const MODEL_LIST: ModelProfile[] = [
  modelProfile('Qwen3-27B', true),
  modelProfile('Coder-30B', true),
  modelProfile('Old-Coder', false),
]

function serverRow(id: string, instance: string | null): ServerProfile {
  return { id, engine: 'llama.cpp', port: 8580, tags: [], instance }
}

/** One profile an instance runs, one free: only the free row can act. */
const SERVER_ROWS: ServerProfile[] = [
  serverRow('default', 'default'),
  serverRow('qwen-27b', null),
]

/** One of the two lists, whose delete is the shared RowDelete. */
type List = {
  /** What the messages call it. */
  label: string
  /** The list page, with its table read. */
  open: () => Promise<HTMLElement>
  /** The row delete's name, with the row's id (22 D4.1). */
  deleteKey: string
  /** The reason a row's delete cannot act (22 D4.2). */
  reasonKey: string
  /** A row whose delete cannot act, and one whose can. */
  blocked: string
  free: string
}

const MODELS: List = {
  label: 'the Models list',
  open: async () => {
    startAt('#/profiles/model')
    fakeBff({ modelProfiles: MODEL_LIST })
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('table', { name: en('model-profiles-title') })
    await flushTasks()
    return main
  },
  deleteKey: 'model-profiles-delete-row',
  reasonKey: 'model-profiles-delete-needs-off',
  blocked: 'Qwen3-27B',
  free: 'Old-Coder',
}

const SERVERS: List = {
  label: 'the Server profiles list',
  open: async () => {
    startAt('#/profiles/server')
    fakeBff({ serverProfiles: SERVER_ROWS })
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('table', {
      name: en('server-profiles-title'),
    })
    await flushTasks()
    return main
  },
  deleteKey: 'server-profiles-delete-row',
  reasonKey: 'server-profiles-delete-in-use',
  blocked: 'default',
  free: 'qwen-27b',
}

const LISTS = [MODELS, SERVERS]

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a promise chain) run. */
async function flushTasks(n = 3) {
  await Promise.all(
    Array.from({ length: n }, () => new Promise((r) => setTimeout(r, 0))),
  )
}

/** The words a node shows, whitespace folded. */
function text(el: Element | null): string {
  return (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function tableOf(main: HTMLElement, list: List): HTMLElement {
  const key = list === MODELS ? 'model-profiles-title' : 'server-profiles-title'
  return within(main).getByRole('table', { name: en(key) })
}

/** The row's delete, named with the row's id (22 D4.1). */
function deleteOf(main: HTMLElement, list: List, id: string): HTMLElement {
  return within(tableOf(main, list)).getByRole('button', {
    name: en(list.deleteKey, id),
  })
}

/** The box that holds a row's delete and its tip (22 D4.13). */
function boxOf(button: HTMLElement): HTMLElement {
  const box = button.closest('.hint-box')
  expect(box, 'the delete sits in a hint box').not.toBeNull()
  return box as HTMLElement
}

/** The tip, shown by a hover of the box (the box owns the handlers, D4.13). */
async function tipShown(main: HTMLElement, list: List): Promise<HTMLElement> {
  const minus = deleteOf(main, list, list.blocked)
  fireEvent.mouseOver(minus)
  await flushTasks()
  const tip = boxOf(minus).querySelector<HTMLElement>('.hint-tip')
  expect(tip, `${list.label}: the hover shows the tip`).not.toBeNull()
  return tip!
}

// ── The stylesheet reader (the local copy of 22 D4.1) ───────────────────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = { compact: string; body: string; at: number; depth: number }

const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  compact: m[1].replace(/\s+/g, ''),
  body: m[2],
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

const TOP = RULES.filter((r) => r.depth === 0)

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

/** Every declaration of every top-level rule whose selector list names `sel`
 *  exactly, in the sheet's order. */
function declsOf(sel: string): string[] {
  return TOP.filter((r) => r.compact.split(',').includes(sel)).flatMap((r) =>
    r.body.split(';').map((d) => norm(d)),
  )
}

/** The declarations of a property, in the sheet's order. */
function values(decls: string[], prop: string): string[] {
  return decls
    .filter((d) => d.startsWith(`${prop}:`))
    .map((d) => norm(d.slice(prop.length + 1)))
}

/** The one rule whose whole selector is `sel`. */
function ruleOf(sel: string): Rule | undefined {
  return RULES.find((r) => r.compact === sel)
}

/** One declaration of a rule's body. */
function propOf(body: string, prop: string): string | undefined {
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && norm(d.slice(0, i)) === prop) return norm(d.slice(i + 1))
  }
  return undefined
}

/** The declarations that reach `el` from the top-level rules, merged in the
 *  cascade's order (the local copy of 22 D2.2's reader). */
function cascade(el: HTMLElement): Record<string, string> {
  const own = new Set([el.tagName.toLowerCase()])
  for (const c of el.classList) own.add(`.${c}`)
  if (el.id) own.add(`#${el.id}`)
  const hits: { rule: Rule; spec: number }[] = []
  TOP.forEach((rule) => {
    let spec = -1
    for (const part of rule.compact.split(',')) {
      const tokens = part.match(/[#.]?[-a-z0-9_]+/g) ?? []
      if (tokens.length > 0 && tokens.every((t) => own.has(t)))
        spec = Math.max(spec, tokens.filter((t) => t.startsWith('.')).length)
    }
    if (spec >= 0) hits.push({ rule, spec })
  })
  hits.sort((a, b) => a.spec - b.spec || a.rule.at - b.rule.at)
  const decls: Record<string, string> = {}
  for (const h of hits)
    for (const d of h.rule.body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) decls[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
  return decls
}

// ── The viewport and the boxes, stubbed (22 D4.17) ──────────────────────

type Rect = { top: number; bottom: number; left: number; right: number }

/** The viewport, as a browser reports it: jsdom's is fixed and read-only. */
function viewport(width: number, height: number) {
  vi.spyOn(window, 'innerWidth', 'get').mockReturnValue(width)
  vi.spyOn(window, 'innerHeight', 'get').mockReturnValue(height)
}

/** The box `el` occupies: jsdom lays nothing out, so every rect is a stub. */
function rect(el: HTMLElement, r: Rect) {
  el.getBoundingClientRect = () =>
    ({
      ...r,
      x: r.left,
      y: r.top,
      width: r.right - r.left,
      height: r.bottom - r.top,
      toJSON: () => ({}),
    }) as DOMRect
}

/** The tip's own size, read as a rect and as the offsets: the brief names
 *  "the tip's own size", not one way of reading it. */
function tipSize(el: HTMLElement, width: number, height: number) {
  rect(el, { top: 0, bottom: height, left: 0, right: width })
  Object.defineProperty(el, 'offsetWidth', { value: width, configurable: true })
  Object.defineProperty(el, 'offsetHeight', {
    value: height,
    configurable: true,
  })
}

/** Where the script put the tip, in px, off its inline style. */
function placed(tip: HTMLElement): { top: number; left: number } {
  return {
    top: Number.parseFloat(tip.style.top),
    left: Number.parseFloat(tip.style.left),
  }
}

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  startAt('')
})

// ── 22 D4.16: out of the flow ───────────────────────────────────────────

test('22 D4.16: the tip is out of the flow: the sheet gives .hint-tip position: fixed and a numeric z-index of 2 or more (above the sticky Save bar and the table head, both z-index 1), and on both lists the tip is the −\u2019s next sibling inside the span.hint-box of the cell, so showing it takes no room', () => {
  const decls = declsOf('.hint-tip')
  expect(decls.length, 'the tip has rules in the sheet').toBeGreaterThan(0)
  expect(
    values(decls, 'position'),
    '22 D4.16: the tip is placed against the viewport, so the table\u2019s scroll box neither clips it nor counts it, and it adds no scroll',
  ).toEqual(['fixed'])
  const z = values(decls, 'z-index')
  expect(z.length, '22 D4.16: the tip names the layer it floats on').toBe(1)
  expect(
    Number.isFinite(Number(z[0])),
    `22 D4.16: the z-index is a number, and the sheet says ${z[0] ?? 'nothing'}`,
  ).toBe(true)
  expect(
    Number(z[0]),
    '22 D4.16: above the sticky Save bar and the table head, both z-index 1',
  ).toBeGreaterThanOrEqual(2)
})

test('22 D4.16: on both lists the tip is the −\u2019s next sibling inside the span.hint-box of the cell, the − first, and the rule that reaches the shown tip places it against the viewport', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const minus = deleteOf(main, list, list.blocked)
    const box = boxOf(minus)
    expect(box.tagName, `${list.label}: the box is a span`).toBe('SPAN')
    expect(box.parentElement?.tagName, 'in the cell').toBe('TD')
    expect(box.firstElementChild, 'the − comes first').toBe(minus)
    const tip = await tipShown(main, list)
    expect(
      tip.parentElement,
      `${list.label}: the tip is a child of the box`,
    ).toBe(box)
    expect(
      tip.previousElementSibling,
      '22 D4.16: the tip is the −\u2019s next sibling',
    ).toBe(minus)
    expect(
      norm(cascade(tip)['position'] ?? ''),
      `${list.label}: the rule that reaches the shown tip says fixed, so it takes no room in the cell`,
    ).toBe('fixed')
    cleanup()
  }
})

// ── 22 D4.17: where it opens and how it flips ────────────────────────────

test('22 D4.17: the script places the tip inline from the −\u2019s rect, the viewport and the tip\u2019s own size with an 8 px gutter: under the − touching it (top = r.bottom, left = r.left), its right edge at the −\u2019s right edge when it would pass the viewport, above the − touching it when it would pass the bottom, and a scroll of the table\u2019s box or a resize places it again', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const tip = await tipShown(main, list)
    const minus = deleteOf(main, list, list.blocked)
    viewport(1280, 720)
    tipSize(tip, 200, 40)
    // Under the −, touching it: the pointer crosses nothing (1.4.13).
    rect(minus, { top: 100, bottom: 124, left: 40, right: 64 })
    fireEvent.scroll(window)
    expect(
      placed(tip),
      `${list.label}: under the −, touching it: top = r.bottom, left = r.left`,
    ).toEqual({ top: 124, left: 40 })
    // Near the right edge: the right edges align, the gutter kept.
    rect(minus, { top: 100, bottom: 124, left: 1150, right: 1174 })
    fireEvent.scroll(window)
    expect(
      placed(tip),
      `${list.label}: near the right edge, left = r.right - tipWidth`,
    ).toEqual({ top: 124, left: 974 })
    // Near the bottom: it opens above, touching the −.
    rect(minus, { top: 690, bottom: 714, left: 40, right: 64 })
    fireEvent.scroll(window)
    expect(
      placed(tip),
      `${list.label}: near the bottom, top = r.top - tipHeight`,
    ).toEqual({ top: 650, left: 40 })
    // A scroll anywhere, the table\u2019s own box included (a capturing listener
    // on window), places it again: it never covers the − (2.4.11).
    rect(minus, { top: 300, bottom: 324, left: 200, right: 224 })
    const box = main.querySelector('.table-scroll')
    expect(box, `${list.label}: the table has its scroll box`).not.toBeNull()
    fireEvent.scroll(box!)
    expect(
      placed(tip),
      `${list.label}: a scroll of the table\u2019s box places it again`,
    ).toEqual({ top: 324, left: 200 })
    // A resize places it again.
    rect(minus, { top: 400, bottom: 424, left: 300, right: 324 })
    fireEvent(window, new Event('resize'))
    expect(placed(tip), `${list.label}: a resize places it again`).toEqual({
      top: 424,
      left: 300,
    })
    cleanup()
  }
})

// ── 22 D4.17 D1, D2: the fix measured (brief, "D1 fix measured") ─────────

test('22 D4.17 D1: the tip is measured at the origin: place() sets its inline top and left to 0px before reading offsetWidth and offsetHeight, at the first placement and on a re-placement after a scroll (measured: at the right edge the tip wraps narrower at its old place, so a size read there decides the flip from a stale box)', async () => {
  const reads: { phase: string; top: string; left: string }[] = []
  let phase = 'first'
  const record = (size: number) =>
    function (this: HTMLElement) {
      if (this.classList.contains('hint-tip'))
        reads.push({ phase, top: this.style.top, left: this.style.left })
      return size
    }
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockImplementation(
    record(200),
  )
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(
    record(40),
  )
  const main = await MODELS.open()
  const minus = deleteOf(main, MODELS, MODELS.blocked)
  await tipShown(main, MODELS)
  viewport(1280, 720)
  rect(minus, { top: 100, bottom: 124, left: 40, right: 64 })
  phase = 'scroll'
  fireEvent.scroll(window)
  expect(
    reads.filter((r) => r.phase === 'first').length,
    'the first placement measures the tip',
  ).toBeGreaterThan(0)
  expect(
    reads.filter((r) => r.phase === 'scroll').length,
    '22 D4.17 D1: a re-placement after a scroll measures it again',
  ).toBeGreaterThan(0)
  for (const r of reads)
    expect(
      [r.top, r.left],
      `22 D4.17 D1: the size is read with top and left at 0px (${r.phase} placement)`,
    ).toEqual(['0px', '0px'])
})

test('22 D4.17 D2: after a flip above, top is clamped to the 8 px gutter: a tip taller than the room above the − lands at top = 8, inside the viewport, not at a negative top above it', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const tip = await tipShown(main, list)
    const minus = deleteOf(main, list, list.blocked)
    viewport(1280, 720)
    tipSize(tip, 200, 700)
    rect(minus, { top: 690, bottom: 714, left: 40, right: 64 })
    fireEvent.scroll(window)
    expect(
      placed(tip),
      `${list.label}: 22 D4.17 D2: the flip above is clamped to the gutter, so the tip stays in the viewport (2.4.11)`,
    ).toEqual({ top: 8, left: 40 })
    cleanup()
  }
})

// ── 22 D4.18 D3: Escape reaches the tip from anywhere (same review) ──────

test('22 D4.18 D3: Escape hides a hover-shown tip with focus elsewhere: while the tip shows it listens on the document, so Escape reaches it wherever the focus is, and the focus does not move', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    await tipShown(main, list)
    const other = deleteOf(main, list, list.free)
    other.focus()
    expect(
      document.activeElement,
      `${list.label}: the focus is on the other row’s −, not on the − that shows the tip`,
    ).toBe(other)
    fireEvent.keyDown(document, { key: 'Escape' })
    await flushTasks()
    expect(
      boxOf(deleteOf(main, list, list.blocked)).querySelector('.hint-tip'),
      `${list.label}: 22 D4.18 D3: Escape on the document hides the hover-shown tip`,
    ).toBeNull()
    expect(
      document.activeElement,
      `${list.label}: and the focus stays where it was, Escape sends the user nowhere`,
    ).toBe(other)
    cleanup()
  }
})

// ── 22 D4.19: the look ──────────────────────────────────────────────────

test('22 D4.19: the tip keeps D4.13\u2019s look and adds a shadow: the .hint-tip rules give a --surface fill, a --border-w solid --control edge, --radius-check, --text, box-shadow: var(--shadow), max-width: min(16rem, 100vw - 16px), white-space: normal and no font-size, and no rule that reaches the tip turns forced colours off', () => {
  const decls = declsOf('.hint-tip')
  expect(decls.length, 'the tip has rules in the sheet').toBeGreaterThan(0)
  expect(values(decls, 'background'), 'the --surface fill').toEqual([
    'var(--surface)',
  ])
  expect(
    values(decls, 'border'),
    'the --control edge, the cue that survives forced colours',
  ).toEqual(['var(--border-w) solid var(--control)'])
  expect(values(decls, 'border-radius'), 'the check radius').toEqual([
    'var(--radius-check)',
  ])
  expect(values(decls, 'color'), '--text on --surface (1.4.3)').toEqual([
    'var(--text)',
  ])
  expect(
    values(decls, 'box-shadow'),
    '22 D4.19: the shadow that reads as above the rows it covers',
  ).toEqual(['var(--shadow)'])
  expect(
    values(decls, 'max-width').map((v) => v.replace(/\s+/g, '')),
    '22 D4.17: at most 16rem, and never past the gutters at 320 px (1.4.10)',
  ).toEqual(['min(16rem,100vw-16px)'])
  expect(
    values(decls, 'white-space'),
    '22 D5: it wraps, undoing the nowrap of the cell',
  ).toEqual(['normal'])
  expect(
    values(decls, 'font-size'),
    'the body text size: the tip declares none (1.4.4)',
  ).toEqual([])
  const forced = RULES.filter((r) =>
    r.compact.split(',').includes('.hint-tip'),
  ).flatMap((r) => r.body.split(';').map((d) => norm(d)))
  expect(
    forced.filter((d) => d.startsWith('forced-color-adjust:')),
    '22 D4.19: forced-color-adjust stays auto, so the border comes back as CanvasText',
  ).toEqual([])
})

// ── 22 D4.20: screen readers, unchanged ─────────────────────────────────

test('22 D4.20: on both lists the tip stays hidden from readers (aria-hidden, no role, no aria-live, and nothing around it is a live region), and the −\u2019s aria-describedby names the hidden ${id}-reason span that carries the reason, so it is read once with the button', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    const minus = deleteOf(main, list, list.blocked)
    const named = minus.getAttribute('aria-describedby')
    expect(named, `${list.label}: 22 D4.2: the − names its reason`).toBeTruthy()
    const said = document.getElementById(named!)
    expect(said, 'an element the id names').not.toBeNull()
    expect(said!.id, 'named by the −\u2019s id with -reason').toBe(
      `${minus.id}-reason`,
    )
    expect(text(said), 'the words of the brief').toBe(en(list.reasonKey))
    expect(isHidden(said), 'kept out of sight').toBe(true)
    const tip = await tipShown(main, list)
    expect(
      tip.getAttribute('aria-hidden'),
      `${list.label}: 22 D4.20: the tip is the eye\u2019s copy`,
    ).toBe('true')
    expect(tip.getAttribute('role'), 'no role').toBeNull()
    expect(tip.getAttribute('aria-live'), 'no live region').toBeNull()
    const live: string[] = []
    for (let n: Element | null = tip.parentElement; n; n = n.parentElement) {
      const role = n.getAttribute('role') ?? ''
      if (
        n.getAttribute('aria-live') ||
        ['status', 'alert', 'log'].includes(role)
      )
        live.push(`${n.tagName}${role ? `[role=${role}]` : ''}`)
    }
    expect(
      live,
      `${list.label}: nothing the tip sits in is announced (the reason is read once, with the button)`,
    ).toEqual([])
    cleanup()
  }
})

// ── 22 D4.21: the icons ────────────────────────────────────────────────

test('22 D4.21: Icon.tsx draws trash-2 with lucide-static 1.74.0\u2019s five paths in order, decorative as every other icon, and minus is gone from the list (nothing uses it once the rows\u2019 − is a trash can)', async () => {
  const { Icon } = await iconModule()
  const { container } = render(createElement(Icon, { name: 'trash-2' }))
  const svg = container.querySelector('svg')
  expect(svg, '22 D4.21: trash-2 joins the list').not.toBeNull()
  expect(svg!.getAttribute('class'), 'the class the sheet sizes').toBe('icon')
  for (const [key, value] of Object.entries(ICON_ATTRS))
    expect(svg!.getAttribute(key), `the svg's ${key}`).toBe(value)
  expect(
    [...svg!.querySelectorAll('path')].map((p) => p.getAttribute('d')),
    '22 D4.21: the path data of the package, in order',
  ).toEqual(TRASH)
  expect(text(svg!), 'an icon holds no text').toBe('')
  const gone = render(createElement(Icon, { name: 'minus' }))
  expect(
    gone.container.querySelector('svg')!.querySelectorAll('path').length,
    '22 D4.21: minus leaves the list: it draws no shape',
  ).toBe(0)
})

test('22 D4.21: the row delete of both lists is the trash can in a button carrying ot-add and ot-delete; the Server profiles list\u2019s + keeps the plus and carries no ot-delete; the Save bar\u2019s Delete on both profile pages stays a text button with no icon', async () => {
  for (const list of LISTS) {
    const main = await list.open()
    for (const id of [list.blocked, list.free]) {
      const button = deleteOf(main, list, id)
      expect(
        [...button.querySelectorAll('svg path')].map((p) =>
          p.getAttribute('d'),
        ),
        `${list.label} ${id}: 22 D4.21: the trash can, in order`,
      ).toEqual(TRASH)
      expect(
        button.classList.contains('ot-add'),
        'the shared icon button',
      ).toBe(true)
      expect(
        button.classList.contains('ot-delete'),
        `${list.label} ${id}: 22 D4.21: a delete carries its own class beside it`,
      ).toBe(true)
    }
    cleanup()
  }
  const main = await SERVERS.open()
  const plus = within(main).getByRole('button', {
    name: en('server-profiles-new'),
  })
  expect(
    [...plus.querySelectorAll('svg path')].map((p) => p.getAttribute('d')),
    '22 D4.21: the list\u2019s + keeps the plus',
  ).toEqual(PLUS)
  expect(plus.classList.contains('ot-add'), 'the shared icon button').toBe(true)
  expect(
    plus.classList.contains('ot-delete'),
    'and no delete class: the + takes no --danger hover',
  ).toBe(false)
  cleanup()
  for (const [route, key, options] of [
    [
      '#/profiles/model/Qwen3-27B',
      'model-profile-delete',
      { modelProfiles: MODEL_LIST },
    ],
    [
      '#/profiles/server/default',
      'server-profile-delete',
      { serverProfiles: SERVER_ROWS },
    ],
  ] as [string, string, BffOptions][]) {
    startAt(route)
    fakeBff(options)
    render(<App />)
    const main = screen.getByRole('main')
    const verb = await within(main).findByRole('button', { name: en(key) })
    expect(
      verb.querySelector('svg'),
      `22 D4.21: the Save bar\u2019s Delete on ${route} stays a text button`,
    ).toBeNull()
    expect(accName(verb), 'its word says it').toBe(en(key))
    cleanup()
  }
})

test('22 D4.21: the sheet makes every .ot-add a 40 px round outlined button (border-radius: var(--radius-pill)) and gives the hover, only while the button can act, a --surface fill on every .ot-add and --danger for the icon and the edge of .ot-add.ot-delete', () => {
  const add = declsOf('.ot-add')
  expect(
    add.length,
    'the shared icon button has rules in the sheet',
  ).toBeGreaterThan(0)
  expect(values(add, 'width'), 'the 40 px box').toEqual(['40px'])
  expect(values(add, 'min-height'), 'and never shorter (2.5.8)').toEqual([
    '40px',
  ])
  expect(
    values(add, 'border-radius'),
    '22 D4.21: round: the pill replaces the button default\u2019s --radius',
  ).toEqual(['var(--radius-pill)'])
  expect(values(add, 'border'), 'the --control edge').toEqual([
    'var(--border-w) solid var(--control)',
  ])
  expect(values(add, 'background'), 'on --bg').toEqual(['var(--bg)'])
  expect(values(add, 'color'), 'the icon is --text').toEqual(['var(--text)'])
  const hover = ruleOf('.ot-add:not(.inactive):hover')
  expect(
    hover,
    '22 D4.21: the hover of a button that can act, one rule for every .ot-add',
  ).not.toBeUndefined()
  expect(propOf(hover!.body, 'background'), 'a --surface fill').toBe(
    'var(--surface)',
  )
  const del = ruleOf('.ot-add.ot-delete:not(.inactive):hover')
  expect(
    del,
    '22 D4.21: the delete\u2019s own hover, beside the shared one',
  ).not.toBeUndefined()
  expect(propOf(del!.body, 'color'), 'the trash is --danger on hover').toBe(
    'var(--danger)',
  )
  expect(propOf(del!.body, 'border-color'), 'and its edge').toBe(
    'var(--danger)',
  )
  expect(
    values(declsOf('.ot-add.inactive'), 'background'),
    '22 D4.21: an inactive button keeps the add\u2019s fill: the hover is the active one\u2019s',
  ).toEqual([])
})

// ── 22 D4.22: the refusal of a switched-on delete ────────────────────────

test('22 D4.22: en_US.json carries the switched-on refusal the brief gives, the same words on the list and on the page (the page has its own switch since brief 24 A2, so "in the list" is wrong there: 3.3.3)', () => {
  const table: Record<string, string> = strings
  expect(
    table['api-model-profile-enabled'],
    '22 D4.22: the English of the brief',
  ).toBe('{0} is switched on, so it cannot be deleted. Switch it off first.')
})
