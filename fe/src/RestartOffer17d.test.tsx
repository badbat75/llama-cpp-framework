// The restart offer and the instance's server profile switch (fe/briefs/16-server-profiles-and-instances.md,
// slice 17d, as its "17d check (2026-11-26)" amends it: 17d.1a, 17d.2a, 17d.3a,
// 17d.4a, 17d.5b, 17d.6a, 17d.7a and 17d.9a replace 17d.1, 17d.2, 17d.3, 17d.4,
// 17d.5, 17d.6, 17d.7 and 17d.9, and win). The offer is the row of secondary
// Restart buttons a save or a switch leaves where the change needs a restart to
// apply: inside the sticky Save bar on the model and server profile pages, above
// the table on the Models list, and nowhere on the instance page, whose head
// carries its own Restart (17d.6a, 17e.7a). The instance page's Server profile
// section switches the instance's server profile with a select and a button
// (PUT /api/instances/{id}/profiles/server). "17d.6" is the brief's criterion 6.
// The whole app renders against the fake bff (mocked fetch).
//
// The names the offer fills its texts with are the display names the app shows
// (22 D4.37, built by `instanceName` of test/bff.ts), read from the app's own
// instance list; an id the list lacks shows as the id (17d.1a).
//
// What jsdom cannot see is left to the designer's `fe/ux` run of the
// `instance-server-switch` and `model-profiles-restart-offer` states: the offer's
// buttons wrapping one per line at 320 px and 200 % text with no sideways scroll
// (17d.10), and the switch's long option cut after the id (17c.11a).
//
// The 17d review's defects are pinned by the tests ending with its label: D2
// (the focus does not come back to the offer's button after its Sign in's
// dialog) by one test per place the offer lives, each signing in THROUGH the
// dialog; D1 (the offer's buttons are not laid out as the run row's: the icon
// touches the words) by the cascade test of the Models list section, read off
// the sheet; the Models list carry-over (the offer's own status line and alert
// surviving the next switch's answer) by the two tests of that section; and
// the review's adviser note 1, as its re-check's D3 amends it (the switch's
// button sits in its select's `.field-control` after the select, at its own
// width, with the select's hint after the button) by the layout test of the
// instance page section.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  instanceName,
  isHidden,
  modelProfileDoc,
  reply,
  serverProfileDoc,
  startAt,
  stubDialog,
  type BffOptions,
  type Instance,
  type ServerProfile,
} from './test/bff.ts'
import { serverProfileHref } from './route.ts'

/** The `rotate-cw` icon of the run row's Restart (Icon.tsx): its second
 *  stroke, the one no other icon carries. */
const ROTATE_CW = 'M21 3v5h-5'

/** The instances of every test: one with a label, one with a port (the auto
 *  name), one stopped with neither. */
const INSTANCES: Instance[] = [
  {
    id: 'default',
    engine: 'llama.cpp',
    state: 'active',
    label: 'Coding assistant',
  },
  { id: 'coder', engine: 'llama.cpp', state: 'inactive', port: 8580 },
  { id: 'embed', engine: 'llama.cpp', state: 'inactive' },
]

/** The name the app shows for one of them (22 D4.37). */
const NAME = {
  default: instanceName(INSTANCES[0]!),
  coder: instanceName(INSTANCES[1]!),
  embed: instanceName(INSTANCES[2]!),
}

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers every group shares ──────────────────────────────────────────

/** An element's text, with the page's runs of whitespace folded. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function inactive(el: HTMLElement): boolean {
  return (
    (el as HTMLButtonElement).disabled ||
    el.getAttribute('aria-disabled') === 'true'
  )
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The texts of the elements a control's aria-describedby names. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** The offer's buttons on screen, in DOM order: every button whose name is
 *  `restart-offer-button` filled with a name. */
function offerButtons(root: HTMLElement): HTMLButtonElement[] {
  return within(root).queryAllByRole('button', {
    name: /^Restart /,
  }) as HTMLButtonElement[]
}

/** The `role="status"` line holding `text`. */
function statusOf(root: HTMLElement, text: string): HTMLElement {
  const el = within(root).getByText(text)
  const line = el.closest('[role="status"]')
  expect(line, `${text}: in a role="status" line`).not.toBeNull()
  return line as HTMLElement
}

