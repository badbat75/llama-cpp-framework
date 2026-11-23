// The fixtures and checks the tests of the save bar's alert share (198b to
// 198h in fe/briefs/02h-model-profile-save.md): the two pages that carry a
// save bar, read from a fake bff, their controls, and the bar's state. The
// rule under test is the save bar's own (SaveBar.tsx, useSaveAlert), so every
// check is made on the rendered page. A test file imports no other test file,
// so the fixtures of ServerSave.test.tsx and ModelProfileSave*.test.tsx are
// repeated here in the smaller form these tests need.
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { expect } from 'vitest'
import App from '../App.tsx'
import {
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  serverProfileDoc,
  startAt,
  type BffOptions,
} from './bff.ts'

export const ID = 'Qwen3-27B'
export const PROFILE_HASH = `#/profiles/model/${ID}`
export const SERVER_HASH = '#/profiles/server/default'
export const PROFILE_GET = `/api/profiles/model/${ID}`
export const PROFILE_PUT = `/api/profiles/model/${ID}`
export const SERVER_GET = '/api/profiles/server/default'
export const SERVER_PUT = '/api/profiles/server/default'

/** The fake bff of a page (test/bff.ts). */
export type Bff = ReturnType<typeof fakeBff>

export const MODE = en('model-profile-embedding-mode')
export const POOLING = en('model-profile-pooling')
export const CTX = en('model-profile-context-size')
export const FIT = en('model-profile-fit-to-vram')
export const PORT = en('server-port')
export const BIND = en('server-bind-to')
export const BIND_LOCALHOST = en('server-bind-localhost')
export const WEBUI = en('server-web-ui')
export const WEBUI_TOGGLE = en('server-web-ui-toggle')
export const LOG_LEVEL = en('server-log-level')
export const SIGN_IN = en('session-sign-in')
export const RELOAD = en('save-reload')
export const SAVE = en('save-button')
export const UNSAVED = en('save-unsaved')
export const SAVING = en('save-saving')

/** The Server page's loaded Port (198h's example). */
export const PORT_LOADED = '8080'

/** What the model profile page reads: Context size present (its default box
 * unticked and its input editable, 156), Fit to VRAM off and valid (185: only
 * an invalid `fit` blocks a Save, and these rows must reach the PUT), the
 * Embeddings selects set, and the loaded Pooling value not one of the options,
 * so Pooling shows the extra option 132a gives a loaded value that is not on
 * the list. */
export const AS_LOADED: Record<string, string> = {
  'ctx-size': '172032',
  fit: 'false',
  embeddings: 'true',
  pooling: 'weird',
}

/** The page pair a fixture returns: the fake bff, and the page's `main`. */
export type Page = {
  bff: ReturnType<typeof fakeBff>
  main: HTMLElement
}

/** The model profile page, read as `AS_LOADED` with `over` on top and
 * revision `r1`, answering the save with `save`. */
