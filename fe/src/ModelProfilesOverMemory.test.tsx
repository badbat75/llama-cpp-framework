// The two resource refusals of a model profile, on the Models list's on/off
// switch (fe/briefs/11-resource-refusals.md, slice M1): a switch answered 422
// model-profile-over-memory (11 M1.6 to 11 M1.9) and 422
// model-profile-model-missing (11 M1.13). The switch alert says the key's text
// bare, with no "Not saved: " in front and no api-other; no Sign in, the switch
// status empty, the list read again with the row as read, and the focus on the
// switch that was pressed. The strings themselves (11 M1.1, 11 M1.10) are
// strings.test.ts, and the profile page (11 M1.2 to 11 M1.5, 11 M1.11, 11
// M1.12) is ModelProfileOverMemory.test.tsx. The switch's own rules are 2c's,
// in ModelProfilesSwitch.test.tsx, whose helpers this file repeats (a test
// file imports no other test file).
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
  en,
  fakeBff,
  reply,
  shownText,
  startAt,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

/** The fixture of the brief (section 4 and section 7): the profile `qwen-27b`,
 * switched on, whose switch the user presses. */
const MEMORY = {
  key: 'model-profile-over-memory',
  args: ['qwen-27b', 'default', '30721', '24560'],
}
const MISSING = {
  key: 'model-profile-model-missing',
  args: ['qwen-27b', 'default', '/srv/models/qwen-27b.gguf'],
  field: 'model',
}

function modelProfile(
  id: string,
  enabled: boolean,
  model = '',
  engine = 'llama.cpp',
  engineAvailable = true,
): ModelProfile {
  return { id, engine, engineAvailable, enabled, model }
}

/** Two on, one off, in the bff's order. */
const LIST: ModelProfile[] = [
  modelProfile('qwen-27b', true, '/m/qwen/qwen-27b.gguf'),
  modelProfile('Coder-30B', true, '/m/coder/Coder-30B.gguf'),
  modelProfile('Old-Coder', false),
]

/** What the read after the refusal says: a refusal writes nothing, so
 * `qwen-27b` is still off, as the file has it. */
const READ_AFTER: ModelProfile[] = [
  modelProfile('qwen-27b', false, '/m/qwen/qwen-27b.gguf'),
  LIST[1]!,
  LIST[2]!,
]

