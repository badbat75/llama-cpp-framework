// The model profile page's cards in the page order of criterion 83
// (fe/briefs/02e-model-profile-cards.md): each card's h3 strings key, then its
// fields' labels in order. Every whole-page assertion of the model profile page's
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

/** A field: its label, and for a field whose control sits in a group of 2e's
 * table 112 the key of its row in the read's `values`, plus `boxed` when its
 * control sits behind a default box (the table's `box checked` column). The
 * labels of the n-gram groups repeat (`Lookup n-gram` and `Draft m-gram` stand
 * for three keys each), so the key, not the label, tells which field a table
 * means. */
export type Field = Label | { label: Label; key: string; boxed?: boolean }

/** A keyed field (one of 2e's 112, listed with its row key), as opposed to a
 * plain label, which is a strings key or `shown(...)` of literal text. */
function keyed(
  field: Field,
): field is { label: Label; key: string; boxed?: boolean } {
  return typeof field === 'object' && 'key' in field
}

/** A field's label. */
export function fieldLabel(field: Field): Label {
  return keyed(field) ? field.label : field
}

/** The key of the field's row in the read's `values` (2e's 112). */
function fieldKeyOf(field: Field): string | undefined {
  return keyed(field) ? field.key : undefined
}

/** The key of the field's default box: the row key for a keyed field the table
 * boxes, else the label-keyed entry of `BOXED_FIELDS`. */
function boxedKeyOf(field: Field): string | undefined {
  if (keyed(field) && field.boxed) return field.key
  return BOXED_FIELDS[fieldId(fieldLabel(field))]
}

/** One card: the strings key of its h3, and its fields in the card's order.
 * A card with no field of its own (Assets: its values are a description list,
 * 73) has no About toggle and adds no tab stop. */
export type Card = { heading: string; fields: Field[] }

