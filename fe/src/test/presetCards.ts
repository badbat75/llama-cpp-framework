// The preset page's cards in the page order of criterion 83
// (fe/briefs/02e-preset-cards.md): each card's h3 strings key, then its
// fields' labels in order. Every whole-page assertion of the preset page's
// tests (the h3 list, the About toggles, the tab path, the heading levels)
// derives from this one list: a sub-slice that adds cards adds them here, at
// the place 83 gives them, and nowhere else.
import { en } from './bff.ts'

/** A field's label: its strings key, or `shown(...)` where the brief gives
 * the label as literal text and not a string (Top-K, Top-P, Min-P of 97). */
export type Label = string | { shown: string }

/** A label that is literal text, not a string. */
export function shown(text: string): Label {
  return { shown: text }
}

/** The label as the page shows it. */
export function labelText(label: Label): string {
  return typeof label === 'string' ? en(label) : label.shown
}

/** One card: the strings key of its h3, and its fields in the card's order.
 * A card with no field of its own (Assets: its values are a description list,
 * 73) has no About toggle and adds no tab stop. */
export type Card = { heading: string; fields: Label[] }

export const PRESET_CARDS: Card[] = [
  { heading: 'preset-assets', fields: [] },
  {
    heading: 'preset-hardware-config',
    fields: [
      'preset-gpu-layers',
      'preset-moe-cpu-layers',
      'preset-moe-cache',
      'preset-dense-ffn-cpu-layers',
      'preset-op-offload',
      'preset-repack',
      'preset-no-host-buffer',
      'preset-lazy-tensors',
      'preset-load-mode',
      'preset-fit-to-vram',
      'preset-fit-min-context',
      'preset-fit-free-vram',
      'preset-draft-device',
      'preset-draft-gpu-layers',
    ],
  },
  {
    heading: 'preset-resource-context',
    fields: [
      'preset-context-size',
      'preset-parallel-slots',
      'preset-unified-kv',
      'preset-per-slot-context',
      'preset-batch-size',
      'preset-u-batch-size',
    ],
  },
  {
    heading: 'preset-embeddings',
    fields: ['preset-embedding-mode', 'preset-pooling'],
  },
  {
    heading: 'preset-kv-cache',
    fields: [
      'preset-k-cache-type',
      'preset-v-cache-type',
      'preset-flash-attention',
      'preset-cache-ram',
      'preset-cache-reuse',
      'preset-context-checkpoints',
      'preset-checkpoint-step',
    ],
  },
  {
    heading: 'preset-chat-reasoning',
    fields: [
      'preset-chat-template',
      'preset-prefill-assistant',
      'preset-reasoning',
      'preset-reasoning-format',
      'preset-reasoning-effort',
      'preset-keep-past-thinking',
      'preset-thinking-budget',
      'preset-budget-message',
      'preset-max-output',
    ],
  },
  {
    heading: 'preset-sampling-overrides',
    fields: [
      'preset-temperature',
      shown('Top-K'),
      shown('Top-P'),
      shown('Min-P'),
      'preset-repeat-penalty',
      'preset-presence-penalty',
      'preset-backend-sampling',
    ],
  },
  {
    heading: 'preset-advanced',
    fields: ['preset-load-on-startup', 'preset-chat-kwargs'],
  },
  {
    heading: 'preset-vision',
    fields: [
      'preset-vision-on-gpu',
      'preset-vision-gpu',
      'preset-min-image-tokens',
      'preset-max-image-tokens',
    ],
  },
  {
    heading: 'preset-speculative-decoding',
    fields: [
      'preset-draft-n-max',
      'preset-draft-p-min',
      'preset-draft-n-min',
      'preset-draft-n-ubatch',
      'preset-draft-k-cache-type',
      'preset-draft-v-cache-type',
      'preset-match-length',
      'preset-min-draft',
      'preset-max-draft',
      'preset-lookup-n-gram',
      'preset-draft-m-gram',
      'preset-lookup-n-gram',
      'preset-draft-m-gram',
      'preset-lookup-n-gram',
      'preset-draft-m-gram',
      'preset-min-hits',
    ],
  },
]

/** The text of every card's h3, in page order (83). */
export function cardHeadings(): string[] {
  return PRESET_CARDS.map((card) => en(card.heading))
}

/** The card headings as strings keys, for `cardOf`. */
export function cardHeadingKeys(): string[] {
  return PRESET_CARDS.map((card) => card.heading)
}

/** The integer keys that get NO input mode (2h.158): the any-whole-number
 * keys of 157's first row and the layer counts of its last row. A phone's
 * numeric keyboard may have no minus key (MDN, `inputmode`), so the fe adds
 * none: every other integer key keeps `inputmode="numeric"`. */
