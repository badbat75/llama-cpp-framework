// Brief 16, slice 17f (fe/briefs/16-server-profiles-and-instances.md): the
// instance's NAME, what every place shows and says for it, and the texts of the
// slice. The whole app renders against a fake bff (mocked fetch); "16 17f.n" is
// the brief's criterion n. The display name form is in
// InstanceNameEdit17f2.test.tsx (17f2 replaces 17f's Rename button) and
// the "not confirmed" rule of every write in WriteOutcome17f.test.tsx; the
// `.instance-id` rule of 17f.8 is in Reflow.test.ts, beside the other CSS
// readers, and the nav's keyboard walk in NodesMenu1.test.tsx.
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  en,
  fakeBff,
  instanceName,
  reply,
  serverProfileDoc,
  shownText,
  startAt,
  type BffOptions,
  type Instance,
} from './test/bff.ts'

/** The three shapes of a name, in the order the list answers them: a label, an
 * unlabelled instance with a port (named by its engine and port), and one with
 * no port (named by its id). 16 17f.1. */
const NAMED: Instance[] = [
  {
    id: 'default',
    engine: 'llama.cpp',
    state: 'active',
    port: 9931,
    label: 'Coding assistant',
  },
  { id: 'coder', engine: 'llama.cpp', state: 'inactive', port: 8580 },
  { id: 'embed', engine: 'strata', state: 'failed', port: 0 },
]

/** The name of one fixture item, as 17f.1 computes it. */
const nameOf = (item: Instance): string => instanceName(item)

/** Answers the list from a function, so a test can answer any shape of `label`. */
function itemsAnswer(items: () => unknown[]): BffOptions['instances'] {
  return () => reply(200, JSON.stringify(items()))
}

/** The tiles list of the Overview, once it is rendered: the one list that is
 * not inside a tile (17b.1 puts a chips list inside every tile). */
async function tilesList(main: HTMLElement): Promise<HTMLElement> {
  const lists = (
    await within(main).findAllByRole('list', { hidden: true })
  ).filter((list) => list.closest('.tile') === null)
  expect(lists.length, 'one list of tiles').toBe(1)
  return lists[0]!
}