export const MODEL_PROFILE_CARDS: Card[] = [
  { heading: 'model-profile-assets', fields: [] },
  // Brief 14 (fe/briefs/14-gpu-distribution.md, GD1.1): 02e's 83 reserved this
  // place between Assets and Hardware. The GPUs are no field of 2e's table 112,
  // so the card's one field is the split mode select; the GPU rows, their
  // checkboxes, weights and move buttons depend on the device read and the
  // user's checks, and are pinned in ModelProfileGpu*.test.tsx.
  {
    heading: 'model-profile-gpu-distribution',
    fields: ['model-profile-split-mode'],
  },
  {
    heading: 'model-profile-hardware-config',
    fields: [
      'model-profile-gpu-layers',
      'model-profile-moe-cpu-layers',
      'model-profile-moe-cache',
      'model-profile-dense-ffn-cpu-layers',
      'model-profile-op-offload',
      'model-profile-repack',
      'model-profile-no-host-buffer',
      'model-profile-lazy-tensors',
      'model-profile-load-mode',
      'model-profile-fit-to-vram',
      'model-profile-fit-min-context',
      'model-profile-fit-free-vram',
      'model-profile-draft-device',
      'model-profile-draft-gpu-layers',
    ],
  },
  {
    heading: 'model-profile-resource-context',
    fields: [
      'model-profile-context-size',
      'model-profile-parallel-slots',
      'model-profile-unified-kv',
      'model-profile-per-slot-context',
      'model-profile-batch-size',
      'model-profile-u-batch-size',
    ],
  },
  {
    heading: 'model-profile-embeddings',
    fields: ['model-profile-embedding-mode', 'model-profile-pooling'],
  },
  {
    heading: 'model-profile-kv-cache',
    fields: [
      'model-profile-k-cache-type',
      'model-profile-v-cache-type',
      'model-profile-flash-attention',
      'model-profile-cache-ram',
      'model-profile-cache-reuse',
      'model-profile-context-checkpoints',
      'model-profile-checkpoint-step',
    ],
  },
  {
    heading: 'model-profile-chat-reasoning',
    fields: [
      'model-profile-chat-template',
      'model-profile-prefill-assistant',
      'model-profile-reasoning',
      'model-profile-reasoning-format',
      'model-profile-reasoning-effort',
      'model-profile-keep-past-thinking',
      'model-profile-thinking-budget',
      'model-profile-budget-message',
      'model-profile-max-output',
    ],
  },
  {
    heading: 'model-profile-sampling-overrides',
    fields: [
      'model-profile-temperature',
      shown('Top-K'),
      shown('Top-P'),
      shown('Min-P'),
      'model-profile-repeat-penalty',
      'model-profile-presence-penalty',
      'model-profile-backend-sampling',
    ],
  },
  {
    heading: 'model-profile-advanced',
    fields: ['model-profile-load-on-startup', 'model-profile-chat-kwargs'],
  },
  {
    heading: 'model-profile-vision',
    fields: [
      'model-profile-vision-on-gpu',
      'model-profile-vision-gpu',
      'model-profile-min-image-tokens',
      'model-profile-max-image-tokens',
    ],
  },
  {
    heading: 'model-profile-speculative-decoding',
    fields: [
      {
        label: 'model-profile-draft-n-max',
        key: 'spec-draft-n-max',
        boxed: true,
      },
      {
        label: 'model-profile-draft-p-min',
        key: 'spec-draft-p-min',
        boxed: true,
      },
      {
        label: 'model-profile-draft-n-min',
        key: 'spec-draft-n-min',
        boxed: true,
      },
      {
        label: 'model-profile-draft-n-ubatch',
        key: 'spec-draft-n-ubatch',
        boxed: true,
      },
      { label: 'model-profile-draft-k-cache-type', key: 'spec-draft-type-k' },
      { label: 'model-profile-draft-v-cache-type', key: 'spec-draft-type-v' },
      {
        label: 'model-profile-match-length',
        key: 'spec-ngram-mod-n-match',
        boxed: true,
      },
      {
        label: 'model-profile-min-draft',
        key: 'spec-ngram-mod-n-min',
        boxed: true,
      },
      {
        label: 'model-profile-max-draft',
        key: 'spec-ngram-mod-n-max',
        boxed: true,
      },
      {
        label: 'model-profile-lookup-n-gram',
        key: 'spec-ngram-simple-size-n',
        boxed: true,
      },
      {
        label: 'model-profile-draft-m-gram',
        key: 'spec-ngram-simple-size-m',
        boxed: true,
      },
      {
        label: 'model-profile-lookup-n-gram',
        key: 'spec-ngram-map-k-size-n',
        boxed: true,
      },
      {
        label: 'model-profile-draft-m-gram',
        key: 'spec-ngram-map-k-size-m',
        boxed: true,
      },
      {
        label: 'model-profile-lookup-n-gram',
        key: 'spec-ngram-map-k4v-size-n',
        boxed: true,
      },
      {
        label: 'model-profile-draft-m-gram',
        key: 'spec-ngram-map-k4v-size-m',
        boxed: true,
      },
      {
        label: 'model-profile-min-hits',
        key: 'spec-ngram-map-k4v-min-hits',
        boxed: true,
      },
    ],
  },
]

/** The text of every card's h3, in page order (83). */
export function cardHeadings(): string[] {
  return MODEL_PROFILE_CARDS.map((card) => en(card.heading))
}

/** The card headings as strings keys, for `cardOf`. */
export function cardHeadingKeys(): string[] {
  return MODEL_PROFILE_CARDS.map((card) => card.heading)
}

/** The integer keys that get NO input mode (2h.158): the any-whole-number
 * keys of 157's first row and the layer counts of its last row. A phone's
 * numeric keyboard may have no minus key (MDN, `inputmode`), so the fe adds
 * none: every other integer key keeps `inputmode="numeric"`. Brief 19's
 * 19 G.14 gives `n-cpu-moe` the range 0 to 4096, so it is no longer one of
 * them. */
export const NO_INPUT_MODE_KEYS: string[] = [
  'cache-ram',
  'top-k',
  'n-predict',
  'reasoning-budget',
  'n-gpu-layers',
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
  return MODEL_PROFILE_CARDS.flatMap((card) =>
    card.fields.map((f) => labelText(fieldLabel(f))),
  )
}

