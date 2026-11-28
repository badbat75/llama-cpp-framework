// Brief 22, slice IP5, the page half (fe/briefs/22-model-sources.md, "IP5",
// IP5.4, IP5.5, IP5.6, IP5.7): the hardware-free model profile page at
// #/profiles/model/<id>, `src/ModelProfilePage.tsx` and its cards,
// `src/ModelProfileCards.tsx`.
//
// The head loses the state line's switch and gains a Used by row (IP5.4). The
// page keeps the eight cards of IP5.5, in today's order, each filtered to
// `checks::MODEL_PROFILE_STAY_KEYS`: no control of a `PLACEMENT_KEYS` key is
// rendered, the four placement cards are gone, and the page reads no
// `model-info` and no `/api/node/devices` any more (IP5.5). The Save bar's
// Delete follows `usedBy` (IP5.6) and the bar shows the bff's
// 422 model-profile-placement-key (IP5.7).
//
// The words are pinned in `src/test/bff.ts`'s EN map, which App.test.tsx
// "1a strings" compares against `strings/en_US.json`. The key lists the tests
// loop over are the stub's mirror of `crates/core/src/checks.rs`.
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
import { cardHeadingId, fieldId } from './ModelProfileCards.tsx'
import {
  INSTANCES,
  MODEL_PROFILE_ID,
  MODEL_PROFILE_STAY_KEYS,
  PLACEMENT_KEYS,
  en,
  fakeBff,
  instanceName,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

beforeEach(() => startAt(`#/profiles/model/${MODEL_PROFILE_ID}`))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** The page, read from the stub. `usedBy` is who lists the model (IP5.0). */
async function page(options: BffOptions = {}, usedBy: string[] = []) {
  const bff = fakeBff({
    instances: INSTANCES,
    // IP5.0: the document the page reads carries no `enabled` and no key of
    // `PLACEMENT_KEYS`, and carries `usedBy`. `enabled: undefined` is dropped by
    // the JSON the stub sends, so the page sees a document without it.
    modelProfile: () =>
      reply(200, {
        ...modelProfileDoc(
          {
            embeddings: 'true',
            pooling: 'mean',
            mmproj: '/m/vision.gguf',
            'spec-type': 'ngram-mod',
          },
          { revision: 'r1', usedBy },
        ),
        enabled: undefined,
      }),
    ...options,
  })
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: MODEL_PROFILE_ID })
  const main = screen.getByRole('main')
  // The read lands. IP5.5 keeps Assets the first card, so its heading is the
  // marker that stands today and after the slice: a page that draws no card is
  // a page that cannot read the document IP5.0 describes, which is IP5.0's red.
  await waitFor(() =>
    expect(
      [...main.querySelectorAll('h3')].map((h) => (h.textContent ?? '').trim()),
      'IP5.0: the page reads the document of IP5.0 and builds its cards, Assets first, from it. It draws no card at all: it is still reading the pre-IP5 document, with enabled and the placement keys',
    ).toContain(en('model-profile-assets')),
  )
  return { bff, main }
}

/** The page's head `dl.assets` row labelled `label`, its `dd` text. */
function headRow(main: HTMLElement, label: string): HTMLElement {
  const dt = [...main.querySelectorAll('dl.assets dt')].find(
    (el) => (el.textContent ?? '').trim() === label,
  )
  expect(dt, `the head has a ${label} row`).not.toBeUndefined()
  const dd = dt!.nextElementSibling
  expect(
    dd?.tagName,
    `the ${label} row is followed by its dd (the shape of ServerProfilePage.tsx 434)`,
  ).toBe('DD')
  return dd! as HTMLElement
}

/** The links of a place, as text and href. */
function links(el: HTMLElement): { text: string; href: string | null }[] {
  return [...el.querySelectorAll('a')].map((a) => ({
    text: (a.textContent ?? '').trim(),
    href: a.getAttribute('href'),
  }))
}

/** The page's card titles, in DOM order. */
function cardTitles(main: HTMLElement): string[] {
  return [...main.querySelectorAll('h3')].map((h3) =>
    (h3.textContent ?? '').trim(),
  )
}

/** The card section whose `h3` is the title of `titleKey`. */
function cardOf(main: HTMLElement, titleKey: string): HTMLElement {
  const h3 = within(main).getByRole('heading', {
    level: 3,
    name: en(titleKey),
  })
  const card = h3.closest('section')
  expect(card, `${en(titleKey)}: its Card section`).toBeTruthy()
  return card as HTMLElement
}

/** The keys of the fields a card draws, in page order (a field's control is
 *  `<id>-control`, `ModelProfileCards.tsx` 2029). */
function cardFieldKeys(card: HTMLElement): string[] {
  return [...card.querySelectorAll('[id$="-control"]')].map((el) =>
    el.id.slice('model-profile-'.length, '-control'.length * -1),
  )
}

