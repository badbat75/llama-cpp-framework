// The plain cards of a preset's page (fe/briefs/02e-preset-cards.md, section
// 1.4): a card is a table of fields and each field is one row of data, the key
// it reads in the read's `values`, the strings keys of its label and hint, its
// control and, for a select, its options. A later slice adds rows; the renderer
// here does not change.
//
// Every control is natively disabled (criterion 85): the page shows the file
// and cannot change it until it can save, so nothing is ever sent. Each field
// is the same Field the Server page uses, so the page looks the same before
// and after the save lands.
//
// 2e.3 adds the rows of its tables (criteria 101, 102) and, with them, the
// kinds the renderer had not met: the checkbox, the note of a card (90) and
// the device select (103), which in 2e holds only the file's value because
// the device list arrives with 2f. 2e.4 adds the Hardware card (107) and, with
// it, the unit of a text row, the empty option each device select names for
// itself, a select matched ignoring case and space (Load mode), and a checkbox
// the configurator only allows under a rule (Fit to VRAM, 108). 2e.3c gives
// that row the reading llama-server has for `-fit [on|off]` (127), and the
// field error of a value the engine refuses to start with (128). 2e.5 adds the
// Speculative decoding card (112) and, with it, the group row of 90: a card row
// that is the GUI's bold sub-heading and the rows under it, so a card's rows
// nest and the one renderer still shows them all.
import Card from './Card.tsx'
import {
  CheckboxField,
  DefaultBoxField,
  SelectField,
  TextField,
} from './Field.tsx'
import { t } from './strings.ts'

/** A select's option: the value the file holds, the text the user reads. */
type Option = { value: string; text: string }

/** How a key's value is read as a boolean, the way llama-server reads it
 * (criterion 122):
 * - `flag`: an option without a value. The engine passes the negative flag for
 *   the four falsey words and the plain flag for any other value, the empty one
 *   included (`common_preset::to_args` and `common_arg_utils::is_falsey`).
 * - `truthy`: a preset-only key the router reads as truthy only, so on is the
 *   short list of words and every other value, the empty one included, is off
 *   (`is_truthy`, `tools/server/server-models.cpp`).
 * - `valued`: `[on|off|auto]`: its handler reads both lists and keeps any other
 *   value as the extra option of criterion 87.
 * - `fit`: `-fit [on|off]`, the one checkbox whose value the engine reads with
 *   the same two lists and refuses anything outside them, so such a value is an
 *   error and not a spelling (criterion 128). The key is absent for it, too,
 *   where the engine's own default is on (`common/common.h`). */
type Reading = 'flag' | 'truthy' | 'valued' | 'fit'

/** What every row carries: the key and the strings keys of its texts. */
type RowBase = {
  /** The key of the read's `values`. */
  key: string
  label: string
  hint: string
  /** The reading of a checkbox or select row whose key is a boolean. Absent for
   * every other row: its value is not a boolean, so it keeps criterion 87. */
  reading?: Reading
}

/** A number: the default box and a text input showing the string as it is. The
 * unit, when the table gives one, sits after the input (criterion 89). */
type NumberRow = RowBase & {
  kind: 'number'
  integer: boolean
  unit?: string
}

/** A select: its options in the table's order, and the option an absent key
 * shows. `normalize` is the match of a key whose value the configurator reads
 * ignoring case and surrounding space (`normalize_load_mode`), so such a value
 * selects its option instead of becoming an extra one. */
type SelectRow = RowBase & {
  kind: 'select'
  options: Option[]
  absent: string
  normalize?: boolean
}

/** A text: one monospace input showing the string as it is. The unit, when the
 * table gives one, sits after the input (criterion 89). */
type TextRow = RowBase & { kind: 'text'; unit?: string }

/** A checkbox: one checkbox whose text is the toggle. `absent` is the state an
 * absent key shows and, as the configurator keeps the default it has for a key
 * for a value it cannot parse, the state of such a value. */
type CheckboxRow = RowBase & {
  kind: 'checkbox'
  toggle: string
  absent: boolean
  /** A box the configurator only lets the user set under a rule (criterion
   * 108: Fit to VRAM sizes the context and the GPU layers, so it is allowed
   * while both are absent). Each key of `needsAbsent` must be absent; when one
   * is not, the text is `notAllowed`. The box keeps showing the file's reading
   * either way (criterion 127). */
  allowed?: { needsAbsent: string[]; notAllowed: string }
}