/** The alert place holds `text` (exact), as a role="alert" of its own. */
function alertOf(root: HTMLElement, text: string): HTMLElement {
  const el = within(root).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${text}: in a role="alert"`).not.toBe(null)
  return alert as HTMLElement
}

/** The offer's button for one instance, by the name it shows. */
function offerButton(root: HTMLElement, name: string): HTMLButtonElement {
  return within(root).getByRole('button', {
    name: en('restart-offer-button', name),
  }) as HTMLButtonElement
}

/** Signs in through the dialog a Sign in opened, as the user does: the
 *  username, the password, Sign in. The fake answers the same user, so the
 *  page stays (App's `renewed`), and the dialog closes. */
async function signInInDialog(user = 'alice') {
  const d = await screen.findByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), user)
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await waitFor(() =>
    expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull(),
  )
}

/** Every button of the offer carries the run row's `rotate-cw` icon (17d.1a). */
function expectRotateIcons(buttons: HTMLButtonElement[]) {
  for (const b of buttons)
    expect(
      b.querySelector(`svg path[d="${ROTATE_CW}"]`),
      `${text(b)}: the rotate-cw icon`,
    ).not.toBeNull()
}

// ── The sheet, read for the cascade (17d review D1) ──────────────────────
//
// jsdom applies no CSS, so the offer's button rules are read off the sheet the
// way `Instances17c.test.tsx` reads the run row's: comments dropped, the rules
// from the brace balance, the nesting from the balance before the selector, the
// specificity from the part's compounds. The reader is a local copy, because a
// test file imports no other test file.

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  body: string
  depth: number
  at: number
}

const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const ALL: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim(),
  body: m[2],
  depth: opened(m.index ?? 0),
  at: m.index ?? 0,
}))

// Top-level rules only: the run row's rules are top-level, and the order of the
// rules inside an @media block says nothing about the cascade of the top ones.

/** A rule's selector parts, whitespace normalised. */
function partsOf(sel: string): string[] {
  return sel
    .split(',')
    .map((p) => p.replace(/\s+/g, ' ').trim())
    .filter((p) => p.length > 0)
}

/**
 * A selector part's compounds, each reduced to its tag and its classes:
 * pseudo-classes and attribute brackets are dropped, so `button:focus` reads as
 * `button`. A compound that is neither (`*`, `[hidden]`) answers `[]`, and
 * `scopedBy` leaves such a part out.
 */
function compounds(part: string): string[][] {
  return part
    .split(/[\s>+~]+/)
    .filter((c) => c.length > 0)
    .map((c) => {
      const bare = c.split('(')[0]!.split(':')[0]!
      const out: string[] = []
      const tag = bare.match(/^[a-zA-Z][a-zA-Z0-9-]*/)
      if (tag) out.push(tag[0].toLowerCase())
      for (const m of bare.matchAll(/\.[a-zA-Z0-9_-]+/g))
        out.push(m[0].toLowerCase())
      return out
    })
}

/** A compound's tokens (its tag and its classes) against `el`: every token the
 *  element carries. */
function matchesCompound(el: Element, toks: string[]): boolean {
  const own = new Set([
    el.tagName.toLowerCase(),
    ...[...el.classList].map((c) => `.${c.toLowerCase()}`),
  ])
  return toks.every((t) => own.has(t))
}

/**
 * The rules of `rules` that style `el`, sibling-aware: the compound before a `+`
 * is one of the element's PREVIOUS siblings, before a `>` its parent, before a
 * space one of its ancestors. `scopedBy` reads every earlier compound as an
 * ancestor, so it misses `.field-control select + button`, the rule the 17d
 * review's D3 takes away, and a pin of that rule's absence through it would
 * pass with nothing to check. A part naming `:not(...)` is left out, as
 * `compounds` reads it as nothing.
 */
function rulesReaching(
  el: HTMLElement,
  rules: Rule[],
): Array<{ rule: Rule; part: string }> {
  const out: Array<{ rule: Rule; part: string }> = []
  for (const rule of rules)
    for (const part of partsOf(rule.sel)) {
      const toks = [...part.matchAll(/([>+~])?\s*([^ >+~]+)/g)].map((m) => ({
        comb: m[1] ?? ' ',
        c: compounds(m[2]!)[0] ?? [],
      }))
      if (toks.length === 0 || toks.some((t) => t.c.length === 0)) continue
      if (!matchesCompound(el, toks[toks.length - 1]!.c)) continue
      let nodes: Element[] = [el]
      let matched = true
      for (let i = toks.length - 2; i >= 0; i--) {
        const comb = toks[i + 1]!.comb
        const c = toks[i]!.c
        const next: Element[] = []
        for (const n of nodes) {
          if (comb === '+' || comb === '~')
            for (
              let s = n.previousElementSibling;
              s;
              s = s.previousElementSibling
            ) {
              if (matchesCompound(s, c)) next.push(s)
              if (comb === '+') break
            }
          else if (comb === '>') {
            const p = n.parentElement
            if (p && matchesCompound(p, c)) next.push(p)
          } else
            for (let p = n.parentElement; p; p = p.parentElement)
              if (matchesCompound(p, c)) next.push(p)
        }
        if (next.length === 0) {
          matched = false
          break
        }
        nodes = next
      }
      if (matched) out.push({ rule, part })
    }
  return out
}

/** Every `name: value` a rule's body declares, whitespace normalised. */
function declsOf(body: string): Array<[string, string]> {
  const out: Array<[string, string]> = []
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0)
      out.push([
        d.slice(0, i).trim(),
        d
          .slice(i + 1)
          .replace(/\s+/g, ' ')
          .trim(),
      ])
  }
  return out
}

// ── The model profile page (17d.4, 17d.4a) ─────────────────────────────

const MODEL = 'Qwen3-27B'
const MODEL_PUT = `/api/profiles/model/${MODEL}`
const POOLING = en('model-profile-pooling')

/** The model profile page, loaded, with `values` and the save's `answer`. */
async function modelPage(options: BffOptions = {}) {
  cleanup()
  startAt(`#/profiles/model/${MODEL}`)
  const bff = fakeBff({
    instances: INSTANCES,
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc(
          {},
          { revision: 'r1', values: { embeddings: 'true', pooling: 'mean' } },
        ),
      ),
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: MODEL })
  await within(main).findByRole('button', { name: en('save-button') })
  return { bff, main }
}