/** The About toggle name of every field, in the tab order of 91. */
export function fieldAboutToggles(): string[] {
  return fieldLabels().map((label) => `${en('field-about')} ${label}`)
}

/** The identity a field is listed under: its label's strings key, or the
 * literal text where the brief labels it with literal text (Top-K, Top-P and
 * Min-P of 97). `Label`s compare by it, so a shown field is nameable in the
 * tables below. */
function fieldId(field: Label): string {
  return typeof field === 'string' ? field : field.shown
}

/** The fields the model profile page makes editable: 2h.1's 130 (the Embeddings
 * card's two controls), 2h.4's 155 (the Resource / context card's six),
 * 2h.5's 164 (the KV cache card's seven, all always enabled), 2h.5's 164
 * (the Sampling overrides card's seven, all always enabled) and 2h.5's 164
 * (the Advanced card's two, the off / on select and the text, always enabled),
 * 2h.6's 170 (the Chat / reasoning card's nine, all always enabled),
 * 2h.7's 175 (the Vision card's four, enabled by 176's rule),
 * 2h.7's 175 (the Hardware card's seven always rows, and the draft pair
 * enabled by 177's rule; the two MoE rows stay out),
 * 2h.8's 183, 186 (the Hardware card's Fit trio: Fit to VRAM always enabled,
 * Fit min context and Fit free VRAM enabled while its box is checked),
 * by their label.
 * Every other field's control stays disabled, so it takes no tab stop. */
const EDITABLE_FIELDS: Label[] = [
  // Brief 14 (GD1.2): the GPU distribution card's split mode select, always
  // enabled (decision 6).
  'model-profile-split-mode',
  'model-profile-context-size',
  'model-profile-parallel-slots',
  'model-profile-unified-kv',
  'model-profile-per-slot-context',
  'model-profile-batch-size',
  'model-profile-u-batch-size',
  'model-profile-embedding-mode',
  'model-profile-pooling',
  'model-profile-k-cache-type',
  'model-profile-v-cache-type',
  'model-profile-flash-attention',
  'model-profile-cache-ram',
  'model-profile-cache-reuse',
  'model-profile-context-checkpoints',
  'model-profile-checkpoint-step',
  'model-profile-temperature',
  shown('Top-K'),
  shown('Top-P'),
  shown('Min-P'),
  'model-profile-repeat-penalty',
  'model-profile-presence-penalty',
  'model-profile-backend-sampling',
  'model-profile-load-on-startup',
  'model-profile-chat-kwargs',
  'model-profile-chat-template',
  'model-profile-prefill-assistant',
  'model-profile-reasoning',
  'model-profile-reasoning-format',
  'model-profile-reasoning-effort',
  'model-profile-keep-past-thinking',
  'model-profile-thinking-budget',
  'model-profile-budget-message',
  'model-profile-max-output',
  'model-profile-vision-on-gpu',
  'model-profile-vision-gpu',
  'model-profile-min-image-tokens',
  'model-profile-max-image-tokens',
  'model-profile-gpu-layers',
  // Brief 19 (19 G.14): MoE CPU layers joins the editable fields. Its state
  // follows the model file's facts (pending and a dense model disable it),
  // which the cards' tests never read: the bff fake answers a reason, and a
  // reason leaves the field enabled. 19 G.14 is pinned in
  // ModelProfileLayers1.test.tsx.
  'model-profile-moe-cpu-layers',
  'model-profile-dense-ffn-cpu-layers',
  'model-profile-op-offload',
  'model-profile-repack',
  'model-profile-no-host-buffer',
  'model-profile-lazy-tensors',
  'model-profile-load-mode',
  'model-profile-fit-to-vram',
  'model-profile-fit-min-context',
  'model-profile-fit-free-vram',
  'model-profile-draft-device',
  'model-profile-draft-gpu-layers',
  // 2h.9's 189 to 191: the Speculative decoding card's sixteen fields, each
  // enabled by its group's rule (190, 191).
  'model-profile-draft-n-max',
  'model-profile-draft-p-min',
  'model-profile-draft-n-min',
  'model-profile-draft-n-ubatch',
  'model-profile-draft-k-cache-type',
  'model-profile-draft-v-cache-type',
  'model-profile-match-length',
  'model-profile-min-draft',
  'model-profile-max-draft',
  'model-profile-lookup-n-gram',
  'model-profile-draft-m-gram',
  'model-profile-min-hits',
]