/** A device select (criterion 103): the empty option, then the file's value
 * when it has one. The device list arrives with 2f. `empty` is the text of the
 * empty option, which each field words for its own device (110). */
type DeviceRow = RowBase & { kind: 'device'; empty: string }

/** A note between fields (criterion 90): the GUI's muted paragraph. Its `key`
 * is only the list's: a note reads no key of the file. */
type NoteRow = { kind: 'note'; key: string; text: string }

/** A group of a card (criterion 90): the GUI's bold sub-heading as an `h4`, and
 * the rows under it in a `role="group"` division named by that heading, so the
 * fields that share a label across groups (the Lookup n-gram and the Draft
 * m-gram of 113) are told apart by their group. */
type GroupRow = {
  kind: 'group'
  key: string
  /** The heading's text: a strings key where the GUI words the group, a
   * literal where its name is a value the file writes (the four speculator
   * names of 114). */
  title: string
  rows: FieldRow[]
}

type Row =
  NumberRow | SelectRow | TextRow | CheckboxRow | DeviceRow | NoteRow | GroupRow

/** The rows that are a field or a note: what a group holds, and what
 * `PresetField` shows. */
type FieldRow = Exclude<Row, GroupRow>

/** A card: its h3 and its fields, in the order of the GUI's page. */
type CardFields = { title: string; rows: Row[] }

/** The tri-state of section 1.3: default / on / off, as `emit_bool` writes it. */
const TRISTATE: Option[] = [
  { value: 'default', text: t('option-default') },
  { value: 'true', text: t('option-on') },
  { value: 'false', text: t('option-off') },
]

/** Off / on of section 1.3: a key the file holds only when it is true. */
const OFF_ON: Option[] = [
  { value: 'false', text: t('option-off') },
  { value: 'true', text: t('option-on') },
]

/** The cache types of 2e.2 (criterion 96): `default` is a string, the ten
 * types are the literals the file writes. */
const CACHE_TYPES: Option[] = [
  { value: 'default', text: t('option-default') },
  ...[
    'f32',
    'f16',
    'bf16',
    'f8_e4m3',
    'q8_0',
    'q5_1',
    'q5_0',
    'q4_1',
    'q4_0',
    'iq4_nl',
  ].map((name) => ({ value: name, text: name })),
]

/** The Reasoning select of 101: auto / on / off. */
const REASONING: Option[] = [
  { value: 'auto', text: t('option-auto') },
  { value: 'on', text: t('option-on') },
  { value: 'off', text: t('option-off') },
]

/** The Reasoning format of 101: `deepseek` is a literal (table 101). */
const REASONING_FORMAT: Option[] = [
  { value: 'auto', text: t('option-auto') },
  { value: 'none', text: t('option-none') },
  { value: 'deepseek', text: 'deepseek' },
]

/** The reasoning-effort options of 101 (MP 917-918). */
const EFFORT: Option[] = [
  { value: 'default', text: t('option-default') },
  { value: 'minimal', text: t('option-minimal') },
  { value: 'low', text: t('option-low') },
  { value: 'medium', text: t('option-medium') },
  { value: 'high', text: t('option-high') },
  { value: 'xhigh', text: t('option-extra-high') },
  { value: 'max', text: t('option-maximum') },
]

/** The Lazy tensors options of 107 (MP 564-565): the file's value is the
 * option's value, matched exactly. */
const LAZY_MODES: Option[] = [
  { value: 'default', text: t('option-default') },
  { value: 'on', text: t('option-on') },
  { value: 'auto', text: t('option-auto') },
  { value: 'off', text: t('option-off') },
]

/** The Load mode options of 107 (MP 102-103 of `components.slint`): three
 * strings, then the literals the file writes. */
const LOAD_MODES: Option[] = [
  { value: 'default', text: t('option-default') },
  { value: 'auto', text: t('option-auto') },
  { value: 'none', text: t('option-none') },
  { value: 'mmap', text: 'mmap' },
  { value: 'mlock', text: 'mlock' },
  { value: 'mmap+mlock', text: 'mmap+mlock' },
  { value: 'dio', text: 'dio' },
]

