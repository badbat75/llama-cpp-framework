// Brief 22, slice L2 (fe/briefs/22-model-sources.md, "L2 acceptance list for
// the tester" items 6 and 7): 22 D4.31, the two cards' intro kept short with
// the rest behind the card's About, and 22 D4.32, the page head, whose
// settings heading stays in the accessibility tree as `.visually-hidden`.
//
// The design-review round of 2026-11-27 (fe/briefs/22-model-sources.md, D1 and
// its advice 2) rewrites the item 6 tests: the intro key shrinks to the card's
// first SENTENCE (its first line was 594 characters, so nothing had got
// shorter), the rest of the old string moves to the new `-about` keys, and the
// card's About toggle moves beside the card's h3, in the heading row. The
// words of D1 are pinned as the review writes them, and read from
// `strings/en_US.json`: a pin that reads both sides of the strings mirror
// (test/bff.ts's EN) is a tautology, the coder moving one file keeping it
// green.
//
// The same round's second review (fe/briefs/22-model-sources.md, "D3, advice 2
// built otherwise") is pinned at the end of the advice 2 tests: advice 2 built
// the card's About as a bare verb at the row's end, and the review asks for the
// field About's look, 8 px after the title. It is a sheet rule, so it is pinned
// as the declarations the review takes away, read off the toggle and off a
// field's About of the same card. The measure itself (the toggle's left at the
// title's right plus 8 px) is the UX check's, jsdom laying nothing out.
//
// Not provable in jsdom, so not tested here: the two gaps of D4.32 (the
// switch and its word `--space-2` apart, and the `--space-5` the state line
// leaves before the first card), which are the designer's measurement of the
// UX check's model-profile-page states. The card's own fields, and what the
// page saves, are ModelProfileGpu*.test.tsx and ModelProfileSave*.test.tsx.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import { en, fakeBff, startAt, type BffOptions } from './test/bff.ts'

const MODEL = '#/profiles/model/Qwen3-27B'
const GPU = en('model-profile-gpu-distribution')
const TENSOR = en('model-profile-tensor-placement')
// The strings file itself, not the test mirror: the words of D1 are pinned
// against what the page reads.
const S = JSON.parse(
  readFileSync(join(import.meta.dirname, 'strings', 'en_US.json'), 'utf8'),
) as Record<string, string>

/** One card of D4.31: its title, its two keys, and the words the review gives
 *  (fe/briefs/22-model-sources.md, D1: the intro is the first sentence, the
 *  about key is the rest of the old string, `\n` breaks included). */
const INTRO_CARDS = [
  {
    title: GPU,
    introKey: 'model-profile-gpu-distribution-intro',
    aboutKey: 'model-profile-gpu-distribution-about',
    intro:
      'Which GPUs run THIS model and how much each holds, saved to models.ini.',
    about:
      "Check nothing to let llama.cpp choose. With two GPUs of different sizes, check both and give the bigger one more; left on Auto, the split follows free VRAM at load, not an even cut. Under split mode layer, once the model file is read, each GPU takes a number of blocks: the model's blocks and its output layer, shared out top to bottom. Otherwise the weights are ratios: 3 and 1 give 75% and 25%. The checked GPUs, top to bottom, are the split order: Move up and Move down change it; the first one is llama.cpp's main GPU.\nKeys: split-mode, device, tensor-split.",
  },
  {
    title: TENSOR,
    introKey: 'model-profile-tensor-placement-intro',
    aboutKey: 'model-profile-tensor-placement-about',
    intro:
      'Exceptions to the GPU distribution above: tensors that go on a device of their own, whatever their layer does.',
    about:
      'On a MoE model, sending the experts of some layers to the CPU frees the most VRAM, as MoE CPU layers does for the first ones. llama.cpp also keeps the embedding table in RAM even when it reports every layer on the GPU: put it on a GPU to reclaim that memory, if the card has room. The first rule that matches a tensor wins.\nKey: override-tensor.',
  },
]

/** The card whose heading is `title`. */
const card = (main: HTMLElement, title: string): HTMLElement =>
  within(main)
    .getByRole('heading', { level: 3, name: title })
    .closest('section')!

// ── The sheet, read as text (Reflow.test.ts's reader, a small local copy:
// importing that file would register its tests here) ───────────────────────