/** The editable fields whose control sits behind a default box (2h.156): the
 * field identity mapped to the key in the read's `values`. A key the file lacks
 * starts its box ticked, and a ticked box disables and empties the input. */
const BOXED_FIELDS: Record<string, string> = {
  'model-profile-context-size': 'ctx-size',
  'model-profile-parallel-slots': 'parallel',
  'model-profile-per-slot-context': 'kv-unified-per-slot',
  'model-profile-batch-size': 'batch-size',
  'model-profile-u-batch-size': 'ubatch-size',
  'model-profile-cache-ram': 'cache-ram',
  'model-profile-cache-reuse': 'cache-reuse',
  'model-profile-context-checkpoints': 'ctx-checkpoints',
  'model-profile-checkpoint-step': 'checkpoint-min-step',
  'model-profile-temperature': 'temp',
  'Top-K': 'top-k',
  'Top-P': 'top-p',
  'Min-P': 'min-p',
  'model-profile-repeat-penalty': 'repeat-penalty',
  'model-profile-presence-penalty': 'presence-penalty',
  'model-profile-thinking-budget': 'reasoning-budget',
  'model-profile-max-output': 'n-predict',
  'model-profile-min-image-tokens': 'image-min-tokens',
  'model-profile-max-image-tokens': 'image-max-tokens',
  'model-profile-gpu-layers': 'n-gpu-layers',
  'model-profile-moe-cpu-layers': 'n-cpu-moe',
  'model-profile-dense-ffn-cpu-layers': 'n-cpu-ffn',
  'model-profile-fit-min-context': 'fit-ctx',
  'model-profile-draft-gpu-layers': 'n-gpu-layers-draft',
}

/** The editable fields whose control is a checkbox: the label's strings key
 * mapped to the strings key of the toggle text that names the control (2e's
 * 87: a checkbox's accessible name is its own text, not the field's label). */
const CHECKBOX_TOGGLES: Record<string, string> = {
  'model-profile-chat-template': 'model-profile-chat-template-toggle',
  'model-profile-prefill-assistant': 'model-profile-prefill-assistant-toggle',
  'model-profile-vision-on-gpu': 'model-profile-vision-offload-toggle',
}

/** One field's enable rule: `present` keys must be in the read's `values`
 * (with a value that is not the empty string, 176: `mmproj` present), and
 * `checked`, when set, must read as a checked flag (2e's 122). */
type Rule = { present: string[]; checked?: string }

/** The editable fields with an enable rule (2h.155, 2h.176, 2h.177): Unified
 * KV is enabled while Parallel slots' box is unticked, which is how a key the
 * file holds starts; every Vision control is enabled while the read's `mmproj`
 * is present and not empty, and Vision GPU also needs Vision on GPU checked;
 * Draft device and Draft GPU layers are enabled while the read's
 * `model-draft` is present and not empty (177), and Fit min context and Fit
 * free VRAM are enabled while the Fit to VRAM box reads checked at load
 * (2h.8's 186). */
const ENABLE_RULE: Record<string, Rule> = {
  'model-profile-unified-kv': { present: ['parallel'] },
  'model-profile-vision-on-gpu': { present: ['mmproj'] },
  'model-profile-vision-gpu': {
    present: ['mmproj'],
    checked: 'mmproj-offload',
  },
  'model-profile-min-image-tokens': { present: ['mmproj'] },
  'model-profile-max-image-tokens': { present: ['mmproj'] },
  'model-profile-draft-device': { present: ['model-draft'] },
  'model-profile-draft-gpu-layers': { present: ['model-draft'] },
  // 2h.8's 186: both Fit dependents are enabled only while the Fit to VRAM
  // box is checked, which is the load state below and the user's clicks.
  'model-profile-fit-min-context': { present: [], checked: 'fit' },
  'model-profile-fit-free-vram': { present: [], checked: 'fit' },
}