/** The Save bar. */
function bar(main: HTMLElement): HTMLElement {
  const el = main.querySelector('.save-bar')
  expect(el, 'the sticky Save bar').toBeTruthy()
  return el as HTMLElement
}

/** The bar's own status line (the offer adds a status of its own inside the
 *  bar, so the bar's line is picked by its class, not by counting). */
function barStatus(main: HTMLElement): HTMLElement {
  const all = within(bar(main)).queryAllByRole('status', {
    hidden: true,
  })
  const line = all.filter((s) => s.classList.contains('save-status'))
  expect(line, 'the bar has its one status line').toHaveLength(1)
  return line[0]!
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(within(main).getByRole('combobox', { name: label }), {
    target: { value },
  })
}

/** The alerts of the Save bar, with the text each shows: the bar holds the
 *  offer's alert as a row of its own (17d.3a), so the offer's words are read
 *  here, and their absence is the offer's alert gone. */
function barAlerts(main: HTMLElement): string[] {
  return within(bar(main))
    .queryAllByRole('alert')
    .map((a) => text(a))
}

/** The Sign in buttons of the Save bar, outside any dialog. */
function barSignIns(main: HTMLElement): HTMLButtonElement[] {
  return (
    within(bar(main)).queryAllByRole('button', {
      name: en('session-sign-in'),
    }) as HTMLButtonElement[]
  ).filter((b) => b.closest('dialog') === null)
}

function saveButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('save-button'),
  }) as HTMLButtonElement
}

/** Saves and waits for the n-th PUT of this model profile. */
async function saveModel(
  main: HTMLElement,
  bff: ReturnType<typeof fakeBff>,
  n = 1,
) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', MODEL_PUT)).toHaveLength(n))
  await waitFor(() =>
    expect(text(barStatus(main)), 'the save answered').not.toBe(
      en('save-saving'),
    ),
  )
}

/** Edits Pooling and saves, the save answering `answer`. */
async function modelSave(
  answer: NonNullable<BffOptions['modelProfileSave']>,
  options: BffOptions = {},
) {
  const { bff, main } = await modelPage({
    modelProfileSave: answer,
    ...options,
  })
  pick(main, POOLING, 'rank')
  await saveModel(main, bff)
  return { bff, main }
}

test('16 17d.4a, 17d.1a: a save answering restart ids says save-saved-restart-ids with the display names, and the offer is a row of the Save bar under its buttons, one secondary button per instance with the rotate-cw icon', async () => {
  const { main } = await modelSave(() =>
    reply(200, {
      revision: 'r2',
      running: true,
      restart: ['default', 'coder'],
    }),
  )
  expect(text(barStatus(main)), 'the names, joined by ", "').toBe(
    en('save-saved-restart-ids', `${NAME.default}, ${NAME.coder}`),
  )
  const buttons = offerButtons(main)
  expect(
    buttons.map((b) => text(b)),
    'one button per id, in the reply order',
  ).toEqual([
    en('restart-offer-button', NAME.default),
    en('restart-offer-button', NAME.coder),
  ])
  expectRotateIcons(buttons)
  for (const b of buttons)
    expect(
      b.closest('.save-bar'),
      `${text(b)}: a row of the bar`,
    ).not.toBeNull()
  // 17d.9a: the offer follows the bar's status and its buttons, before the
  // page's content.
  const actions = bar(main).querySelector('.save-actions')
  expect(actions, 'the bar has its buttons').not.toBeNull()
  expect(before(actions!, buttons[0]!), 'the offer is after the buttons').toBe(
    true,
  )
  expect(
    buttons.every((b) => b.getAttribute('aria-disabled') !== 'true'),
    'the offer can be used at once',
  ).toBe(true)
})

