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
// the device list arrives with 2f.
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

/** What every row carries: the key and the strings keys of its texts. */
type RowBase = {
  /** The key of the read's `values`. */
  key: string
  label: string
  hint: string
}

/** A number: the default box and a text input showing the string as it is. The
 * unit, when the table gives one, sits after the input (criterion 89). */
type NumberRow = RowBase & {
  kind: 'number'
  integer: boolean
  unit?: string
}

/** A select: its options in the table's order, and the option an absent key
 * shows. */
type SelectRow = RowBase & {
  kind: 'select'
  options: Option[]
  absent: string
}

/** A text: one monospace input showing the string as it is. */
type TextRow = RowBase & { kind: 'text' }

/** A checkbox: one checkbox whose text is the toggle. `absent` is the state an
 * absent key shows and, as the configurator keeps what it has for a value it
 * cannot parse (`form.rs`: `unwrap_or(true)`), the state of such a value. */
type CheckboxRow = RowBase & {
  kind: 'checkbox'
  toggle: string
  absent: boolean
}

/** A device select (criterion 103): the empty option, then the file's value
 * when it has one. The device list arrives with 2f. */
type DeviceRow = RowBase & { kind: 'device' }

/** A note between fields (criterion 90): the GUI's muted paragraph. Its `key`
 * is only the list's: a note reads no key of the file. */
type NoteRow = { kind: 'note'; key: string; text: string }

type Row = NumberRow | SelectRow | TextRow | CheckboxRow | DeviceRow | NoteRow

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

/** The empty option of a device select (103): the file names no device. */
const DEVICE_NONE: Option = {
  value: '',
  text: t('preset-device-follows-model'),
}

/** The cards built so far, in the order of criterion 83: the two of 2e.1
 * (Resource / context, 92, and Embeddings, 93) after the Assets card, the
 * three of 2e.2 (KV cache, 96, Sampling overrides, 97, and Advanced, 98), and
 * the two of 2e.3 (Chat / reasoning, 101, after KV cache, and Vision, 102,
 * after Advanced). */
const CARDS: CardFields[] = [
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
        toggle: t('preset-chat-template-toggle'),
        absent: true,
        label: t('preset-chat-template'),
        hint: t('preset-chat-template-hint'),
      },
      {
        kind: 'checkbox',
        key: 'prefill-assistant',
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
        toggle: t('preset-vision-offload-toggle'),
        absent: true,
        label: t('preset-vision-on-gpu'),
        hint: t('preset-vision-on-gpu-hint'),
      },
      {
        kind: 'device',
        key: 'mmproj-device',
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
]

/** The string a key holds: absent and the empty string read alike (criterion
 * 86). */
function held(values: Record<string, string>, key: string): string | undefined {
  const raw = values[key]
  return raw === undefined || raw === '' ? undefined : raw
}

/** A value as the configurator reads a boolean: `ini::parse_bool`
 * (crates/core/src/ini.rs) answers only for `true` and `false`, trimmed, and
 * gives nothing for the rest. A checkbox row decides what that nothing shows
 * (its `absent`). */
function parseBool(value: string): boolean | undefined {
  const text = value.trim()
  return text === 'true' ? true : text === 'false' ? false : undefined
}

/** A select's options and its selected value. A value no option carries
 * becomes one extra last option holding it as its value and its text, selected
 * (criterion 87). */
function shown(
  row: SelectRow,
  value: string | undefined,
): {
  options: Option[]
  value: string
} {
  if (value === undefined) return { options: row.options, value: row.absent }
  if (row.options.some((o) => o.value === value))
    return { options: row.options, value }
  return {
    options: [...row.options, { value, text: value }],
    value,
  }
}

/** A device select's options and selected value (criterion 103): the empty
 * option first, then the device the file names as its own option, selected.
 * The page has no device list to add, so it lists no device of its own. */
function shownDevice(value: string | undefined): {
  options: Option[]
  value: string
} {
  if (value === undefined) return { options: [DEVICE_NONE], value: '' }
  return { options: [DEVICE_NONE, { value, text: value }], value }
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
  row: Row
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
  const value = held(values, row.key)
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
    return (
      <CheckboxField
        {...common}
        toggle={row.toggle}
        checked={parseBool(value ?? '') ?? row.absent}
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
        full={fullValue(row, text)}
        placeholder=""
        onChange={() => undefined}
      />
    )
  }
  const select = row.kind === 'device' ? shownDevice(value) : shown(row, value)
  // What the user reads for a select is the selected option's text, which for
  // an unlisted value is the value itself (criterion 87). A device select
  // reads the file's device, and has nothing to repeat when the file names
  // none (criterion 103).
  const text =
    row.kind === 'device'
      ? (value ?? '')
      : (select.options.find((o) => o.value === select.value)?.text ??
        select.value)
  return (
    <SelectField
      {...common}
      options={select.options}
      value={select.value}
      full={fullValue(row, text)}
      onChange={() => undefined}
    />
  )
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
            <PresetField key={row.key} row={row} values={values} />
          ))}
        </Card>
      ))}
    </>
  )
}