/** The model profile page, its cards read. */
async function cardsPage(options: BffOptions = {}): Promise<HTMLElement> {
  startAt(MODEL)
  fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  // The cards come with the read, so the KV cache card stands for them all.
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-kv-cache'),
  })
  return main
}

beforeEach(() => {
  startAt('#/')
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Item 6: 22 D4.31, as its D1 review (2026-11-27) amends it ─────────────

test('22 L2 D4.31 (its D1 review): the strings file keeps each card’s intro to the one sentence the review gives, and holds the rest of the old string in the new -about key', () => {
  for (const c of INTRO_CARDS) {
    expect(
      S[c.introKey],
      `${c.introKey}: the card’s intro is the review’s sentence, nothing after it`,
    ).toBe(c.intro)
    expect(
      S[c.introKey],
      `${c.introKey}: one sentence, so the card shows it as one line (D4.31: the first line was 594 characters)`,
    ).not.toContain('. ')
    expect(
      S[c.aboutKey],
      `${c.aboutKey}: the rest of the old string, word for word with its \\n breaks, for the card’s About panel`,
    ).toBe(c.about)
  }
})

test('22 L2 D4.31, 22 IP5.5: no card of the page carries a card-level About panel: the two cards D4.31 gave one are not drawn, and the KV cache card shows the one sentence IP5.5 gives it', async () => {
  // Brief 22 IP5.5 takes the GPU distribution and Tensor placement cards off
  // this page (their intro and their About panel move with them, brief 26), and
  // leaves the KV cache card with one intro of its own and no panel. So the
  // card-level About of D4.31 has no subject on this page: what is pinned here
  // is that it left, and that the sentence the page does show is the new key.
  const main = await cardsPage()
  for (const c of INTRO_CARDS)
    expect(
      within(main).queryByRole('heading', { level: 3, name: c.title }),
      `${c.title}: the card is not drawn (22 IP5.5)`,
    ).toBeNull()
  const kv = card(main, en('model-profile-kv-cache'))
  const intros = [...kv.querySelectorAll('.intro')]
  expect(intros.length, 'the KV cache card shows one intro').toBe(1)
  expect(
    (intros[0]!.textContent ?? '').trim(),
    'the intro is the new key of IP5.5',
  ).toBe(en('model-profile-kv-cache-reuse-intro'))
  // A card-level About is a button named by the card's title: none is left, on
  // this card (no panel) or on any card the page draws. A field's About, named
  // by the field's label, is untouched.
  for (const h3 of within(main).getAllByRole('heading', { level: 3 })) {
    const title = (h3.textContent ?? '').trim()
    const box = h3.closest('section')!
    expect(
      within(box).queryByRole('button', {
        name: `${en('field-about')} ${title}`,
      }),
      `${title}: the card carries no card-level About toggle`,
    ).toBeNull()
    // A card-level panel is what that toggle opens, so with no toggle the card
    // has no panel a reader can reach.
    for (const el of box.querySelectorAll('[aria-expanded]')) {
      const name = (
        el.getAttribute('aria-label') ??
        el.textContent ??
        ''
      ).trim()
      expect(
        name,
        `${title}: nothing in this card expands a card-level panel`,
      ).not.toBe(`${en('field-about')} ${title}`)
    }
  }
})

// ── Item 7: 22 D4.32, the page head ───────────────────────────────────────

test('22 L2 D4.32: the model profile page settings heading is visually hidden, and a reader still finds it by role', async () => {
  const main = await cardsPage()
  const title = en('model-profile-settings-title')
  const h2 = within(main).getByRole('heading', { level: 2, name: title })
  expect(
    h2.className,
    'the eye sees no heading over the bar (D4.32)',
  ).toContain('visually-hidden')
  expect(h2.textContent, 'the word is the same key, unchanged').toBe(title)
  // The heading is the settings section's own, where the Save bar sits: the
  // section that holds the bar is the one the heading opens.
  expect(
    within(h2.parentElement!).getByRole('button', {
      name: en('model-profile-delete'),
    }),
    'the heading stands over the bar',
  ).toBeDefined()
  // The cards keep their own headings, which the jump list reaches: D4.32
  // hides the settings heading alone.
  expect(
    within(main)
      .getAllByRole('heading', { level: 3 })
      .some((h) => !h.className.includes('visually-hidden')),
    'the cards keep visible headings',
  ).toBe(true)
})