test('16 17d.9: an offer appearing moves no focus: it stays on the Save button that was used', async () => {
  const { bff, main } = await modelPage({
    modelProfileSave: () =>
      reply(200, { revision: 'r2', running: true, restart: ['default'] }),
  })
  const button = saveButton(main)
  button.focus()
  pick(main, POOLING, 'rank')
  await saveModel(main, bff)
  expect(offerButtons(main), 'the offer is there').toHaveLength(1)
  expect(document.activeElement, 'nothing took the focus').toBe(button)
})

test('16 17d.1a: an id the app’s instance list lacks shows as the id, in the button and in the status line', async () => {
  const { main } = await modelSave(() =>
    reply(200, { revision: 'r2', running: true, restart: ['ghost'] }),
  )
  expect(text(barStatus(main)), 'the id, unread').toBe(
    en('save-saved-restart-ids', 'ghost'),
  )
  expect(
    offerButtons(main).map((b) => text(b)),
    'the button names the id',
  ).toEqual([en('restart-offer-button', 'ghost')])
})

test('16 17d.2, 17d.1a: a click sends POST restart for that id, every button of the offer is aria-disabled and the status reads restart-offer-running with the name, and the 204 leaves restart-offer-sent', async () => {
  const gate = deferred()
  const { bff, main } = await modelSave(
    () =>
      reply(200, {
        revision: 'r2',
        running: true,
        restart: ['default', 'coder'],
      }),
    { run: (action) => (action === 'restart' ? gate.promise : reply(204)) },
  )
  const buttons = offerButtons(main)
  const first = offerButton(main, NAME.default)
  press(first)
  await waitFor(() =>
    expect(bff.to('POST', '/api/instances/default/restart')).toHaveLength(1),
  )
  expect(bff.to('POST', '/api/instances/coder/restart'), 'one id').toHaveLength(
    0,
  )
  expect(
    buttons.map((b) => inactive(b)),
    'every button of the offer is switched off while it runs',
  ).toEqual([true, true])
  expect(text(statusOf(main, en('restart-offer-running', NAME.default)))).toBe(
    en('restart-offer-running', NAME.default),
  )
  gate.resolve(reply(204))
  await waitFor(() =>
    expect(
      text(statusOf(main, en('restart-offer-sent', NAME.default))),
      'the sent line',
    ).toBe(en('restart-offer-sent', NAME.default)),
  )
  expect(
    offerButtons(main).map((b) => inactive(b)),
    'the buttons work again',
  ).toEqual([false, false])
})

test('16 17d.2a: a restart answered 504 client-timeout shows write-timeout and the instances list is read again', async () => {
  const { bff, main } = await modelSave(
    () => reply(200, { revision: 'r2', running: true, restart: ['default'] }),
    {
      run: () => reply(504, { key: 'client-timeout', args: [] }),
    },
  )
  const reads = bff.to('GET', '/api/instances').length
  press(offerButton(main, NAME.default))
  await waitFor(() =>
    expect(text(alertOf(main, en('write-timeout')))).toBe(en('write-timeout')),
  )
  await waitFor(() =>
    expect(
      bff.to('GET', '/api/instances').length,
      'the list read again (17f.10)',
    ).toBeGreaterThan(reads),
  )
})

test('16 17d.2: a restart refused instance-no-model-profiles says api-instance-no-model-profiles with the name', async () => {
  const { main } = await modelSave(
    () => reply(200, { revision: 'r2', running: true, restart: ['coder'] }),
    {
      run: () =>
        reply(409, { key: 'instance-no-model-profiles', args: ['coder'] }),
    },
  )
  press(offerButton(main, NAME.coder))
  await waitFor(() =>
    expect(
      text(alertOf(main, en('api-instance-no-model-profiles', NAME.coder))),
    ).toBe(en('api-instance-no-model-profiles', NAME.coder)),
  )
})

test('16 17d.2: a restart answered 401 shows the session alert and a Sign in after it, the focus back on the button clicked', async () => {
  const { main } = await modelSave(
    () => reply(200, { revision: 'r2', running: true, restart: ['default'] }),
    { run: () => reply(401, { key: 'session-required', args: [] }) },
  )
  const button = offerButton(main, NAME.default)
  press(button)
  await waitFor(() =>
    expect(text(alertOf(main, en('session-ended-retry')))).toBe(
      en('session-ended-retry'),
    ),
  )
  const signIn = within(bar(main)).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(
    before(alertOf(main, en('session-ended-retry')), signIn),
    'the Sign in after the alert',
  ).toBe(true)
  expect(document.activeElement, 'the focus back on the button').toBe(button)
})