/** The flag spellings 2e.3c's 127 reads as on for `fit` (`common/arg.cpp`
 * `is_truthy`, compared trimmed), and the four it reads as off
 * (`is_falsey`). Any other value is invalid: 128 shows the box unchecked. */
const FIT_ON = ['on', 'enabled', 'true', '1']

/** True when 2e.3c's 127 shows the Fit to VRAM box checked on a read: the key
 * absent (llama.cpp's own default is on), or one of the four on words. */
export function fitCheckedAtLoad(values: Record<string, string>): boolean {
  const value = values['fit']
  if (value === undefined) return true
  return FIT_ON.includes(value.trim())
}

/** True when 2h.8's 184 counts the key as "set": the field's default box is
 * unticked, which is a key the file holds with a value (2e's 86: an empty
 * value reads as absent). */
function keySet(values: Record<string, string>, key: string): boolean {
  return values[key] !== undefined && values[key] !== ''
}

/** The strings key of the toggle text that names the Fit to VRAM checkbox
 * (2h.8's 184), from what the read leaves the two default boxes and the box's
 * own state. */
export function fitToggleKey(values: Record<string, string>): string {
  const ctx = keySet(values, 'ctx-size')
  const layers = keySet(values, 'n-gpu-layers')
  if (!ctx && !layers) return 'model-profile-fit-allowed'
  if (!fitCheckedAtLoad(values)) return 'model-profile-fit-not-allowed'
  if (ctx && layers) return 'model-profile-fit-nothing'
  if (ctx) return 'model-profile-fit-layers-only'
  return 'model-profile-fit-context-only'
}

/** The toggle text that names the Fit to VRAM checkbox (2h.8's 184), from
 * what the read leaves the two default boxes and the box's own state. */
export function fitToggleText(values: Record<string, string>): string {
  return en(fitToggleKey(values))
}

/** The flag spellings 2e's 122 reads as unchecked. Any other value, the empty
 * string included, and an absent key, read as checked. */
const FLAG_UNCHECKED = ['off', 'disabled', 'false', '0']

function flagChecked(values: Record<string, string>, key: string): boolean {
  if (key === 'fit') return fitCheckedAtLoad(values)
  const value = values[key]
  return value === undefined || !FLAG_UNCHECKED.includes(value)
}

/** True when 2h.1's 130, 2h.4's 155, 2h.5's 164, 2h.6's 170, 2h.7's 175,
 * 2h.8's 183 or 2h.9's 189 makes this field editable: its default box is
 * enabled, whatever the file holds. */
export function isEditableField(field: Field): boolean {
  return EDITABLE_FIELDS.some((f) => fieldId(f) === fieldId(fieldLabel(field)))
}

/** The 2h.9 groups of 2e's table 112, read from the prefix of the key of their
 * fields. `ngram-map-k4v` comes before `ngram-map-k`, whose prefix it starts
 * (191). */
const SPEC_NGRAM_GROUPS = [
  'ngram-mod',
  'ngram-simple',
  'ngram-map-k4v',
  'ngram-map-k',
]

/** The 2h.9 group of the keyed field, `draft` for the draft group, the group's
 * literal name for an n-gram group, `undefined` for any other key. */
function specGroup(key: string): string | undefined {
  if (key.startsWith('spec-draft-')) return 'draft'
  return SPEC_NGRAM_GROUPS.find((g) => key.startsWith(`spec-${g}-`))
}

/** The `spec-type` list as 2-models' 75 splits it: on `,`, each entry trimmed,
 * empty entries and `none` dropped. */
export function specTypeEntries(values: Record<string, string>): string[] {
  return (values['spec-type'] ?? '')
    .split(',')
    .map((entry) => entry.trim())
    .filter((entry) => entry !== '' && entry !== 'none')
}

