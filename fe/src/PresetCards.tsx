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
import Card from './Card.tsx'
import { DefaultBoxField, SelectField, TextField } from './Field.tsx'
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

type Row = NumberRow | SelectRow | TextRow

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

/** The cards built so far, in the order of criterion 83: the two of 2e.1
 * (Resource / context, 92, and Embeddings, 93) after the Assets card, then the
 * three of 2e.2 (KV cache, 96, Sampling overrides, 97, and Advanced, 98). */
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
]

/** The string a key holds: absent and the empty string read alike (criterion
 * 86). */
function held(values: Record<string, string>, key: string): string | undefined {
  const raw = values[key]
  return raw === undefined || raw === '' ? undefined : raw
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

/** One field of a card, in the form its row names. */
function PresetField({
  row,
  values,
}: {
  row: Row
  values: Record<string, string>
}) {
  const id = `preset-${row.key}`
  const value = held(values, row.key)
  if (row.kind === 'number') {
    return (
      <DefaultBoxField
        id={id}
        label={row.label}
        hint={row.hint}
        box={t('field-default')}
        numeric={row.integer}
        unit={row.unit}
        ticked={value === undefined}
        text={value ?? ''}
        disabled
        onTick={() => undefined}
        onText={() => undefined}
      />
    )
  }
  if (row.kind === 'text') {
    // No placeholder: an example in a field nobody can type into reads as a
    // value (criterion 87).
    return (
      <TextField
        id={id}
        label={row.label}
        hint={row.hint}
        value={value ?? ''}
        placeholder=""
        disabled
        onChange={() => undefined}
      />
    )
  }
  const select = shown(row, value)
  return (
    <SelectField
      id={id}
      label={row.label}
      hint={row.hint}
      options={select.options}
      value={select.value}
      disabled
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