test('16 17d.2 (17d review D2): on the model profile page, signing in through the offer’s dialog puts the focus back on the restart button that was clicked, and the offer’s session alert and its Sign in are gone', async () => {
  const { main } = await modelSave(
    () => reply(200, { revision: 'r2', running: true, restart: ['default'] }),
    { run: () => reply(401, { key: 'session-required', args: [] }) },
  )
  const button = offerButton(main, NAME.default)
  press(button)
  await waitFor(() =>
    expect(text(alertOf(main, en('session-ended-retry')))).toBe(
      en('session-ended-retry'),
    ),
  )
  press(within(bar(main)).getByRole('button', { name: en('session-sign-in') }))
  await signInInDialog()
  expect(barAlerts(main), 'the offer’s session alert is gone').toEqual([])
  expect(barSignIns(main), 'and its Sign in').toEqual([])
  expect(
    document.activeElement,
    'the focus back on the restart button clicked',
  ).toBe(button)
})

test('16 17d.4: running true with restart [] keeps save-saved-restart and shows no offer, and running false says save-saved', async () => {
  const running = await modelSave(() =>
    reply(200, { revision: 'r2', running: true, restart: [] }),
  )
  expect(text(barStatus(running.main))).toBe(en('save-saved-restart'))
  expect(offerButtons(running.main), 'no id to offer: no button').toEqual([])
  const stopped = await modelSave(() =>
    reply(200, { revision: 'r2', running: false, restart: [] }),
  )
  expect(text(barStatus(stopped.main))).toBe(en('save-saved'))
  expect(offerButtons(stopped.main)).toEqual([])
})

test('16 17d.4a: an edit removes the offer, and the next save answers its own', async () => {
  // The save answers by the call: the first leaves a restart id, the second
  // leaves none, so what the second shows is its own answer and not the echo
  // of the first (17d.4a).
  const { bff, main } = await modelSave((_body, _id, n) =>
    n === 1
      ? reply(200, { revision: 'r2', running: true, restart: ['default'] })
      : reply(200, { revision: 'r3', running: false, restart: [] }),
  )
  expect(offerButtons(main)).toHaveLength(1)
  pick(main, POOLING, 'mean')
  expect(offerButtons(main), 'the edit answers the offer').toEqual([])
  await saveModel(main, bff, 2)
  expect(offerButtons(main), 'the second save left no restart id').toEqual([])
  expect(text(barStatus(main))).toBe(en('save-saved'))
})

// ── The server profile page (17d.3, 17d.3a) ────────────────────────────

const SERVER_PUT = '/api/profiles/server/default'