/** The jump list's buttons, in order. */
function jumpTitles(main: HTMLElement): string[] {
  const list = main.querySelector('#model-profile-jump-list')
  expect(list, 'the page has its jump list').not.toBeNull()
  return [...list!.querySelectorAll('button')].map((b) =>
    (b.textContent ?? '').trim(),
  )
}

/** The Save bar, its status line, its alerts and its buttons. */
function bar(main: HTMLElement): HTMLElement {
  const el = main.querySelector('.save-bar')
  expect(el, 'the sticky Save bar').toBeTruthy()
  return el as HTMLElement
}

function barAlerts(main: HTMLElement): string[] {
  return within(bar(main))
    .queryAllByRole('alert')
    .map((a) => (a.textContent ?? '').replace(/\s+/g, ' ').trim())
}

function barStatus(main: HTMLElement): HTMLElement {
  const line = within(bar(main))
    .queryAllByRole('status', { hidden: true })
    .filter((s) => s.classList.contains('save-status'))
  expect(line, 'the bar has its one status line').toHaveLength(1)
  return line[0]!
}

/** The body of the n-th save the page sent (the stub parses it). */
function savedBody(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const calls = bff.calls.filter(
    (c) =>
      c.method === 'PUT' &&
      c.path === `/api/profiles/model/${MODEL_PROFILE_ID}`,
  )
  expect(calls.length, `the page sent ${n} save`).toBeGreaterThanOrEqual(n)
  return calls[n - 1]!.body as Record<string, unknown>
}

// ── IP5.0: the read the page is built from ──────────────────────────────

test('IP5.0: the page reads a document that carries usedBy and no enabled', async () => {
  const { main } = await page({}, ['default'])
  expect(
    [...main.querySelectorAll('h3')].map((h) => (h.textContent ?? '').trim()),
    'IP5.0: GET /api/profiles/model/{id} answers a document with no enabled and no placement key, and the page builds its cards from it',
  ).toContain(en('model-profile-assets'))
})

// ── IP5.4: the head ─────────────────────────────────────────────────────

test('IP5.4: the page has no switch, and its head Used by row lists the instance profiles that use the model', async () => {
  const { main } = await page({}, ['unassigned', 'default'])
  expect(
    [...main.querySelectorAll('[role="switch"]')],
    'IP5.4: a model profile has no on/off state, so the page shows no switch',
  ).toEqual([])
  const dd = headRow(main, en('model-profiles-column-used-by'))
  expect(
    links(dd),
    'IP5.4: the head names the same instance profiles as the list, each reaching its entry page',
  ).toEqual([
    { text: 'unassigned', href: '#/profiles/instance/unassigned' },
    { text: 'default', href: '#/profiles/instance/default' },
  ])
})

test('IP5.4: with no instance profile using the model, the head row says so in words', async () => {
  const { main } = await page()
  expect(
    headRow(main, en('model-profiles-column-used-by')).textContent,
    'IP5.4: an unused model profile names no instance profile',
  ).toContain(en('model-profiles-unused'))
})

// ── IP5.5: the cards that stay ──────────────────────────────────────────

test('IP5.5: the page draws exactly the eight cards of IP5.5, in DOM order, and its jump list matches them', async () => {
  const { main } = await page()
  const eight = [
    en('model-profile-assets'),
    en('model-profile-embeddings'),
    en('model-profile-kv-cache'),
    en('model-profile-chat-reasoning'),
    en('model-profile-sampling-overrides'),
    en('model-profile-advanced'),
    en('model-profile-vision'),
    en('model-profile-speculative-decoding'),
  ]
  expect(
    cardTitles(main),
    'IP5.5: GPU distribution, Tensor placement, Hardware config and Resource / context are gone, and every card that stays keeps its place in today’s order',
  ).toEqual(eight)
  expect(
    jumpTitles(main),
    'IP5.5: "On this page" lists the same eight, in the same order',
  ).toEqual(eight)
})

test('IP5.5: the KV cache card keeps cache-reuse alone, and its intro is the new key that says where the rest went', async () => {
  const { main } = await page()
  const card = cardOf(main, 'model-profile-kv-cache')
  expect(
    cardFieldKeys(card),
    'IP5.5: the card keeps cache-reuse and nothing else (the context, the sizes and the cache types are the instance profile’s)',
  ).toEqual(['cache-reuse'])
  expect(
    within(card)
      .getByText(en('model-profile-kv-cache-reuse-intro'))
      .textContent!.replace(/\s+/g, ' ')
      .trim(),
    'IP5.5: the card’s intro is the text of the new key',
  ).toBe(en('model-profile-kv-cache-reuse-intro'))
})