/** The cards built so far, in the order of criterion 83: the Hardware card of
 * 2e.4 (107) after the Assets card, then the two of 2e.1 (Resource / context,
 * 92, and Embeddings, 93), the three of 2e.2 (KV cache, 96, Sampling overrides,
 * 97, and Advanced, 98), and the two of 2e.3 (Chat / reasoning, 101, after KV
 * cache, and Vision, 102, after Advanced), and the one of 2e.5 (Speculative
 * decoding, 112, after Vision, in the five groups of 90). */
const CARDS: CardFields[] = [
  {
    title: t('preset-hardware-config'),
    rows: [
      {
        kind: 'number',
        integer: true,
        key: 'n-gpu-layers',
        label: t('preset-gpu-layers'),
        hint: t('preset-gpu-layers-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'n-cpu-moe',
        label: t('preset-moe-cpu-layers'),
        hint: t('preset-moe-cpu-layers-hint'),
      },
      {
        kind: 'number',
        integer: true,
        unit: 'MiB',
        key: 'moe-cache-mib',
        label: t('preset-moe-cache'),
        hint: t('preset-moe-cache-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'n-cpu-ffn',
        label: t('preset-dense-ffn-cpu-layers'),
        hint: t('preset-dense-ffn-cpu-layers-hint'),
      },
      {
        kind: 'select',
        key: 'op-offload',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('preset-op-offload'),
        hint: t('preset-op-offload-hint'),
      },
      {
        kind: 'select',
        key: 'repack',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('preset-repack'),
        hint: t('preset-repack-hint'),
      },
      {
        kind: 'select',
        key: 'no-host',
        reading: 'flag',
        options: OFF_ON,
        absent: 'false',
        label: t('preset-no-host-buffer'),
        hint: t('preset-no-host-buffer-hint'),
      },
      {
        kind: 'select',
        key: 'lazy-mode',
        options: LAZY_MODES,
        absent: 'default',
        label: t('preset-lazy-tensors'),
        hint: t('preset-lazy-tensors-hint'),
      },
      {
        kind: 'select',
        key: 'load-mode',
        options: LOAD_MODES,
        absent: 'default',
        normalize: true,
        label: t('preset-load-mode'),
        hint: t('preset-load-mode-hint'),
      },
      {
        kind: 'checkbox',
        key: 'fit',
        reading: 'fit',
        toggle: t('preset-fit-allowed'),
        absent: true,
        allowed: {
          needsAbsent: ['ctx-size', 'n-gpu-layers'],
          notAllowed: t('preset-fit-not-allowed'),
        },
        label: t('preset-fit-to-vram'),
        hint: t('preset-fit-to-vram-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'fit-ctx',
        label: t('preset-fit-min-context'),
        hint: t('preset-fit-min-context-hint'),
      },
      {
        kind: 'text',
        unit: 'MiB',
        key: 'fit-target',
        label: t('preset-fit-free-vram'),
        hint: t('preset-fit-free-vram-hint'),
      },
      {
        kind: 'device',
        key: 'device-draft',
        empty: t('preset-device-same-as-model'),
        label: t('preset-draft-device'),
        hint: t('preset-draft-device-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'n-gpu-layers-draft',
        label: t('preset-draft-gpu-layers'),
        hint: t('preset-draft-gpu-layers-hint'),
      },
    ],
  },
  {
    title: t('preset-resource-context'),
    rows: [
      {
        kind: 'number',
        integer: true,
        key: 'ctx-size',
        label: t('preset-context-size'),
        hint: t('preset-context-size-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'parallel',
        label: t('preset-parallel-slots'),
        hint: t('preset-parallel-slots-hint'),
      },
      {
        kind: 'select',
        key: 'kv-unified',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('preset-unified-kv'),
        hint: t('preset-unified-kv-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'kv-unified-per-slot',
        label: t('preset-per-slot-context'),
        hint: t('preset-per-slot-context-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'batch-size',
        label: t('preset-batch-size'),
        hint: t('preset-batch-size-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'ubatch-size',
        label: t('preset-u-batch-size'),
        hint: t('preset-u-batch-size-hint'),
      },
    ],
  },
  {
    title: t('preset-embeddings'),
    rows: [
      {
        kind: 'select',
        key: 'embeddings',
        reading: 'flag',
        options: OFF_ON,
        absent: 'false',
        label: t('preset-embedding-mode'),
        hint: t('preset-embedding-mode-hint'),
      },
      {
        kind: 'select',
        key: 'pooling',
        options: [
          { value: 'default', text: t('option-default') },
          { value: 'none', text: t('option-none') },
          { value: 'mean', text: t('option-mean') },
          // Not a strings key: a literal of the Embeddings table of
          // fe/briefs/02e-preset-cards.md section 1.4.
          { value: 'cls', text: 'cls' },
          { value: 'last', text: t('option-last') },
          { value: 'rank', text: t('option-rank') },
        ],
        absent: 'default',
        label: t('preset-pooling'),
        hint: t('preset-pooling-hint'),
      },
    ],
  },
  {
    title: t('preset-kv-cache'),
    rows: [
      {
        kind: 'select',
        key: 'cache-type-k',
        options: CACHE_TYPES,
        absent: 'default',
        label: t('preset-k-cache-type'),
        hint: t('preset-k-cache-type-hint'),
      },
      {
        kind: 'select',
        key: 'cache-type-v',
        options: CACHE_TYPES,
        absent: 'default',
        label: t('preset-v-cache-type'),
        hint: t('preset-v-cache-type-hint'),
      },
      {
        kind: 'select',
        key: 'flash-attn',
        reading: 'valued',
        options: TRISTATE,
        absent: 'default',
        label: t('preset-flash-attention'),
        hint: t('preset-flash-attention-hint'),
      },
      {
        kind: 'number',
        integer: true,
        unit: 'MiB',
        key: 'cache-ram',
        label: t('preset-cache-ram'),
        hint: t('preset-cache-ram-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'cache-reuse',
        label: t('preset-cache-reuse'),
        hint: t('preset-cache-reuse-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'ctx-checkpoints',
        label: t('preset-context-checkpoints'),
        hint: t('preset-context-checkpoints-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'checkpoint-min-step',
        label: t('preset-checkpoint-step'),
        hint: t('preset-checkpoint-step-hint'),
      },
    ],
  },
  {
    title: t('preset-chat-reasoning'),
    rows: [
      {
        kind: 'checkbox',
        key: 'jinja',
        reading: 'flag',
        toggle: t('preset-chat-template-toggle'),
        absent: true,
        label: t('preset-chat-template'),
        hint: t('preset-chat-template-hint'),
      },
      {
        kind: 'checkbox',
        key: 'prefill-assistant',
        reading: 'flag',
        toggle: t('preset-prefill-assistant-toggle'),
        absent: true,
        label: t('preset-prefill-assistant'),
        hint: t('preset-prefill-assistant-hint'),
      },
      {
        kind: 'select',
        key: 'reasoning',
        options: REASONING,
        absent: 'auto',
        label: t('preset-reasoning'),
        hint: t('preset-reasoning-hint'),
      },
      {
        kind: 'select',
        key: 'reasoning-format',
        options: REASONING_FORMAT,
        absent: 'auto',
        label: t('preset-reasoning-format'),
        hint: t('preset-reasoning-format-hint'),
      },
      {
        kind: 'select',
        key: 'reasoning-effort',
        options: EFFORT,
        absent: 'default',
        label: t('preset-reasoning-effort'),
        hint: t('preset-reasoning-effort-hint'),
      },
      {
        kind: 'select',
        key: 'reasoning-preserve',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('preset-keep-past-thinking'),
        hint: t('preset-keep-past-thinking-hint'),
      },
      {
        // The note of 90, at the GUI's place (MP 933-938).
        kind: 'note',
        key: 'output-budget-note',
        text: t('preset-output-budget-note'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'reasoning-budget',
        label: t('preset-thinking-budget'),
        hint: t('preset-thinking-budget-hint'),
      },
      {
        kind: 'text',
        key: 'reasoning-budget-message',
        label: t('preset-budget-message'),
        hint: t('preset-budget-message-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'n-predict',
        label: t('preset-max-output'),
        hint: t('preset-max-output-hint'),
      },
    ],
  },
  {
    title: t('preset-sampling-overrides'),
    rows: [
      {
        kind: 'number',
        integer: false,
        key: 'temp',
        label: t('preset-temperature'),
        hint: t('preset-temperature-hint'),
      },
      {
        // Not strings keys: Top-K, Top-P and Min-P are acronyms, so their
        // labels are literals (table 97).
        kind: 'number',
        integer: true,
        key: 'top-k',
        label: 'Top-K',
        hint: t('preset-top-k-hint'),
      },
      {
        kind: 'number',
        integer: false,
        key: 'top-p',
        label: 'Top-P',
        hint: t('preset-top-p-hint'),
      },
      {
        kind: 'number',
        integer: false,
        key: 'min-p',
        label: 'Min-P',
        hint: t('preset-min-p-hint'),
      },
      {
        kind: 'number',
        integer: false,
        key: 'repeat-penalty',
        label: t('preset-repeat-penalty'),
        hint: t('preset-repeat-penalty-hint'),
      },
      {
        kind: 'number',
        integer: false,
        key: 'presence-penalty',
        label: t('preset-presence-penalty'),
        hint: t('preset-presence-penalty-hint'),
      },
      {
        kind: 'select',
        key: 'backend-sampling',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('preset-backend-sampling'),
        hint: t('preset-backend-sampling-hint'),
      },
    ],
  },
  {
    title: t('preset-advanced'),
    rows: [
      {
        kind: 'select',
        key: 'load-on-startup',
        reading: 'truthy',
        options: OFF_ON,
        absent: 'false',
        label: t('preset-load-on-startup'),
        hint: t('preset-load-on-startup-hint'),
      },
      {
        kind: 'text',
        key: 'chat-template-kwargs',
        label: t('preset-chat-kwargs'),
        hint: t('preset-chat-kwargs-hint'),
      },
    ],
  },
  {
    title: t('preset-vision'),
    rows: [
      {
        kind: 'checkbox',
        key: 'mmproj-offload',
        reading: 'flag',
        toggle: t('preset-vision-offload-toggle'),
        absent: true,
        label: t('preset-vision-on-gpu'),
        hint: t('preset-vision-on-gpu-hint'),
      },
      {
        kind: 'device',
        key: 'mmproj-device',
        empty: t('preset-device-follows-model'),
        label: t('preset-vision-gpu'),
        hint: t('preset-vision-gpu-hint'),
      },
      {
        // The note of 90, at the GUI's place (MP 1099-1104).
        kind: 'note',
        key: 'image-tokens-note',
        text: t('preset-image-tokens-note'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'image-min-tokens',
        label: t('preset-min-image-tokens'),
        hint: t('preset-min-image-tokens-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'image-max-tokens',
        label: t('preset-max-image-tokens'),
        hint: t('preset-max-image-tokens-hint'),
      },
    ],
  },
  {
    title: t('preset-speculative-decoding'),
    rows: [
      {
        kind: 'group',
        key: 'draft-model',
        title: t('preset-draft-model-group'),
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-draft-n-max',
            label: t('preset-draft-n-max'),
            hint: t('preset-draft-n-max-hint'),
          },
          {
            // The card's only float row (table 112).
            kind: 'number',
            integer: false,
            key: 'spec-draft-p-min',
            label: t('preset-draft-p-min'),
            hint: t('preset-draft-p-min-hint'),
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-draft-n-min',
            label: t('preset-draft-n-min'),
            hint: t('preset-draft-n-min-hint'),
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-draft-n-ubatch',
            label: t('preset-draft-n-ubatch'),
            hint: t('preset-draft-n-ubatch-hint'),
          },
          {
            // The note of 90, at the GUI's place: before the two cache types
            // it speaks of.
            kind: 'note',
            key: 'draft-cache-note',
            text: t('preset-draft-cache-note'),
          },
          {
            kind: 'select',
            key: 'spec-draft-type-k',
            options: CACHE_TYPES,
            absent: 'default',
            label: t('preset-draft-k-cache-type'),
            hint: t('preset-draft-k-cache-type-hint'),
          },
          {
            kind: 'select',
            key: 'spec-draft-type-v',
            options: CACHE_TYPES,
            absent: 'default',
            label: t('preset-draft-v-cache-type'),
            hint: t('preset-draft-v-cache-type-hint'),
          },
        ],
      },
      {
        // The four groups are the speculator names the file writes, so they are
        // literals and not strings (114, D7). Each group's rows share their
        // labels with another group's: the group tells them apart (113).
        kind: 'group',
        key: 'ngram-mod',
        title: 'ngram-mod',
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-mod-n-match',
            label: t('preset-match-length'),
            hint: t('preset-match-length-hint'),
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-mod-n-min',
            label: t('preset-min-draft'),
            hint: t('preset-min-draft-hint'),
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-mod-n-max',
            label: t('preset-max-draft'),
            hint: t('preset-max-draft-hint'),
          },
        ],
      },
      {
        kind: 'group',
        key: 'ngram-simple',
        title: 'ngram-simple',
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-simple-size-n',
            label: t('preset-lookup-n-gram'),
            hint: t('preset-ngram-simple-lookup-hint'),
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-simple-size-m',
            label: t('preset-draft-m-gram'),
            hint: t('preset-ngram-simple-draft-hint'),
          },
        ],
      },
      {
        kind: 'group',
        key: 'ngram-map-k',
        title: 'ngram-map-k',
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k-size-n',
            label: t('preset-lookup-n-gram'),
            hint: t('preset-ngram-map-lookup-hint'),
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k-size-m',
            label: t('preset-draft-m-gram'),
            hint: t('preset-ngram-map-draft-hint'),
          },
        ],
      },
      {
        kind: 'group',
        key: 'ngram-map-k4v',
        title: 'ngram-map-k4v',
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k4v-size-n',
            label: t('preset-lookup-n-gram'),
            hint: t('preset-ngram-map-values-lookup-hint'),
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k4v-size-m',
            label: t('preset-draft-m-gram'),
            hint: t('preset-ngram-map-values-draft-hint'),
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k4v-min-hits',
            label: t('preset-min-hits'),
            hint: t('preset-min-hits-hint'),
          },
        ],
      },
    ],
  },
]

/** The string a key holds: absent and the empty string read alike (criterion
 * 86). */
function held(values: Record<string, string>, key: string): string | undefined {
  const raw = values[key]
  return raw === undefined || raw === '' ? undefined : raw
}

/** The string a row reads. For a flag the key is present when the file holds it,
 * the empty value included, because llama-server then passes the plain flag
 * (criterion 123). `fit` takes a value too, so `fit =` reaches the engine as
 * `--fit ""`: present, and refused (criterion 127). Every other row keeps
 * criterion 86: an empty value is absent. */
function heldFor(
  row: RowBase,
  values: Record<string, string>,
): string | undefined {
  const raw = values[row.key]
  if (raw === undefined) return undefined
  return row.reading === 'flag' ||
    row.reading === 'truthy' ||
    row.reading === 'fit'
    ? raw
    : raw === ''
      ? undefined
      : raw
}

/** The words llama-server reads as off for a flag (`common_arg_utils::is_falsey`),
 * and the words the router reads as on for a key it reads as truthy only
 * (`is_truthy`). Compared trimmed, since the INI grammar drops the space around
 * a value, and case-sensitive, as both are (`common/arg.cpp`, criterion 122). */
const FALSEY = ['off', 'disabled', 'false', '0']
const TRUTHY = ['on', 'enabled', 'true', '1']

/** The trim of the INI grammar the engine reads a value with (`common/preset.cpp`,
 * criterion 122's intro): the spaces (U+0020) and tabs (U+0009) around the value,
 * and nothing else, so `off` followed by a non-breaking space stays itself. Not
 * JavaScript's `trim()`, which drops every Unicode space. */
function iniTrim(value: string): string {
  return value.replace(/^[ \t]+|[ \t]+$/g, '')
}

/** The written-as line's text (criterion 124): the value as the file writes it,
 * trimmed, or the words for a key written with no value. */
function writtenText(trimmed: string): string {
  return trimmed === ''
    ? t('preset-written-empty')
    : t('preset-written-as', trimmed)
}

/** A key's value read as llama-server reads it (criterion 122), with the
 * spelling to show under the control (124).
 *
 * `on` is the state the engine acts as. It is `undefined` only for a `valued`
 * key whose value is in neither list: the engine is given that value, so the
 * control shows it as the extra option of criterion 87.
 *
 * `written` is the line's text for a value the configurator would not write: the
 * configurator keeps `true` and `false`, so those two show no line. The `fit`
 * row's configurator writes `on` and `off`, so those two show no line there.
 *
 * `error` is the field's error for a value the engine refuses to start with
 * (criterion 128): only `fit` refuses, the other readings hand the value on. */
function readAs(
  reading: Reading,
  raw: string,
): {
  on: boolean | undefined
  written: string | undefined
  error: string | undefined
} {
  const trimmed = iniTrim(raw)
  const falsey = FALSEY.includes(trimmed)
  const truthy = TRUTHY.includes(trimmed)
  if (reading === 'fit')
    return {
      on: truthy,
      written: raw === 'on' || raw === 'off' ? undefined : writtenText(trimmed),
      error: falsey || truthy ? undefined : t('preset-fit-invalid'),
    }
  if (reading === 'valued' && !falsey && !truthy)
    return { on: undefined, written: undefined, error: undefined }
  return {
    on: reading === 'flag' ? !falsey : truthy,
    written:
      trimmed === 'true' || trimmed === 'false'
        ? undefined
        : writtenText(trimmed),
    error: undefined,
  }
}

/** A select's options and its selected value. A value no option carries
 * becomes one extra last option holding it as its value and its text, selected
 * (criterion 87). A row whose key is a boolean of table 122 holds it trimmed as
 * the INI grammar trims it (criterion 124a). A row that normalizes matches an
 * option ignoring case and surrounding space, and the extra option still
 * carries the value as written. */
function shown(
  row: SelectRow,
  value: string | undefined,
): {
  options: Option[]
  value: string
} {
  if (value === undefined) return { options: row.options, value: row.absent }
  const match = row.normalize ? value.trim().toLowerCase() : value
  const hit = row.options.find((o) => o.value === match)
  if (hit !== undefined) return { options: row.options, value: hit.value }
  const extra = row.reading === undefined ? value : iniTrim(value)
  return {
    options: [...row.options, { value: extra, text: extra }],
    value: extra,
  }
}

/** A device select's options and selected value (criterion 103): the empty
 * option first, then the device the file names as its own option, selected.
 * The page has no device list to add, so it lists no device of its own. */
function shownDevice(
  row: DeviceRow,
  value: string | undefined,
): {
  options: Option[]
  value: string
} {
  const none = { value: '', text: row.empty }
  if (value === undefined) return { options: [none], value: '' }
  return { options: [none, { value, text: value }], value }
}

/** The full-value line's limits, in characters (criterion 119): the number
 * input is 10ch wide (`input.number`), a text input or a select holds about 20
 * in the field column at 320 px, the narrowest case. A count and not a
 * measure, so the rule is the same at every width and needs no layout. */
const FULL_LIMIT: Record<'number' | 'text' | 'select' | 'device', number> = {
  number: 8,
  text: 20,
  select: 20,
  device: 20,
}

/** The full-value line of criterion 118: a value longer than its control shows
 * is repeated in full under it, since a disabled control cannot be scrolled or
 * wrapped. `undefined` when the control shows all of the value: nothing to add.
 */
function fullValue(
  row: NumberRow | TextRow | SelectRow | DeviceRow,
  text: string,
) {
  return text.length <= FULL_LIMIT[row.kind]
    ? undefined
    : { id: `preset-full-${row.key}`, text }
}

/** One field of a card, in the form its row names. */
function PresetField({
  row,
  values,
}: {
  row: FieldRow
  values: Record<string, string>
}) {
  if (row.kind === 'note') return <p className="note">{row.text}</p>
  // What every control of a read-only card shares: its id, the texts of its
  // field, and the native disabled of criterion 85. Each control's own
  // handler does nothing: the page sends nothing.
  const common = {
    id: `preset-${row.key}`,
    label: row.label,
    hint: row.hint,
    disabled: true,
  }
  const value = heldFor(row, values)
  // A row whose key is a boolean: the reading gives the state the engine acts as
  // and the spelling the file holds (criterion 122).
  const state =
    row.reading === undefined || value === undefined
      ? undefined
      : readAs(row.reading, value)
  if (row.kind === 'number') {
    const text = value ?? ''
    return (
      <DefaultBoxField
        {...common}
        box={t('field-default')}
        numeric={row.integer}
        unit={row.unit}
        ticked={value === undefined}
        text={text}
        full={fullValue(row, text)}
        onTick={() => undefined}
        onText={() => undefined}
      />
    )
  }
  if (row.kind === 'checkbox') {
    // A box the configurator only allows under a rule (108) says so in its own
    // text. The rule says nothing about the box itself: what the box shows is
    // what the engine does with the file's value, allowed or not (127), and a
    // value the engine refuses carries the field's error (128).
    const rule = row.allowed
    const toggle =
      rule === undefined ||
      rule.needsAbsent.every((key) => held(values, key) === undefined)
        ? row.toggle
        : rule.notAllowed
    const written = state?.written
    return (
      <CheckboxField
        {...common}
        toggle={toggle}
        checked={state === undefined ? row.absent : state.on === true}
        written={
          written === undefined
            ? undefined
            : { id: `preset-written-${row.key}`, text: written }
        }
        error={state?.error}
        onChange={() => undefined}
      />
    )
  }
  if (row.kind === 'text') {
    // No placeholder: an example in a field nobody can type into reads as a
    // value (criterion 87).
    const text = value ?? ''
    return (
      <TextField
        {...common}
        value={text}
        unit={row.unit}
        full={fullValue(row, text)}
        placeholder=""
        onChange={() => undefined}
      />
    )
  }
  const select =
    row.kind === 'device'
      ? shownDevice(row, value)
      : // A flag's select shows on or off by the reading and lists no extra
        // option (criterion 125). A valued key whose value is in neither list
        // keeps criterion 87: the extra option carries it.
        state !== undefined && state.on !== undefined
        ? { options: row.options, value: state.on ? 'true' : 'false' }
        : shown(row, value)
  // What the user reads for a select is the selected option's text, which for
  // an unlisted value is the value itself (criterion 87). A device select
  // reads the file's device, and has nothing to repeat when the file names
  // none (criterion 103).
  const text =
    row.kind === 'device'
      ? (value ?? '')
      : (select.options.find((o) => o.value === select.value)?.text ??
        select.value)
  const full = fullValue(row, text)
  // The written-as line of criterion 124: the control shows what the engine does
  // with the value, the line what the file holds. A flag's control shows on or
  // off, so any value the configurator would not write needs the line. A valued
  // key whose value is in neither list is the extra option, so the select shows
  // it itself and needs no line (124a): what the select clips, the full-value
  // line of 118 repeats.
  const written = state?.written
  return (
    <SelectField
      {...common}
      options={select.options}
      value={select.value}
      full={full}
      written={
        written === undefined
          ? undefined
          : { id: `preset-written-${row.key}`, text: written }
      }
      onChange={() => undefined}
    />
  )
}

/** One row of a card: a group is its heading and the division the heading
 * names, holding its own rows; every other row is the field of `PresetField`. */
function PresetRow({
  row,
  values,
}: {
  row: Row
  values: Record<string, string>
}) {
  if (row.kind === 'group') {
    const heading = `preset-group-${row.key}`
    return (
      <div className="subgroup">
        <h4 id={heading}>{row.title}</h4>
        <div role="group" aria-labelledby={heading}>
          {row.rows.map((nested) => (
            <PresetRow key={nested.key} row={nested} values={values} />
          ))}
        </div>
      </div>
    )
  }
  return <PresetField row={row} values={values} />
}

/** The cards, in the order of the table, each with the fields of its rows. */
export default function PresetCards({
  values,
}: {
  values: Record<string, string>
}) {
  return (
    <>
      {CARDS.map((card) => (
        <Card key={card.title} title={card.title}>
          {card.rows.map((row) => (
            <PresetRow key={row.key} row={row} values={values} />
          ))}
        </Card>
      ))}
    </>
  )
}