export const NO_INPUT_MODE_KEYS: string[] = [
  'cache-ram',
  'top-k',
  'n-predict',
  'reasoning-budget',
  'n-gpu-layers',
  'n-cpu-moe',
  'n-cpu-ffn',
  'n-gpu-layers-draft',
]

/** The input mode a number field takes (87 as superseded by 2h.158):
 * `numeric` for an integer key with a range starting at 0 or above, none
 * for a float key and for the keys of NO_INPUT_MODE_KEYS. */
export function expectedInputMode(
  key: string,
  integer?: boolean,
): string | null {
  return integer && !NO_INPUT_MODE_KEYS.includes(key) ? 'numeric' : null
}

/** Every field's label, in card and field order (92, 93, 96, 97, 98). */
export function fieldLabels(): string[] {
  return PRESET_CARDS.flatMap((card) => card.fields.map(labelText))
}

/** The About toggle name of every field, in the tab order of 91. */
export function fieldAboutToggles(): string[] {
  return fieldLabels().map((label) => `${en('field-about')} ${label}`)
}

/** The fields the preset page makes editable: 2h.1's 130 (the Embeddings
 * card's two controls), 2h.4's 155 (the Resource / context card's six) and
 * 2h.5's 164 (the KV cache card's seven, all always enabled), by their label
 * key. Every other field's control stays disabled, so it takes no tab stop. */
const EDITABLE_FIELDS = [
  'preset-context-size',
  'preset-parallel-slots',
  'preset-unified-kv',
  'preset-per-slot-context',
  'preset-batch-size',
  'preset-u-batch-size',
  'preset-embedding-mode',
  'preset-pooling',
  'preset-k-cache-type',
  'preset-v-cache-type',
  'preset-flash-attention',
  'preset-cache-ram',
  'preset-cache-reuse',
  'preset-context-checkpoints',
  'preset-checkpoint-step',
]

/** The editable fields whose control sits behind a default box (2h.156): the
 * label key mapped to the key in the read's `values`. A key the file lacks
 * starts its box ticked, and a ticked box disables and empties the input. */
const BOXED_FIELDS: Record<string, string> = {
  'preset-context-size': 'ctx-size',
  'preset-parallel-slots': 'parallel',
  'preset-per-slot-context': 'kv-unified-per-slot',
  'preset-batch-size': 'batch-size',
  'preset-u-batch-size': 'ubatch-size',
  'preset-cache-ram': 'cache-ram',
  'preset-cache-reuse': 'cache-reuse',
  'preset-context-checkpoints': 'ctx-checkpoints',
  'preset-checkpoint-step': 'checkpoint-min-step',
}

/** The editable fields with an enable rule (2h.155): Unified KV is enabled
 * while Parallel slots' box is unticked, which is how a key the file holds
 * starts. */
const ENABLE_RULE: Record<string, string> = { 'preset-unified-kv': 'parallel' }

/** True when 2h.1's 130 or 2h.4's 155 makes the field with this label key
 * editable: its default box is enabled, whatever the file holds. */
export function isEditableField(labelKey: string): boolean {
  return EDITABLE_FIELDS.includes(labelKey)
}

/** True when the control of the field with this label key is enabled on a
 * read that holds `values`: a boxed field's input follows its box (156), and
 * a field with an enable rule follows that rule (155). False for a field of a
 * read-only card, so the expectations of 85 stay pinned for every other card. */
export function controlEnabled(
  labelKey: string,
  values: Record<string, string>,
): boolean {
  if (!isEditableField(labelKey)) return false
  const boxed = BOXED_FIELDS[labelKey]
  if (boxed !== undefined) return boxed in values
  const rule = ENABLE_RULE[labelKey]
  if (rule !== undefined) return rule in values
  return true
}

/** Every field's tab stops in the order of 91: each field's About toggle,
 * then the controls 2h.1, 2h.4 and 2h.5 made editable and left enabled on this
 * read (141, 155, 156): a boxed field's default box, named by the box's own
 * text, and its input while the box is unticked. The whole-page tab path of
 * every preset page test is this list, read with the values of its fixture. */
export function fieldTabStops(values: Record<string, string>): string[] {
  const stops: string[] = []
  for (const card of PRESET_CARDS)
    for (const field of card.fields) {
      const label = labelText(field)
      stops.push(`${en('field-about')} ${label}`)
      if (typeof field !== 'string' || !isEditableField(field)) continue
      if (BOXED_FIELDS[field] !== undefined) stops.push(en('field-default'))
      if (controlEnabled(field, values)) stops.push(label)
    }
  return stops
}

/** The save bar's two buttons, at the end of the page (2h.1's 141). */
export function saveBarTabStops(): string[] {
  return [en('save-discard'), en('save-button')]
}