test('IP5.5: no field of a key that moved to the instance profile is rendered', async () => {
  await page()
  const rendered = PLACEMENT_KEYS.filter(
    (key) =>
      document.getElementById(`${fieldId(key)}-control`) !== null ||
      document.getElementById(fieldId(key)) !== null,
  )
  expect(
    rendered,
    'IP5.5: every key of checks::PLACEMENT_KEYS is set per model in the instance profile’s entry, so the model profile page shows no control for any of them',
  ).toEqual([])
})

test('IP5.5: every key that stays keeps its control', async () => {
  const { main } = await page()
  // The keys IP5.5 names for the eight cards, with the document the helper
  // reads (the vision file present, the n-gram drafter on). A key the page
  // shows read-only, a file of the Assets card, has no control to keep.
  const kept = [
    'embeddings',
    'pooling',
    'cache-reuse',
    'n-predict',
    'jinja',
    'prefill-assistant',
    'reasoning',
    'reasoning-format',
    'chat-template-kwargs',
    'temp',
    'top-k',
    'top-p',
    'min-p',
    'repeat-penalty',
    'presence-penalty',
    'backend-sampling',
    'image-min-tokens',
    'image-max-tokens',
    'spec-draft-n-max',
    'spec-draft-n-min',
    'spec-draft-p-min',
    'spec-ngram-mod-n-match',
    'spec-ngram-mod-n-min',
    'spec-ngram-mod-n-max',
  ]
  for (const key of kept)
    expect(
      MODEL_PROFILE_STAY_KEYS.includes(key),
      `${key}: a key of checks::MODEL_PROFILE_STAY_KEYS, the list the page filters by`,
    ).toBe(true)
  const missing = kept.filter(
    (key) => document.getElementById(`${fieldId(key)}-control`) === null,
  )
  expect(
    missing,
    'IP5.5: each of these keys has a control on the page, in the card IP5.5 names',
  ).toEqual([])
  expect(
    main.textContent,
    'IP5.5: the KV cache card keeps cache-reuse alone and says where the rest went',
  ).toContain(en('model-profile-kv-cache-reuse-intro'))
})

test('IP5.5: the page reads no model file facts and no GPU list: they belong to the cards that moved', async () => {
  const { bff } = await page()
  const read = bff.calls.map((c) => c.path)
  expect(
    read.filter((p) => p.endsWith('/model-info')),
    'IP5.5: the page shows no layer facts and no over-memory reading, so it asks for none',
  ).toEqual([])
  expect(
    read.filter((p) => p === '/api/node/devices'),
    'IP5.5: the page shows no GPU list, so it never runs the probe',
  ).toEqual([])
})

// ── IP5.5: the note under the head ──────────────────────────────────────

test('IP5.5: the note under the head says where the placement lives, and links to the first instance profile that lists the model', async () => {
  const { main } = await page({}, ['default', 'unassigned'])
  const note = [...main.querySelectorAll('p.intro')].find((p) =>
    (p.textContent ?? '').includes(en('model-profile-placement-elsewhere')),
  )
  expect(
    note,
    'IP5.5: one p.intro after the Used by row, the words of model-profile-placement-elsewhere',
  ).not.toBeUndefined()
  expect(
    links(note!.parentElement!),
    'IP5.5: the first instance profile’s entry page follows as a link named by the key, with the id in it',
  ).toContainEqual({
    text: en('model-profile-placement-link', 'default'),
    href: `#/profiles/instance/default/models/${MODEL_PROFILE_ID}`,
  })
})

test('IP5.5: the note stands with no link when no instance profile lists the model', async () => {
  const { main } = await page()
  expect(
    main.textContent,
    'IP5.5: the note is the page’s own, so it shows whatever lists the model',
  ).toContain(en('model-profile-placement-elsewhere'))
  expect(
    links(main).map((l) => l.href ?? ''),
    'IP5.5: with usedBy empty there is no placement link',
  ).not.toContain(`#/profiles/instance/default/models/${MODEL_PROFILE_ID}`)
})

// ── IP5.6: the Save bar ─────────────────────────────────────────────────

test('IP5.6: the page’s Delete can act on a model no instance profile lists', async () => {
  const { main } = await page()
  const del = within(main).getByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    del.getAttribute('aria-disabled'),
    'IP5.6: an unused model profile is deletable from its page',
  ).not.toBe('true')
})

test('IP5.6: the page’s Delete cannot act while an instance profile lists the model, and says who', async () => {
  const { main } = await page({}, ['default'])
  const del = within(main).getByRole('button', {
    name: en('model-profile-delete'),
  })
  expect(
    del.getAttribute('aria-disabled'),
    'IP5.6: a listed model profile is not deletable: the instance profile that lists it is its user',
  ).toBe('true')
  expect(
    del.getAttribute('aria-describedby'),
    'IP5.6: the Delete describes itself with its reason (22 L2.B2 b)',
  ).toBe('model-profile-delete-reason')
  expect(
    document.getElementById('model-profile-delete-reason')?.textContent,
    'IP5.6: the reason names the user, not a switch',
  ).toBe(en('model-profiles-delete-in-use'))
})