/** The server profile page of `default`, loaded, with the save's `answer`. */
async function serverPage(options: BffOptions = {}) {
  cleanup()
  startAt('#/profiles/server/default')
  const bff = fakeBff({ instances: INSTANCES, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  await within(main).findByRole('button', { name: en('save-button') })
  return { bff, main }
}

/** Edits the Port and saves, the save answering `answer`. */
async function serverSave(
  answer: NonNullable<BffOptions['serverProfileSave']>,
  options: BffOptions = {},
) {
  const { bff, main } = await serverPage({
    serverProfileSave: answer,
    ...options,
  })
  type(within(main).getByRole('textbox', { name: en('server-port') }), '8581')
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', SERVER_PUT)).toHaveLength(1))
  await waitFor(() =>
    expect(text(barStatus(main)), 'the save answered').not.toBe(
      en('save-saving'),
    ),
  )
  return { bff, main }
}

test('16 17d.3a, 17d.1a: a save answering running true with the document’s instance says save-saved-restart-ids with that instance’s name and offers it as a row of the Save bar under its buttons', async () => {
  const { main } = await serverSave(() =>
    reply(200, { revision: 'r-2', running: true }),
  )
  expect(text(barStatus(main)), 'the document names default').toBe(
    en('save-saved-restart-ids', NAME.default),
  )
  const buttons = offerButtons(main)
  expect(buttons.map((b) => text(b))).toEqual([
    en('restart-offer-button', NAME.default),
  ])
  expectRotateIcons(buttons)
  expect(buttons[0]!.closest('.save-bar'), 'a row of the bar').not.toBeNull()
  const actions = bar(main).querySelector('.save-actions')!
  expect(before(actions, buttons[0]!), 'under the buttons').toBe(true)
})

test('16 17d.3a: running true with the document’s instance null says save-saved-restart and shows no offer', async () => {
  const { main } = await serverSave(
    () => reply(200, { revision: 'r-2', running: true }),
    {
      serverProfile: () => reply(200, serverProfileDoc({}, { instance: null })),
    },
  )
  expect(text(barStatus(main))).toBe(en('save-saved-restart'))
  expect(offerButtons(main), 'no instance: no offer').toEqual([])
})

test('16 17d.3a: an edit removes the offer', async () => {
  const { main } = await serverSave(() =>
    reply(200, { revision: 'r-2', running: true }),
  )
  expect(offerButtons(main)).toHaveLength(1)
  type(within(main).getByRole('textbox', { name: en('server-port') }), '8582')
  expect(offerButtons(main), 'the edit answers the offer').toEqual([])
})

test('16 17d.3a, 17d.2 (17d review D2): on the server profile page, signing in through the offer’s dialog puts the focus back on the restart button that was clicked, and the offer’s session alert and its Sign in are gone', async () => {
  const { main } = await serverSave(
    () => reply(200, { revision: 'r-2', running: true }),
    { run: () => reply(401, { key: 'session-required', args: [] }) },
  )
  const button = offerButton(main, NAME.default)
  press(button)
  await waitFor(() =>
    expect(text(alertOf(main, en('session-ended-retry')))).toBe(
      en('session-ended-retry'),
    ),
  )
  press(within(bar(main)).getByRole('button', { name: en('session-sign-in') }))
  await signInInDialog()
  expect(barAlerts(main), 'the offer’s session alert is gone').toEqual([])
  expect(barSignIns(main), 'and its Sign in').toEqual([])
  expect(
    document.activeElement,
    'the focus back on the restart button clicked',
  ).toBe(button)
})

// ── The Models list (17d.5, 17d.5b) ────────────────────────────────────

// ── The instance page's server profile switch (17d.6a, 17d.7a, 17d.8) ──

/** The pool: one profile the instance runs, two free with a port, one free
 *  with none. */
const POOL: ServerProfile[] = [
  {
    id: 'edge',
    engine: 'llama.cpp',
    port: 8080,
    tags: [],
    instance: 'default',
  },
  { id: 'free-one', engine: 'llama.cpp', port: 8090, tags: [], instance: null },
  { id: 'free-two', engine: 'llama.cpp', port: 8091, tags: [], instance: null },
  { id: 'free-three', engine: 'llama.cpp', tags: [], instance: null },
]

function fakeClock() {
  // shouldAdvanceTime keeps the findBy* waits running: the instance page polls
  // the instances list on a timer.
  vi.useFakeTimers({ shouldAdvanceTime: true })
}

async function flush() {
  await act(async () => {
    for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

/** The instance page of `default`, with its Server profile section present. */
async function instancePage(options: BffOptions = {}) {
  cleanup()
  fakeClock()
  startAt('#/instances/default')
  const bff = fakeBff({
    instances: [
      {
        id: 'default',
        engine: 'llama.cpp',
        state: 'active',
        serverProfile: 'edge',
      },
    ],
    serverProfiles: POOL,
    ...options,
  })
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  const h2 = await within(main).findByRole('heading', {
    level: 2,
    name: en('instance-server-title'),
  })
  const section = (h2.closest('section') ?? h2.parentElement)! as HTMLElement
  return { bff, main, section }
}

/** The section's Value list row: the `dt` and the link. The switch adds a row
 *  of its own to the same list, so the profile's row is the FIRST term. */
function profileRow(section: HTMLElement): {
  dt: HTMLElement
  link: HTMLElement
} {
  const dt = within(section).getAllByRole('term')[0]!
  const dd = dt.parentElement!.querySelector('dd')
  expect(dd, 'the row has its value').not.toBeNull()
  const link = dd!.querySelector('a')
  expect(link, 'the value is a link').not.toBeNull()
  return { dt, link: link as HTMLElement }
}

function switchSelect(section: HTMLElement): HTMLSelectElement {
  return within(section).getByRole('combobox', {
    name: en('instance-server-switch-label'),
  }) as HTMLSelectElement
}

function switchButton(section: HTMLElement): HTMLButtonElement {
  return within(section).getByRole('button', {
    name: en('instance-server-switch'),
  }) as HTMLButtonElement
}

/** The section's status line: the one that carries the switch's answer. */
function sectionStatus(section: HTMLElement): HTMLElement {
  const all = within(section).queryAllByRole('status')
  expect(all, 'the section has its one status line').toHaveLength(1)
  return all[0]!
}

function sentSwitch(bff: ReturnType<typeof fakeBff>): unknown[] {
  return bff.calls
    .filter((c) => c.method === 'PUT' && c.path.endsWith('/profiles/server'))
    .map((c) => c.body)
}

async function switchTo(
  section: HTMLElement,
  bff: ReturnType<typeof fakeBff>,
  profile: string,
) {
  fireEvent.change(switchSelect(section), { target: { value: profile } })
  press(switchButton(section))
  await waitFor(() => expect(sentSwitch(bff)).toHaveLength(1))
}

test('16 17d.6a: with a server profile set the row’s link reads instance-server-open with its name, the dt stays, and after the Value list a select labelled instance-server-switch-label is hinted instance-server-switch-hint, its options the choose entry and the free profiles as instance-server-option, then the button instance-server-switch, aria-disabled while the empty option is chosen, then the status', async () => {
  const { section } = await instancePage()
  const { dt, link } = profileRow(section)
  expect(text(dt), 'the row’s name stays').toBe(en('instance-server-title'))
  expect(text(link), 'the link says Open').toBe(
    en('instance-server-open', 'edge'),
  )
  expect(link.getAttribute('href'), 'and leads there').toBe(
    serverProfileHref('edge'),
  )
  const select = switchSelect(section)
  expect(described(select), 'the hint is the one the brief gives').toContain(
    en('instance-server-switch-hint'),
  )
  expect(
    [...select.options].map((o) => o.textContent ?? ''),
    'the choose entry, then the free rows only',
  ).toEqual([
    en('instance-new-server-choose'),
    en('instance-server-option', 'free-one', '8090'),
    en('instance-server-option', 'free-two', '8091'),
    'free-three',
  ])
  const button = switchButton(section)
  expect(select.value, 'the empty option is chosen').toBe('')
  expect(inactive(button), 'the button waits for a choice').toBe(true)
  // 17d.6: the select, then the button, then the status and the alert place.
  expect(before(select, button), 'the button follows the select').toBe(true)
  expect(before(button, sectionStatus(section)), 'the status follows').toBe(
    true,
  )
})

test('16 17d.6a (17d review, adviser note 1, as its re-check D3 amends it): the switch’s button keeps its own width right after its select: no rule reaching it declares flex-basis: 100% or width: 100%, it stays in the select’s .field-control after the select, and the select’s hint follows the button in the markup', async () => {
  // The adviser note 1 put the button in the control column under the select;
  // its fix, `.field-control select + button { flex-basis: 100% }`, stretched it
  // across the whole column, wider than its select, and stood it between the
  // select and the hint that describes the select (the re-check's D3). The fix
  // removes the rule: the button keeps its own width right after the select (it
  // wraps under it at 320 px, `.field-control` wraps) and the hint sits under
  // both. jsdom applies no CSS, so the width is pinned as the sheet and the
  // hint's place as the markup.
  const { section } = await instancePage()
  const select = switchSelect(section)
  const button = switchButton(section)
  const control = select.closest('.field-control')
  expect(control, 'the select sits in a .field-control').not.toBeNull()
  expect(
    button.closest('.field-control'),
    'the button is in the same control cell as the select',
  ).toBe(control)
  expect(before(select, button), 'the button follows the select').toBe(true)
  // Every rule, the @media ones included: a media rule that stretches the
  // button is the same defect at the width it names.
  const stretched: string[] = []
  for (const { rule, part } of rulesReaching(button, ALL))
    for (const [name, value] of declsOf(rule.body)) {
      // The basis of a `flex` shorthand is its third token; `max-width` and
      // `min-width` are not the `width` that stretches a box.
      const basis =
        name === 'flex-basis' || name === 'width'
          ? value
          : name === 'flex'
            ? (value.split(/\s+/)[2] ?? '')
            : ''
      if (basis === '100%') stretched.push(`${part} { ${name}: ${value} }`)
    }
  expect(
    stretched,
    'no rule stretches the button across the control column (D3)',
  ).toEqual([])
  // The hint the select names: one of its descriptions, after the button, in
  // the same field, so it reads as the line under the pair.
  const ids = (select.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
  for (const id of ids)
    expect(document.getElementById(id), `#${id} is a real id`).not.toBeNull()
  const hint = ids
    .map((id) => document.getElementById(id)!)
    .find((el) => text(el) === en('instance-server-switch-hint'))
  expect(hint, 'the select’s hint is one of its descriptions').toBeDefined()
  expect(
    before(button, hint!),
    'the hint follows the button in the markup',
  ).toBe(true)
  expect(hint!.closest('.field'), 'in the same field').toBe(
    select.closest('.field'),
  )
})

test('16 17d.6a: no free profile says instance-server-no-free with the profiles link, and there is no select and no button', async () => {
  const { section } = await instancePage({
    serverProfiles: POOL.map((row) => ({ ...row, instance: 'default' })),
  })
  await within(section).findByText(en('instance-server-no-free'))
  const link = within(section).getByRole('link', {
    name: en('instance-new-profiles-link'),
  })
  expect(link.getAttribute('href')).toBe('#/profiles/server')
  expect(
    within(section).queryByRole('combobox', {
      name: en('instance-server-switch-label'),
    }),
    'no select',
  ).toBeNull()
  expect(
    within(section).queryByRole('button', {
      name: en('instance-server-switch'),
    }),
    'no button',
  ).toBeNull()
})

test('16 17d.7a, 17d.9: the switch sends PUT profiles/server with the chosen id, says instance-server-switching with the button aria-disabled, and a 200 running false says instance-server-switched with the name; then the list and the pool are read again, the select is back on its empty option and the focus stays on the button', async () => {
  const gate = deferred()
  const { bff, section } = await instancePage({
    instanceServerProfile: () => gate.promise,
  })
  const reads = bff.to('GET', '/api/instances').length
  const poolReads = bff.to('GET', '/api/profiles/server').length
  const button = switchButton(section)
  fireEvent.change(switchSelect(section), { target: { value: 'free-one' } })
  expect(sentSwitch(bff), 'the select alone sends nothing').toEqual([])
  press(button)
  await waitFor(() => expect(sentSwitch(bff)).toHaveLength(1))
  expect(sentSwitch(bff)[0], 'the body').toEqual({ serverProfile: 'free-one' })
  expect(text(sectionStatus(section))).toBe(en('instance-server-switching'))
  expect(inactive(switchButton(section)), 'the button is switched off').toBe(
    true,
  )
  gate.resolve(reply(200, { running: false }))
  await flush()
  expect(text(sectionStatus(section))).toBe(
    en('instance-server-switched', 'free-one'),
  )
  expect(switchSelect(section).value, 'the select is empty again').toBe('')
  expect(
    bff.to('GET', '/api/instances').length,
    'the instances read again',
  ).toBeGreaterThan(reads)
  expect(
    bff.to('GET', '/api/profiles/server').length,
    'the pool read again',
  ).toBeGreaterThan(poolReads)
  expect(document.activeElement, 'the focus stays on the button').toBe(button)
})

test('16 17d.7a, 17d.6a: a 200 running true says instance-server-switched-restart and shows no offer: the page carries its own Restart in the head', async () => {
  const { bff, main, section } = await instancePage({
    instanceServerProfile: () => reply(200, { running: true }),
  })
  await switchTo(section, bff, 'free-two')
  expect(text(sectionStatus(section))).toBe(
    en('instance-server-switched-restart', 'free-two'),
  )
  expect(offerButtons(main), 'no offer anywhere on this page').toEqual([])
})

test('16 17d.8: after 404 client-no-server-profile and 409 client-server-profile-in-use the pool is read again', async () => {
  const cases: [number, string][] = [
    [404, 'client-no-server-profile'],
    [409, 'client-server-profile-in-use'],
  ]
  for (const [status, key] of cases) {
    const { bff, section } = await instancePage({
      instanceServerProfile: () => reply(status, { key, args: ['free-one'] }),
    })
    const poolReads = bff.to('GET', '/api/profiles/server').length
    await switchTo(section, bff, 'free-one')
    await flush()
    expect(
      bff.to('GET', '/api/profiles/server').length,
      `${key}: the pool read again`,
    ).toBeGreaterThan(poolReads)
    cleanup()
  }
})

test('16 17d.8: 404 client-no-instance says instance-not-found, and 503, 502, 401 and 403 say their own', async () => {
  const cases: [() => Response, string][] = [
    [
      () => reply(404, { key: 'client-no-instance', args: ['default'] }),
      en('instance-not-found', 'default'),
    ],
    [
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
    [
      () => reply(502, { key: 'client-service-failed', args: [] }),
      en('api-client-service-failed'),
    ],
    [
      () => reply(401, { key: 'session-required', args: [] }),
      en('session-ended-retry'),
    ],
    [
      () => reply(403, { key: 'client-not-authorized', args: [] }),
      en('api-client-not-authorized'),
    ],
  ]
  for (const [answer, what] of cases) {
    const { bff, section } = await instancePage({
      instanceServerProfile: answer,
    })
    await switchTo(section, bff, 'free-one')
    await flush()
    expect(text(alertOf(section, what)), what).toBe(what)
    cleanup()
  }
})

test('16 17d.8: a 401 leaves a Sign in after the alert, the focus back on the switch button', async () => {
  const { section } = await instancePage({
    instanceServerProfile: () =>
      reply(401, { key: 'session-required', args: [] }),
  })
  const button = switchButton(section)
  fireEvent.change(switchSelect(section), { target: { value: 'free-one' } })
  press(button)
  await flush()
  const alert = alertOf(section, en('session-ended-retry'))
  const signIn = within(section).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(before(alert, signIn), 'the Sign in after the alert').toBe(true)
  expect(document.activeElement, 'the focus back on the button').toBe(button)
  expect(
    isHidden(
      within(section).getByRole('combobox', {
        name: en('instance-server-switch-label'),
      }),
    ),
    'the section stays on screen',
  ).toBe(false)
})