/** The app at `hash`, loaded to the point the page shows its content. */
async function open(hash: string, options: BffOptions = {}) {
  startAt(hash)
  const bff = fakeBff({ instances: NAMED, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  return { bff, main }
}

beforeEach(() => {
  startAt('#/')
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── 17f.1: the name ───────────────────────────────────────────────────────

test('16 17f.1: the name of an item is its label when it is a non-empty string, instance-name-auto with the engine and the port when the port is above 0, and the id when there is neither', async () => {
  const { main } = await open('#/')
  await screen.findByRole('heading', { level: 2, name: en('instances-title') })
  const links = within(await tilesList(main)).getAllByRole('link')
  expect(
    links.map((a) => shownText(a)),
    'the visible text of each tile link is the name of its item, in the list ' +
      'order (17f.3 adds a reader run inside the link, left out here)',
  ).toEqual(NAMED.map(nameOf))
  expect(nameOf(NAMED[0]!), 'a label is shown as the bff gives it').toBe(
    'Coding assistant',
  )
  expect(
    nameOf(NAMED[1]!),
    'no label and a port: the engine and the port, spelled by instance-name-auto',
  ).toBe(en('instance-name-auto', 'llama.cpp', '8580'))
  expect(nameOf(NAMED[2]!), 'no label and no port: the id').toBe('embed')
})

test('16 17f.1: the reader takes a label that is missing, null, the empty string, a number or a string of spaces as no label, and a label of one space as a label', async () => {
  const shapes = [
    { id: 'a', engine: 'llama.cpp', state: 'active', port: 9931 },
    { id: 'b', engine: 'llama.cpp', state: 'active', port: 9931, label: null },
    { id: 'c', engine: 'llama.cpp', state: 'active', port: 9931, label: '' },
    { id: 'd', engine: 'llama.cpp', state: 'active', port: 9931, label: 7 },
    { id: 'e', engine: 'llama.cpp', state: 'active', port: 9931, label: ' ' },
  ]
  const { main } = await open('#/', { instances: itemsAnswer(() => shapes) })
  await screen.findByRole('heading', { level: 2, name: en('instances-title') })
  const links = within(await tilesList(main)).getAllByRole('link')
  const auto = en('instance-name-auto', 'llama.cpp', '9931')
  expect(
    links.slice(0, 4).map((a) => shownText(a)),
    'missing, null, "" and a number are no label: the engine and the port',
  ).toEqual([auto, auto, auto, auto])
  const space = links[4]!
  expect(
    space.firstChild?.nodeType,
    'a label of one space IS a label: the link opens with the label text',
  ).toBe(Node.TEXT_NODE)
  expect(
    space.firstChild?.textContent,
    'the label is taken as the bff gives it, spaces kept',
  ).toBe(' ')
  expect(shownText(space), 'and not the engine and the port').not.toBe(auto)
})

// ── 17f.2: the nav ────────────────────────────────────────────────────────

test('16 17f.2: the nav link of an instance is the box icon then a span.nav-instance holding the name alone, heard as the name, with no server profile and no id in it', async () => {
  await open('#/')
  const nav = await screen.findByRole('navigation')
  const named = await within(nav).findByRole('link', {
    name: 'Coding assistant',
  })
  expect(named.getAttribute('href'), 'the link is the instance page').toBe(
    '#/instances/default',
  )
  const span = named.querySelector('.nav-instance')
  expect(span, 'the name is in a span.nav-instance').not.toBeNull()
  expect(span!.textContent, 'the span holds the name and nothing else').toBe(
    'Coding assistant',
  )
  expect(
    named.querySelectorAll('.nav-instance [aria-hidden="true"]').length,
    '17f.2: no decorative run beside the name',
  ).toBe(0)
  expect(
    named.querySelectorAll('.nav-instance .visually-hidden').length,
    '17f.2: the link says nothing beside the name',
  ).toBe(0)
  expect(
    named.textContent,
    'the id is not in the entry, and neither is the server profile',
  ).toBe('Coding assistant')
  expect(
    within(nav).getByRole('link', { name: nameOf(NAMED[1]!) }).textContent,
    'an unlabelled instance is named by its engine and port',
  ).toBe(nameOf(NAMED[1]!))
})

test('16 17f.2: two unlabelled instances on one port show the same entry, told apart by their order and their page, and the menu adds no id', async () => {
  const pair = [
    { id: 'one', engine: 'llama.cpp', state: 'active', port: 9931 },
    { id: 'two', engine: 'llama.cpp', state: 'active', port: 9931 },
  ]
  await open('#/', { instances: itemsAnswer(() => pair) })
  const nav = await screen.findByRole('navigation')
  const links = await within(nav).findAllByRole('link', {
    name: en('instance-name-auto', 'llama.cpp', '9931'),
  })
  expect(links.length, 'both entries read the same').toBe(2)
  expect(
    links.map((a) => a.getAttribute('href')),
    'each leads to its own page, where the id shows (17f.4)',
  ).toEqual(['#/instances/one', '#/instances/two'])
})

// ── 17f.3: the tiles ──────────────────────────────────────────────────────

test('16 17f.3: the tile link reads the name and is heard as the name with instance-id-said, and a p.instance-id under the tile head reads instance-id-shown with the id', async () => {
  const { main } = await open('#/')
  await screen.findByRole('heading', { level: 2, name: en('instances-title') })
  const items = within(await tilesList(main)).getAllByRole('listitem')
  expect(items.length, 'one tile per item').toBe(NAMED.length)
  items.forEach((tile, i) => {
    const item = NAMED[i]!
    const name = nameOf(item)
    // The id the reader says, only where the name is not the id: 17f.1 leaves
    // it out when it would say the same twice, so the tile of `embed` is heard
    // as `embed` alone (the next test pins that).
    const link = within(tile).getByRole('link', {
      name:
        name === item.id ? name : `${name}${en('instance-id-said', item.id)}`,
    })
    expect(link.getAttribute('href'), 'the link is the instance page').toBe(
      `#/instances/${item.id}`,
    )
    expect(link.classList.contains('tile-id'), 'the link is a.tile-id').toBe(
      true,
    )
    const head = tile.querySelector('.tile-head')
    expect(head, 'the tile has a head').not.toBeNull()
    const id = tile.querySelector('p.instance-id')
    if (name === item.id) {
      expect(id, 'the name IS the id: no id line').toBeNull()
      return
    }
    expect(id, 'the id shows under the tile head').not.toBeNull()
    expect(id!.textContent, 'the id line is instance-id-shown').toBe(
      en('instance-id-shown', item.id),
    )
    expect(
      before(head!, id!),
      'the id line is under the tile head, not above it',
    ).toBe(true)
  })
})

test('16 17f.3: the tile of an instance named by its id has no p.instance-id, and its link is heard as the id alone', async () => {
  const { main } = await open('#/')
  await screen.findByRole('heading', { level: 2, name: en('instances-title') })
  const tile = within(await tilesList(main)).getAllByRole('listitem')[2]!
  expect(
    tile.querySelector('.instance-id'),
    'the id would say the name twice: no id line',
  ).toBeNull()
  expect(
    within(tile).getByRole('link', { name: 'embed' }).textContent,
    'the link reads the id',
  ).toBe('embed')
})

// ── 17f.4: the instance page ──────────────────────────────────────────────

// 17f2.2: the page's own id line reads `instance-id-fixed` ("ID {0}, does not
// change"); the tiles and the Used by link keep `instance-id-shown`.
test('16 17f.4: the instance page h1 reads the name, a p.instance-id right after it reads instance-id-fixed, the title is the name with the app name, and the route stays #/instances/<id>', async () => {
  const { main } = await open('#/instances/default')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'Coding assistant',
  })
  expect(h1.getAttribute('id'), 'the heading keeps its id').toBe('page-heading')
  // 16 17f2.1: the heading sits in the title row, with the pencil after it, so
  // the id line of 17f2.2 is the row's own next sibling.
  const id = h1.parentElement?.nextElementSibling
  expect(id?.tagName, 'a paragraph right after the title row').toBe('P')
  expect(id?.classList.contains('instance-id'), 'it is a p.instance-id').toBe(
    true,
  )
  expect(id?.textContent, 'it reads instance-id-fixed with the id').toBe(
    en('instance-id-fixed', 'default'),
  )
  // The title is set by the heading's own effect, which React runs after the
  // commit that the finder saw: poll, so a fast reply cannot be read as the
  // title lagging behind the name.
  await waitFor(() =>
    expect(document.title, 'the title is the name with the app name').toBe(
      en('page-title', 'Coding assistant'),
    ),
  )
  expect(window.location.hash, 'the route is the id, not the name').toBe(
    '#/instances/default',
  )
})

test('16 17f.4: the page of an instance with no label heads with the engine and the port and shows the id, and the page of one with no port heads with the id alone and, by 17f2.2, shows the id line too', async () => {
  const { main } = await open('#/instances/coder')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('instance-name-auto', 'llama.cpp', '8580'),
  })
  expect(
    h1.parentElement?.nextElementSibling?.textContent,
    'the id shows beside a name that is not the id (17f2.2: under the title row)',
  ).toBe(en('instance-id-fixed', 'coder'))
  cleanup()
  const second = await open('#/instances/embed')
  const other = await within(second.main).findByRole('heading', {
    level: 1,
    name: 'embed',
  })
  // The heading of a page whose list read has not landed is the route id, so
  // the finder resolves before the instance is known: wait for the line.
  await within(second.main).findByText(en('instance-id-fixed', 'embed'))
  expect(
    second.main.querySelector('.instance-id')?.textContent,
    '17f2.2: the name IS the id, and the page says so, so the reader always sees which word is the key',
  ).toBe(en('instance-id-fixed', 'embed'))
  expect(
    other.parentElement?.nextElementSibling?.classList.contains('instance-id'),
    'the id line is right under the title row, as on the other page (17f2.2)',
  ).toBe(true)
})