export async function profileLoaded(
  save?: BffOptions['modelProfileSave'],
  over: Record<string, string> = {},
): Promise<Page> {
  startAt(PROFILE_HASH)
  const bff = fakeBff({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc(
          {},
          { revision: 'r1', values: { ...AS_LOADED, ...over } },
        ),
      ),
    modelProfileSave: save,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The Server page, read with the Models folder set (so its input is
 * editable) and Port at `PORT_LOADED`, answering the save with `save`. */
export async function serverLoaded(
  save?: BffOptions['serverProfileSave'],
  over: Record<string, unknown> = {},
): Promise<Page> {
  startAt(SERVER_HASH)
  const bff = fakeBff({
    serverProfile: () =>
      reply(
        200,
        serverProfileDoc({
          ModelsDir: '/var/lib/models',
          Port: Number(PORT_LOADED),
          ...over,
        }),
      ),
    serverProfileSave: save,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

// ── Controls ──────────────────────────────────────────────────────────────

/** A text input of a field: in its group, or labelled. */
export function box(main: HTMLElement, label: string): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: label })
  return (
    g
      ? within(g).getByRole('textbox', { name: label })
      : within(main).getByRole('textbox', { name: label })
  ) as HTMLInputElement
}

export function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

/** A checkbox by the name of its group and of the box. */
export function checkBox(
  main: HTMLElement,
  label: string,
  name: string,
): HTMLInputElement {
  return within(within(main).getByRole('group', { name: label })).getByRole(
    'checkbox',
    { name },
  ) as HTMLInputElement
}

/** The model profile page's Fit to VRAM checkbox: the field holds one, named
 * by the toggle text 184 picks, so it is taken by its position. */
export function fitBox(main: HTMLElement): HTMLInputElement {
  return within(within(main).getByRole('group', { name: FIT })).getByRole(
    'checkbox',
  ) as HTMLInputElement
}

/** Types `value` into a text input. */
export function typeInput(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

export function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

/** The control a refusal marks: the input of `label`, or its select. */
export function marked(
  main: HTMLElement,
  label: string,
  kind: 'box' | 'select',
): HTMLElement {
  return kind === 'select' ? selectOf(main, label) : box(main, label)
}

export function saveButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', { name: SAVE }) as HTMLButtonElement
}

/** Clicks Save changes and waits for the PUT (a client check sends nothing). */
export async function sent(
  main: HTMLElement,
  bff: ReturnType<typeof fakeBff>,
  url: string,
) {
  fireEvent.click(saveButton(main))
  await waitForPut(bff, url)
}

/** Waits for the one PUT of a save. */
export async function waitForPut(bff: ReturnType<typeof fakeBff>, url: string) {
  await waitFor(() => expect(bff.to('PUT', url)).toHaveLength(1))
}

// ── The bar ───────────────────────────────────────────────────────────────

/** The model profile page's status line: the save bar's (the settings section
 * is loaded, the bar is below it). */
export function profileStatus(main: HTMLElement): HTMLElement {
  within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  return oneStatus(main)
}

/** The Server page's status line: the save bar's (the header's run status is
 * outside the bar, 1c.40). */
export function serverStatus(main: HTMLElement): HTMLElement {
  within(main).getByRole('heading', { level: 2, name: en('server-title') })
  return oneStatus(main)
}

/** The save bar's role="status". A card that announces what the user did keeps
 * its own status region in the DOM from its first render (brief 14 GD1.9), so
 * the bar's line is picked inside the bar, not by counting the page's regions
 * (fe/briefs/14-gpu-distribution.md section 6, ruling of 2026-11-21). */
function oneStatus(main: HTMLElement): HTMLElement {
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => s.closest('.save-bar') !== null)
  expect(all, 'the save bar has its status line').toHaveLength(1)
  return all[0]!
}

/** The bar's message area: the status line, the alert and the alert's answer
 * button (SaveBar.tsx). */
export function messages(status: HTMLElement): HTMLElement {
  const bar = status.parentElement
  expect(bar, 'the status line sits in the bar').not.toBe(null)
  return bar!
}

/** The bar's alert says `text`, inside a role="alert" of its own. */
export function expectAlert(status: HTMLElement, text: string, what: string) {
  const el = within(messages(status)).getByText(text)
  expect(el.closest('[role="alert"]'), `${what}: in role="alert"`).not.toBe(
    null,
  )
}

/** The bar shows no alert. */
export function expectNoAlert(status: HTMLElement, what: string) {
  expect(
    within(messages(status)).queryByRole('alert'),
    `${what}: no alert in the bar`,
  ).toBeNull()
}

/** The bar's answer button of a kind. */
export function expectAction(status: HTMLElement, name: string, what: string) {
  expect(
    within(messages(status)).getByRole('button', { name }),
    `${what}: the button ${name}`,
  ).toBeTruthy()
}

/** Clicks the bar's answer button named `name`. */
export function pressAction(status: HTMLElement, name: string) {
  fireEvent.click(within(messages(status)).getByRole('button', { name }))
}

/** The form is clean: Save changes disabled, the status line empty (30a). */
export function expectClean(
  main: HTMLElement,
  status: HTMLElement,
  what: string,
) {
  expect(
    saveButton(main).getAttribute('aria-disabled'),
    `${what}: Save disabled`,
  ).toBe('true')
  expect(status.textContent, `${what}: the status line empty`).toBe('')
}

/** The form is dirty: Save has no aria-disabled (SaveBar.tsx sets the
 * attribute only when it disables), the status line says save-unsaved. */
export function expectDirty(
  main: HTMLElement,
  status: HTMLElement,
  what: string,
) {
  expect(
    saveButton(main).hasAttribute('aria-disabled'),
    `${what}: Save enabled`,
  ).toBe(false)
  expect(status.textContent, `${what}: the status line`).toBe(UNSAVED)
}

// ── The mark of a field (198e) ────────────────────────────────────────────

/** `el` is marked invalid with the error `text` under it (142). */
export function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  const ids = (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
  const described = ids.map(
    (id) => document.getElementById(id)?.textContent ?? `(no #${id})`,
  )
  expect(described, `${what} aria-describedby`).toContain(text)
  const error = ids
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
}

/** `el` carries no mark. */
export function expectNoError(el: HTMLElement, what: string) {
  expect(el.getAttribute('aria-invalid') ?? 'false', what).toBe('false')
}

// ── 198f: what is focusable, and what is announced ────────────────────────

/** Every control of the page a keyboard user can reach: not disabled, not
 * inert, not hidden. */
export function focusable(): HTMLElement[] {
  return Array.from(
    document.body.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]:not([tabindex="-1"])',
    ),
  ).filter(
    (el) =>
      !el.hasAttribute('disabled') &&
      !el.hasAttribute('inert') &&
      !el.closest('[inert]') &&
      el.getAttribute('aria-hidden') !== 'true' &&
      getComputedStyle(el).display !== 'none',
  )
}