test('IP5.6: a save answering running instance ids names them and offers one Restart each', async () => {
  const { bff, main } = await page({
    modelProfileSave: () => reply(200, { revision: 'r2', running: ['coder'] }),
  })
  const pooling = within(main).getByRole('combobox', {
    name: en('model-profile-pooling'),
  })
  fireEvent.change(pooling, { target: { value: 'rank' } })
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await waitFor(() =>
    expect(
      (barStatus(main).textContent ?? '').trim(),
      'IP5.6: the bar names the running instances of the reply',
    ).toBe(en('save-saved-restart-ids', instanceName(INSTANCES[1]!))),
  )
  expect(
    within(main)
      .queryAllByRole('button', { name: /^Restart / })
      .map((b) => (b.textContent ?? '').trim()),
    'IP5.6: one Restart per running instance, named by its name',
  ).toEqual([en('restart-offer-button', instanceName(INSTANCES[1]!))])
  expect(
    savedBody(bff).values,
    'IP5.6: the save carries the edit and the revision',
  ).toMatchObject({ pooling: 'rank' })
})

// ── IP5.7: the refusals ─────────────────────────────────────────────────

test('IP5.7: the form sends no key that moved, so the save of an edit is accepted', async () => {
  const { bff, main } = await page()
  // The save of an unchanged form sends nothing, so the form is made dirty
  // with a stay-key edit first.
  const pooling = within(main).getByRole('combobox', {
    name: en('model-profile-pooling'),
  })
  fireEvent.change(pooling, { target: { value: 'rank' } })
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await waitFor(() => expect(savedBody(bff)).toBeTruthy())
  const values = (savedBody(bff).values ?? {}) as Record<string, unknown>
  const moved = PLACEMENT_KEYS.filter((key) => Object.hasOwn(values, key))
  expect(
    moved,
    'IP5.7: the page owns no placement key any more, so its body carries none and the bff’s 422 model-profile-placement-key never shows',
  ).toEqual([])
})

test('IP5.7: a 422 model-profile-placement-key shows in the bar’s alert, naming the key', async () => {
  const { main } = await page({
    modelProfileSave: () =>
      reply(422, {
        key: 'model-profile-placement-key',
        args: ['ctx-size'],
        field: 'ctx-size',
      }),
  })
  const pooling = within(main).getByRole('combobox', {
    name: en('model-profile-pooling'),
  })
  fireEvent.change(pooling, { target: { value: 'rank' } })
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await waitFor(() =>
    expect(
      barAlerts(main),
      'IP5.7: the bar’s alert is the strings sentence, with the key the bff named',
    ).toContain(en('api-model-profile-placement-key', 'ctx-size')),
  )
})

test('IP5.4: the page says nothing of a profile being on or off', async () => {
  const { main } = await page()
  expect(
    main.querySelectorAll('.model-profiles-switch').length,
    'IP5.4: the page’s own switch is gone: no element of the switch class',
  ).toBe(0)
  expect(
    within(main).queryAllByRole('switch').length,
    'IP5.4: no role=switch anywhere on the page',
  ).toBe(0)
  expect(
    main.querySelectorAll('.model-profiles-switch-status').length,
    'IP5.4: the switch’s status line is gone with it',
  ).toBe(0)
  expect(
    main.querySelector('#model-profile-switch-dirty'),
    'IP5.4: the line the switch describes itself with is gone with it',
  ).toBeNull()
  // The words of the state line, pinned as the English they are today: the
  // three keys of 24 A2.1 and 23 A2.3 are IP5.8's to delete, so a test cannot
  // name them by key.
  const text = main.textContent ?? ''
  for (const gone of [
    'Switched on: its engine serves it.',
    'Switched off: kept, but not served.',
    'Save or discard your changes first.',
  ])
    expect(
      text,
      `IP5.4: the state line’s words leave with its switch: ${gone}`,
    ).not.toContain(gone)
})

test('IP5.5: every jump target of the list is a card the page draws', async () => {
  const { main } = await page()
  const targets = jumpTitles(main).map(() => null)
  expect(targets, 'the jump list is the eight, pinned above').toHaveLength(8)
  for (const id of [
    'assets',
    'embeddings',
    'kv-cache',
    'chat-reasoning',
    'sampling-overrides',
    'advanced',
    'vision',
    'speculative-decoding',
  ]) {
    const heading = document.getElementById(cardHeadingId(id))
    expect(
      heading,
      `IP5.5: the jump reaches the ${id} card’s heading, which is drawn`,
    ).not.toBeNull()
    expect(main.contains(heading!), `the ${id} heading is on the page`).toBe(
      true,
    )
  }
})