test('16 17f.4: a message that names an instance keeps the id: the page of an id the manager does not have reads instance-not-found with the id', async () => {
  const { main } = await open('#/instances/nosuch', {
    instances: [],
  })
  const text = await within(main).findByText(en('instance-not-found', 'nosuch'))
  expect(text.textContent, 'the id, not a name').toContain('nosuch')
})

/** The Used by link of 17f.4, as it must read: the name alone. The id follows
 *  the link as plain text (22 D4.34 replaces the 17f review's D1, which set it
 *  inside the link after a space): what a reader hears as the link is the
 *  instance's name, and the id is still there to tell two named alike apart. */
const USED_BY = 'Coding assistant'
const USED_BY_ID = en('instance-id-shown', 'default')

test('16 17f.4 (as 22 D4.34 amends the 17f review D1): the Server profiles list links its Used by cell with the name alone, and the id in a span.instance-id right after the link, outside it', async () => {
  const pool = [
    {
      id: 'qwen-27b',
      engine: 'llama.cpp',
      port: 9931,
      tags: [],
      instance: 'default',
    },
  ]
  const { main } = await open('#/profiles/server', {
    serverProfiles: () => reply(200, pool),
  })
  const link = await within(main).findByRole('link', { name: USED_BY })
  expect(link.getAttribute('href'), 'the link is the instance page').toBe(
    '#/instances/default',
  )
  expect(
    link.textContent,
    '22 D4.34: the link reads the name alone, so 2.4.4 is met',
  ).toBe(USED_BY)
  const span = link.nextElementSibling
  expect(span, 'the id follows the link, as its sibling').not.toBeNull()
  expect(
    span!.classList.contains('instance-id'),
    'the id is in a span.instance-id',
  ).toBe(true)
  expect(
    span!.closest('a'),
    '22 D4.34: it is outside the link, plain text on the same line',
  ).toBeNull()
  expect(span!.textContent).toBe(USED_BY_ID)
})