function nameOf(el: HTMLElement): string {
  const text = (el.getAttribute('aria-label') ?? el.textContent ?? '').trim()
  return `${el.tagName.toLowerCase()} "${text.slice(0, 24)}"`
}

/** No live region of the page carries `text`: not the status line, not an
 * alert, not anything the page marked aria-live (198f). */
export function expectNotAnnounced(
  main: HTMLElement,
  text: string,
  what: string,
) {
  const regions = main.querySelectorAll<HTMLElement>(
    '[role="status"], [role="alert"], [aria-live]',
  )
  for (const region of regions) {
    const kind =
      region.getAttribute('role') ?? region.getAttribute('aria-live') ?? 'live'
    expect(
      region.textContent ?? '',
      `${what}: the ${kind} region does not announce`,
    ).not.toContain(text)
  }
}

/** The quiet of a form that just became clean (198f): the focus did not move,
 * no control stopped being focusable, nothing announced the alert that went,
 * the status line is empty and Save is disabled. The caller's own "no alert"
 * check comes last, so one red names the alert. */
export function expectQuiet(
  main: HTMLElement,
  status: HTMLElement,
  focusOn: HTMLElement,
  was: HTMLElement[],
  alertText: string,
  what: string,
) {
  expect(document.activeElement, `${what}: the focus did not move`).toBe(
    focusOn,
  )
  const now = new Set(focusable())
  for (const el of was) {
    expect(now.has(el), `${what}: ${nameOf(el)} is still focusable`).toBe(true)
  }
  expectNotAnnounced(main, alertText, what)
  expectClean(main, status, what)
}

/** A control's way to leave its loaded state, and to come back to it. The pair
 * is armed while the form is loaded, so the loaded state is the one read from
 * the control (198d: "clean" by any route). */
export type Moves = {
  dirty: (main: HTMLElement) => void
  clean: (main: HTMLElement) => void
}

/** A text field: typed `to`, then the loaded text typed back. */
export function textMoves(
  label: string,
  to: string,
): (main: HTMLElement) => Moves {
  return (main) => {
    const input = box(main, label)
    const was = input.value
    return {
      dirty: () => typeInput(input, to),
      clean: () => typeInput(input, was),
    }
  }
}

/** A select: picked onto `to`, then picked back on its loaded option, the
 * extra option of 132a included. */
export function selectMoves(
  label: string,
  to: string,
): (main: HTMLElement) => Moves {
  return (main) => {
    const select = selectOf(main, label)
    const was = select.value
    return {
      dirty: () => fireEvent.change(select, { target: { value: to } }),
      clean: () => fireEvent.change(select, { target: { value: was } }),
    }
  }
}

/** A checkbox: clicked off its loaded state, clicked back (two clicks put it
 * on the state it loaded in). */
export function clickMoves(
  get: (main: HTMLElement) => HTMLElement,
): (main: HTMLElement) => Moves {
  return (main) => {
    const control = get(main)
    return {
      dirty: () => fireEvent.click(control),
      clean: () => fireEvent.click(control),
    }
  }
}
