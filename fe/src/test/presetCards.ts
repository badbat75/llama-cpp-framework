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

/** Every field's label, in card and field order (92, 93, 96, 97, 98). */
export function fieldLabels(): string[] {
  return PRESET_CARDS.flatMap((card) => card.fields.map(labelText))
}

/** The About toggle name of every field, in the tab order of 91. */
export function fieldAboutToggles(): string[] {
  return fieldLabels().map((label) => `${en('field-about')} ${label}`)
}