beforeEach(() => {
  startAt('#/profiles/model')
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfilesSwitch.test.tsx) ───────────────────────────────

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

async function flush() {
  await act(async () => {
    for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

/** The app at #/profiles/model as an administrator, the table loaded. */
async function open(options: BffOptions = {}) {
  fakeClock()
  const bff = fakeBff({ modelProfiles: LIST, ...options })
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  expect(
    within(main).queryByRole('table', { name: en('model-profiles-title') }),
    'the table is loaded',
  ).not.toBeNull()
  return { bff, main }
}

/** GET /api/profiles/model: `first` on open, then READ_AFTER for the re-read. */
function reads(): BffOptions['modelProfiles'] {
  let n = 0
  return () => (++n === 1 ? reply(200, LIST) : reply(200, READ_AFTER))
}

function modelProfilesReads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', '/api/profiles/model').length
}

/** The bodies of every switch PUT, with the path each went to. */
function writes(bff: ReturnType<typeof fakeBff>): [string, unknown][] {
  return bff.calls
    .filter(
      (c) => c.method === 'PUT' && c.path.startsWith('/api/profiles/model/'),
    )
    .map((c) => [c.path, c.body])
}

function path(id: string): string {
  return `/api/profiles/model/${encodeURIComponent(id)}/enabled`
}

function table(main: HTMLElement): HTMLElement {
  return within(main).getByRole('table', { name: en('model-profiles-title') })
}

function dataRows(main: HTMLElement): HTMLElement[] {
  return within(table(main))
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

/** The rows' cells' text: id, engine, On/Off, the row's − of 22 D4.1 left out
 *  (its words are its accessible name, in a `.visually-hidden` span). */
function rows(main: HTMLElement): string[][] {
  return dataRows(main).map((row) =>
    [...row.querySelectorAll('td, th')].map((c) => shownText(c)),
  )
}

function switchOf(main: HTMLElement, id: string): HTMLInputElement {
  return within(table(main)).getByRole('switch', {
    name: id,
  }) as HTMLInputElement
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

/** The switch region's status (2c.50): the second status of the page. */
function switchStatus(main: HTMLElement): HTMLElement {
  const all = within(main).getAllByRole('status')
  expect(all, 'the filter status and the switch status').toHaveLength(2)
  return all[1]!
}

function alerts(main: HTMLElement): string[] {
  return within(main)
    .queryAllByRole('alert')
    .map((a) => a.textContent ?? '')
}

/** The switch region's alert paragraph, by the class the brief names. */
function switchAlert(main: HTMLElement): HTMLElement | null {
  return main.querySelector('.model-profiles-switch-alert')
}

/** The switch region's Sign in buttons (outside any dialog). */
function signIns(main: HTMLElement): HTMLButtonElement[] {
  return (
    within(main).queryAllByRole('button', {
      name: en('session-sign-in'),
    }) as HTMLButtonElement[]
  ).filter((b) => b.closest('dialog') === null)
}

/** The page, the switch of `qwen-27b` (on) pressed, and 422 `body` answered,
 * the re-read done. */
async function refused(body: Record<string, unknown>) {
  const { bff, main } = await open({
    modelProfileSwitch: () => reply(422, body),
    modelProfiles: reads(),
  })
  const el = switchOf(main, 'qwen-27b')
  press(el)
  await flush()
  expect(writes(bff), 'the switch wrote its change once').toEqual([
    [path('qwen-27b'), { enabled: false }],
  ])
  return { bff, main, el }
}

/** The refusal's English, bare: the list's alert has no "Not saved: ". */
function refusalText(refusal: { key: string; args: string[] }): string {
  return en(`api-${refusal.key}`, ...refusal.args)
}

/** The table answered the refusal with one alert (its text is M1.6, M1.13). */
function answered(main: HTMLElement) {
  expect(alerts(main), 'the table answered with one alert').toHaveLength(1)
}

// ── The memory refusal (11 M1.6 to 11 M1.9) ────────────────────────────────

test('11 M1.6: a switch refused model-profile-over-memory says the text in the switch alert, bare, with the MiB figures as sent and no api-other', async () => {
  const { main } = await refused(MEMORY)
  const text = refusalText(MEMORY)
  expect(text, 'the needed MiB as sent').toContain('it needs 30721 MiB')
  expect(text, 'the available MiB as sent').toContain('the node has 24560 MiB')
  expect(alerts(main), 'the one alert of the table').toEqual([text])
  const p = switchAlert(main)
  expect(p, 'the alert is .model-profiles-switch-alert').not.toBe(null)
  expect(p!.getAttribute('role'), 'role="alert"').toBe('alert')
  expect(p!.textContent, 'bare, with no "Not saved: " in front').toBe(text)
  expect(
    within(main).queryByText(en('api-other', MEMORY.key)),
    'no api-other element',
  ).toBeNull()
  expect(
    document.body.textContent ?? '',
    'no api-other sentence anywhere',
  ).not.toContain('The request was refused')
})

test('11 M1.7: after the over-memory refusal no sign-in button shows and the switch status line is empty', async () => {
  // A guard (2c's 22 and 25 already do this for a failure with a known key).
  const { main } = await refused(MEMORY)
  answered(main)
  expect(signIns(main), 'no Sign in after the alert').toEqual([])
  expect(switchStatus(main).textContent, 'status empty').toBe('')
})

test('11 M1.8: after the over-memory refusal the list is read again and the row shows what the read says: Off, unchecked', async () => {
  // A guard (2c's 26: the row is re-read after a failure).
  const { bff, main } = await refused(MEMORY)
  answered(main)
  expect(modelProfilesReads(bff), 'the profiles read again once').toBe(2)
  expect(switchOf(main, 'qwen-27b').checked, 'the switch shows the read').toBe(
    false,
  )
  expect(rows(main)[0], 'the row reads Off').toEqual([
    'qwen-27b',
    'llama.cpp',
    en('model-profiles-status-off'),
  ])
})

test('11 M1.9: after the over-memory refusal the focus is on the switch that was pressed', async () => {
  // A guard (2c's 27).
  const { main, el } = await refused(MEMORY)
  answered(main)
  expect(document.activeElement, 'focus on the pressed switch').toBe(el)
})

// ── The missing model file (11 M1.13) ───────────────────────────────────────

test('11 M1.13: a switch refused model-profile-model-missing says the text in the switch alert, no api-other, and the focus stays on the switch', async () => {
  const { main } = await refused(MISSING)
  const text = refusalText(MISSING)
  expect(text, 'the path as sent').toContain('`/srv/models/qwen-27b.gguf`')
  const p = switchAlert(main)
  expect(p, 'the alert is .model-profiles-switch-alert').not.toBe(null)
  expect(p!.getAttribute('role'), 'role="alert"').toBe('alert')
  expect(p!.textContent, 'the key text, bare').toBe(text)
  expect(alerts(main), 'the one alert of the table').toEqual([text])
  expect(
    within(main).queryByText(en('api-other', MISSING.key)),
    'no api-other element',
  ).toBeNull()
  expect(
    document.body.textContent ?? '',
    'no api-other sentence anywhere',
  ).not.toContain('The request was refused')
  expect(document.activeElement, 'focus on the pressed switch').toBe(
    switchOf(main, 'qwen-27b'),
  )
})