test('16 17f.4 (as 22 D4.34 amends the 17f review D1): the profile page links its Used by row with the name alone, and the id in a span.instance-id right after the link, outside it', async () => {
  const { main } = await open('#/profiles/server/qwen-27b', {
    serverProfile: () => reply(200, serverProfileDoc()),
  })
  const link = await within(main).findByRole('link', { name: USED_BY })
  expect(link.getAttribute('href')).toBe('#/instances/default')
  expect(
    link.textContent,
    '22 D4.34: the accessible name a reader hears is the instance name, not the name with the id glued to it',
  ).toBe(USED_BY)
  const span = link.nextElementSibling
  expect(span, 'the id follows the link, as its sibling').not.toBeNull()
  expect(span!.closest('a'), 'outside the link').toBeNull()
  expect(span!.textContent, 'the id is in a span.instance-id').toBe(USED_BY_ID)
})

// ── 17f.11: the texts ─────────────────────────────────────────────────────

/** 17f.11: the keys the strings file gains, with the English of section 2.
 *  17f2.10 takes the seven of the Rename form out of the list (`instance-rename`,
 *  `instance-label`, `instance-label-hint`, `instance-label-save`,
 *  `instance-label-saving`, `instance-label-saved`, `instance-label-cleared`):
 *  the form is gone, and its keys are pinned in InstanceNameEdit17f2.test.tsx.
 *  The two refusal texts stay in the list, in the English 17f2.7 gives them. */
const GAINS = [
  'instance-name-auto',
  'instance-id-shown',
  'instance-id-said',
  'instance-label-cancel',
  'api-instance-bad-label',
  'api-instance-label-taken',
  'write-timeout',
  'delete-unknown',
]

test('16 17f.11 (as 17f2.10 trims it): en_US.json gains the keys of section 2 that 17f2 keeps, each with the English of the brief', () => {
  const table: Record<string, string> = strings
  const missing = GAINS.filter((key) => table[key] === undefined)
  expect(missing, 'keys missing from strings/en_US.json').toEqual([])
  const wrong = GAINS.filter((key) => table[key] !== en(key))
  expect(
    wrong,
    'keys whose en_US.json text is not the English of the brief',
  ).toEqual([])
})

test('16 17f.11: en_US.json loses nav-instance-profile and nav-instance-profile-shown, the two keys 17f.2 retires', () => {
  const table: Record<string, string> = strings
  expect(
    table['nav-instance-profile'],
    'the nav entry is the name only: the reader phrase is gone',
  ).toBeUndefined()
  expect(
    table['nav-instance-profile-shown'],
    'the nav entry is the name only: the shown profile is gone',
  ).toBeUndefined()
})