/** True when 2h.9's 190 / 191 enables the group on a read: the draft group
 * while `model-draft` is not empty, or an entry starts with `draft-` (the
 * drafter a `spec-type` names, the stand-in for the model's MTP heads); an
 * n-gram group while an entry is exactly its name. */
export function specGroupEnabled(
  group: string,
  values: Record<string, string>,
): boolean {
  const entries = specTypeEntries(values)
  if (group === 'draft')
    return (
      (values['model-draft'] ?? '').trim() !== '' ||
      entries.some((entry) => entry.startsWith('draft-'))
    )
  return entries.includes(group)
}

/** True when the field's enable rule (155, 176, 190, 191) holds on a read that
 * holds `values`, whatever the state of its own default box. */
function ruleHolds(field: Field, values: Record<string, string>): boolean {
  const key = fieldKeyOf(field)
  const group = key === undefined ? undefined : specGroup(key)
  if (group !== undefined) return specGroupEnabled(group, values)
  const rule = ENABLE_RULE[fieldId(fieldLabel(field))]
  if (rule === undefined) return true
  for (const present of rule.present)
    if (values[present] === undefined || values[present] === '') return false
  return rule.checked === undefined || flagChecked(values, rule.checked)
}

/** True when the control of the field with this label key is enabled on a
 * read that holds `values`: a boxed field's input follows its box (156), and
 * a field with an enable rule follows that rule (155, 176, 190, 191), on top
 * of its box. False for a field of a read-only card, so the expectations of 85
 * stay pinned for every other card. */
export function controlEnabled(
  field: Field,
  values: Record<string, string>,
): boolean {
  if (!isEditableField(field) || !ruleHolds(field, values)) return false
  const boxed = boxedKeyOf(field)
  return boxed === undefined || boxed in values
}

/** Every field's tab stops in the order of 91: each field's About toggle,
 * then the controls 2h.1, 2h.4, 2h.5, 2h.6, 2h.7, 2h.8 and 2h.9 made editable
 * and left enabled on this read (141, 155, 156, 170, 176, 190, 191): a boxed
 * field's default box, named by the box's own text and only while the field's
 * enable rule leaves the box enabled (176), and its input while the box is
 * unticked; a checkbox, named by its toggle text (87). The whole-page tab path
 * of every model profile page test is this list, read with the values of its
 * fixture. */
export function fieldTabStops(values: Record<string, string>): string[] {
  const stops: string[] = []
  for (const card of MODEL_PROFILE_CARDS)
    for (const field of card.fields) {
      const label = labelText(fieldLabel(field))
      stops.push(`${en('field-about')} ${label}`)
      if (!isEditableField(field)) continue
      if (boxedKeyOf(field) !== undefined)
        if (isEditableField(field) && ruleHolds(field, values))
          stops.push(en('field-default'))
      if (!controlEnabled(field, values)) continue
      if (fieldId(fieldLabel(field)) === 'model-profile-fit-to-vram') {
        stops.push(fitToggleText(values))
        continue
      }
      const toggle = CHECKBOX_TOGGLES[fieldId(fieldLabel(field))]
      stops.push(toggle === undefined ? label : en(toggle))
    }
  return stops
}

/** The tab stops at the start of the page's main, in the order of 2h.10's 197
 * with H10 inserted (fe/briefs/08-hearth-h10-h11.md, H10.10): the back link,
 * then the jump disclosure named by `model-profile-jump-label`, then the
 * section jump list's buttons, one per card of 83 in page order, each named by
 * its card's title (196), then the Assets card's disclosure (H11b.4: the
 * Engine About toggle and select are gone, the Assets toggle is the stop right
 * after the jumps). The card h3s are not tab stops: only a jump button
 * gives one the focus. */
export function pageStartTabStops(): string[] {
  return [
    en('model-profile-back'),
    en('model-profile-jump-label'),
    ...cardHeadings(),
    en('model-profile-assets'),
  ]
}

/** The save bar's two buttons, at the end of the page (2h.1's 141). */
export function saveBarTabStops(): string[] {
  return [en('save-discard'), en('save-button')]
}
