// The plain cards of a model profile's page (fe/briefs/02e-model-profile-cards.md,
// section 1.4): a card is a table of fields and each field is one row of data,
// the key it reads in the read's `values`, the strings keys of its label and
// hint, its control and, for a select, its options. A later slice adds rows;
// the renderer here does not change.
//
// Every control of a card is natively disabled (criterion 85): the page shows
// the file for it and sends nothing. 2h.1 makes the Embeddings card's two
// controls editable (criterion 130): the row says which with `editable`, and
// the page's form edits those and its save bar writes what changed. 2h.4 gives
// the same to the number fields of Resource / context (155), whose default box
// the page keeps in `boxed` (156, the mechanics of `boxed.ts`), and 155 leaves
// a select enabled only while another key's box is unticked; 2h.7 generalises
// that rule to `enabledWhen` (176: "enabled when a box is unticked, a box is
// checked, a file key is present"), on rows and on a card, and gives the
// Vision card its editable controls. 2h.8 gives the Hardware card's Fit trio
// the same, with the text of 184 that follows the two default boxes live, and
// the invalid `fit` of 185, which blocks the save as a client check does.
// 2h.5 gives the KV cache card the same (164: every field of the card enabled,
// always), and 132 decides what each of them sends (the state the control
// shows is the state the page is dirty against, an absent key included).
// 2h.9 gives the Speculative decoding card the same, and with it the group rule
// of 190 and 191: a group's fields move together, enabled by the read's
// `model-draft` and `spec-type`, two keys this page reads and never sends. A
// group the rule leaves off says why in the line of 192, its own `offNote`.
// Each field is the same Field the Server page uses, so the page looks the same
// before and after the save lands.
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
import { useState } from 'react'
import { boxText, boxedFrom, type Boxed } from './boxed.ts'
import Card from './Card.tsx'
import {
  CheckboxField,
  DefaultBoxField,
  Hint,
  HintToggle,
  SelectField,
  TextField,
} from './Field.tsx'
import {
  GpuFieldset,
  factsLine,
  factsRead,
  gpuFacts,
  type FactsState,
  type GpuCardProps,
  type GpuProbe,
} from './GpuSplit.tsx'
import { TensorPlacement, type TensorCardProps } from './TensorPlacement.tsx'
import { t } from './strings.ts'

/** A select's option: the value the file holds, the text the user reads. */
type Option = { value: string; text: string }

/** How a key's value is read as a boolean, the way llama-server reads it
 * (criterion 122):
 * - `flag`: an option without a value. The engine passes the negative flag for
 *   the four falsey words and the plain flag for any other value, the empty one
 *   included (`common_preset::to_args` and `common_arg_utils::is_falsey`).
 * - `truthy`: a model-profile-only key the router reads as truthy only, so on is the
 *   short list of words and every other value, the empty one included, is off
 *   (`is_truthy`, `tools/server/server-models.cpp`).
 * - `valued`: `[on|off|auto]`: its handler reads both lists and keeps any other
 *   value as the extra option of criterion 87.
 * - `fit`: `-fit [on|off]`, the one checkbox whose value the engine reads with
 *   the same two lists and refuses anything outside them, so such a value is an
 *   error and not a spelling (criterion 128). The key is absent for it, too,
 *   where the engine's own default is on (`common/common.h`). */
type Reading = 'flag' | 'truthy' | 'valued' | 'fit'

/** When a row's control is enabled, one rule of 2h.7's 176 (the
 * generalisation of 155's `needsUnticked`: "enabled when ..."). The control
 * is enabled while every rule of the row's list is met; a control a rule
 * disables is natively disabled and keeps the state it holds, so a change
 * made while it was enabled stays in the draft and is sent as for any other
 * editable row.
 * - `unticked`: the number field's default box is unticked (155: Unified KV
 *   sizes a cache shared by the slots, so it needs Parallel slots);
 * - `checked`: the checkbox row's box is ticked, live from the draft (176:
 *   Vision GPU needs Vision on GPU checked; 2h.8's 186: the Fit pair needs Fit
 *   to VRAM checked);
 * - `file`: the key is present in the read's values and not empty (176: the
 *   Vision card's controls do nothing without an MMProj, a key this page
 *   cannot edit). The Hardware card of 177 adds rows of its own. */
type EnableRule =
  | { rule: 'unticked'; key: string }
  | { rule: 'checked'; key: string }
  | { rule: 'file'; key: string }

/** What every row carries: the key and the strings keys of its texts. */
type RowBase = {
  /** The key of the read's `values`. */
  key: string
  label: string
  hint: string
  /** 2h.7's 176: the rules that enable this row's control (an absent list:
   * enabled whenever the row's card enables it). */
  enabledWhen?: EnableRule[]
  /** The reading of a checkbox or select row whose key is a boolean. Absent for
   * every other row: its value is not a boolean, so it keeps criterion 87. */
  reading?: Reading
  /** 2h.1's 130: the page's form edits this row's control and sends what
   * changed. Absent for every other row: its control stays natively disabled
   * (85), so the page only shows the file for it. */
  editable?: true
  /** Brief 14's split mode row: its hint is one paragraph, its line breaks kept
   * by CSS, so one read of the whole text reaches it. Absent: one paragraph per
   * line, as every other row writes it. */
  hintSingle?: true
}

/** A number: the default box and a text input showing the string as it is. The
 * unit, when the table gives one, sits after the input (criterion 89). */
type NumberRow = RowBase & {
  kind: 'number'
  integer: boolean
  unit?: string
  /** Brief 19, 19 G.13, 19 G.14: the row takes a slider beside its input, once
   * the model file's facts are read. `blocks`: a layer count whose top is every
   * block of the file, which is also what the field's default box reads; `moe`:
   * the layers kept on the CPU, whose top is the file's block count and whose
   * default box reads none. Absent: no slider (the draft's layer count). */
  slider?: 'blocks' | 'moe'
  /** Brief 19, 19 G.14: the row's state follows the model file's facts, read
   * beside the GPU card: a dense model has no experts for the key to keep on
   * the CPU, and a read that has not answered has not said. Every other state
   * opens the row; the reason paragraph right before it says which. */
  moeFacts?: true
}

/** A select: its options in the table's order, and the option an absent key
 * shows. */
type SelectRow = RowBase & {
  kind: 'select'
  options: Option[]
  absent: string
  /** 2h.6's 171 (133's row of the selects with no `default` option): every
   * option is a value the file holds, so every state sends its own value, the
   * absent one included. The absent option is only what the control shows for a
   * key the file lacks, and picking it back is no change (132). */
  ownValue?: true
  /** Brief 14's split mode row: a spelling of the value the file holds that the
   * row reads as one of its options (`layer` for the default the key leaves
   * out). Read before the option match, so the state and what the save sends
   * are that option's. */
  alias?: Record<string, string>
  /** Brief 25 GT5b.1 (the review's D1): the words the absent key means, in a
   * muted line under the select while the control is at `absent`, and the
   * select's description. A native select cannot wrap them, so they sit outside
   * it and its option keeps the shared `option-default`. Absent: no line. */
  unsetNote?: string
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
  /** 2h.6's 171 (133's checkbox rows): what the save sends while the box is
   * checked, and what it sends while it is not. Each key has its own spelling,
   * the one the configurator's `form.rs` writes: `jinja` and `mmproj-offload`
   * write `true` and `false`, `prefill-assistant` drops the key when it is on,
   * and `fit` writes `on` and `off`. A row of a later card carries its own. */
  onSend: string | null
  offSend: string | null
  /** A box the configurator only lets the user set under a rule (criterion
   * 108: Fit to VRAM sizes the context and the GPU layers, so it is allowed
   * while both are absent). Each field of `needsAbsent` is a key that must be
   * at default; when one is not, the text is `notAllowed` while the box is
   * unchecked and, once it is checked, the `setOne` text of 2h.8's 184 of the
   * key that is set, naming what the fit can still do, and `setAll` when every
   * one is. The box keeps showing the file's reading either way (criterion
   * 127). */
  allowed?: {
    needsAbsent: { key: string; setOne: string }[]
    notAllowed: string
    setAll: string
  }
}

/** A device select (criterion 103, brief 19 19 G.16 and 19 G.17): the empty
 * option, then the host's GPUs as the device read lists them, then the file's
 * device when no GPU names it. `empty` is the text of the empty option, which
 * each field words for its own device (110). `none` is the text of the `none`
 * option a device key takes for not being on a GPU: only Vision GPU has one. */
type DeviceRow = RowBase & {
  kind: 'device'
  empty: string
  none?: string
}

/** The GPUs fieldset of the GPU distribution card (brief 14): not a field of
 *  the table. Its rows are the host's GPUs and the ones the profile names, and
 *  it composes the `device` and `tensor-split` keys from them, so it writes
 *  nothing at the page's `onEdit` and sits beside the draft's keys. */
type GpuRow = RowBase & { kind: 'gpu' }

/** The rules of `override-tensor` (brief 21, 21 T.2): the Tensor placement
 *  card's one block, a list of rows the page owns, each composing part of the
 *  key. Like the GPUs fieldset it writes no `onEdit`: the key is the rows'. */
type TensorCardRow = RowBase & { kind: 'tensor' }

/** The model file's facts line of the GPU distribution card (brief 19, 19 G.2):
 *  one muted paragraph of the card, after its intro and before its first field,
 *  saying what the read of the model file answered. It reads no key of the
 *  file, and holds no control: its `key` is only the list's. */
type FactsRow = { kind: 'facts'; key: string }

/** A note between fields (criterion 90): the GUI's muted paragraph. Its `key`
 * is only the list's: a note reads no key of the file. `shownWithout` is the
 * key of 177's rule: the note speaks for the controls the `file` rule of 176
 * disables, so it sits where the rule is not met and goes once it is. */
type NoteRow = {
  kind: 'note'
  key: string
  text: string
  shownWithout?: string
  /** Brief 19, 19 G.14: the note shows in this state of the model file's facts
   * read and in no other. Absent: every state. */
  shownFacts?: FactsState
}

/** The drafter that switches a group of the Speculative decoding card on (2h.9's
 * 190, 191): `draft` for the group of a draft model file or of a drafter built
 * into the model, and the four n-gram entries of the `spec-type` list. */
type SpecGroup =
  'draft' | 'ngram-mod' | 'ngram-simple' | 'ngram-map-k' | 'ngram-map-k4v'

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
  /** 2h.9's 190 and 191: the drafter that enables the group's fields, all of
   * them together, read from the values the read gave (`model-draft` and
   * `spec-type`, two keys this page reads and never sends). An entry is matched
   * whole, on the split of 2-models' 75 that the Assets card shows
   * (`specTypeEntries`), so `ngram-map-k` is not `ngram-map-k4v`. Absent: the
   * card alone enables the group. */
  enabledBy?: SpecGroup
  /** 192: the line the group shows while its rule leaves it off, the text the
   * strings file gives (an n-gram group's names the group, a literal). Set
   * where `enabledBy` is. */
  offNote?: string
}

type Row =
  | NumberRow
  | SelectRow
  | TextRow
  | CheckboxRow
  | DeviceRow
  | GpuRow
  | TensorCardRow
  | FactsRow
  | NoteRow
  | GroupRow

/** The rows that are a field or a note: what a group holds, and what
 * `ModelProfileField` shows. */
export type FieldRow = Exclude<Row, GroupRow>

/** A card: its h3 and its fields, in the order of the GUI's page. */
type CardFields = {
  /** 2h.10's 196: the card's place in the page's jump list, as a slug. The
   * heading's id (`cardHeadingId`) is what the jump button reaches the card by,
   * so the jump list does not count the headings of the page. */
  id: string
  title: string
  /** 2h.7's 176: the rule of the whole card, the same `file` rule of 176 one
   * level up: every control of the card is enabled only while the key is
   * present in the read's values and not empty (the Vision card works only
   * with an MMProj, and this page cannot pick one). While the key is not,
   * `note` sits right after the card's h3, before the first field: a plain
   * paragraph in reading order (181), never a control's description. */
  needsFile?: { key: string; note: string }
  /** Brief 14: the card's intro, one paragraph per line, right after the h3.
   *  22 D4.31 as its D1 review (2026-11-27) amends it: the intro is the card's
   *  one sentence, and what the card goes on to say is `about`. */
  introLines?: string[]
  /** 22 D4.31 review D1: the words behind the card's About, the rest of the
   *  intro the card used to show, `\n` breaks included. */
  about?: string
  rows: Row[]
}

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
 * strings, then the literals the file writes. The unset entry is the shared
 * `option-default` (brief 25 GT5b.1, the review's D1: a native select clips the
 * long words at 320 px), and the row says what an unset key means in a line
 * under the select. The value it stands for stays `default`, so the key is
 * still left out of the file. */
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
    // Brief 14 (fe/briefs/14-gpu-distribution.md), in the place 02e's 83
    // reserves: after the page's Assets card, before the Hardware card. Its one
    // editable field is the split mode select; the GPUs beside it are not a
    // field of the table, they compose `device` and `tensor-split`.
    id: 'gpu',
    title: t('model-profile-gpu-distribution'),
    introLines: t('model-profile-gpu-distribution-intro').split('\n'),
    about: t('model-profile-gpu-distribution-about'),
    rows: [
      { kind: 'facts', key: 'model-info' },
      {
        kind: 'select',
        key: 'split-mode',
        label: t('model-profile-split-mode'),
        hint: t('model-profile-split-mode-hint'),
        // The four values of the table, with the default the key leaves out
        // first (GD1.2). `layer` is that default spelled out (GD1.2).
        options: [
          { value: '', text: t('model-profile-split-layer') },
          { value: 'none', text: t('model-profile-split-none') },
          { value: 'tensor', text: t('model-profile-split-tensor') },
          { value: 'row', text: t('model-profile-split-row') },
        ],
        absent: '',
        alias: { layer: '' },
        editable: true,
      },
      { kind: 'gpu', key: 'device', label: t('model-profile-gpus'), hint: '' },
    ],
  },
  {
    // Brief 21 (fe/briefs/21-tensor-placement.md), 21 T.2: after the GPU
    // distribution card, before the Hardware card. Its one row is the rules
    // list, which composes `override-tensor` from its rows.
    id: 'tensor',
    title: t('model-profile-tensor-placement'),
    introLines: t('model-profile-tensor-placement-intro').split('\n'),
    about: t('model-profile-tensor-placement-about'),
    rows: [
      {
        kind: 'tensor',
        key: 'override-tensor',
        label: t('model-profile-ot-rules'),
        hint: '',
      },
    ],
  },
  {
    id: 'hardware',
    title: t('model-profile-hardware-config'),
    // 2h.7's 175: the seven rows the table enables always, and the draft pair
    // behind `model-draft` (177), all editable. The MoE pair stays disabled
    // until 2g reads the model (178); the Fit trio joined with 2h.8.
    rows: [
      {
        kind: 'number',
        integer: true,
        key: 'n-gpu-layers',
        label: t('model-profile-gpu-layers'),
        hint: t('model-profile-gpu-layers-hint'),
        editable: true,
        // 19 G.13: the slider reaches every block of the model file.
        slider: 'blocks',
      },
      // 178: MoE cache stays disabled, and its note sits right before it.
      // 19 G.14: MoE CPU layers became editable, and its state follows the
      // model file's facts, so the reason a read gives for closing it sits
      // right before the field: while the read runs, and for a dense file,
      // which has no experts for the key to keep on the CPU. A read that
      // answered a reason or nothing leaves the field open and says nothing.
      {
        kind: 'note',
        key: 'moe-reading-note',
        text: t('model-profile-moe-reading'),
        shownFacts: 'reading',
      },
      {
        kind: 'note',
        key: 'moe-dense-note',
        text: t('model-profile-moe-dense'),
        shownFacts: 'dense',
      },
      {
        kind: 'number',
        integer: true,
        key: 'n-cpu-moe',
        label: t('model-profile-moe-cpu-layers'),
        hint: t('model-profile-moe-cpu-layers-hint'),
        editable: true,
        moeFacts: true,
        // 19 G.14: the slider only for a file the read answered experts of.
        slider: 'moe',
      },
      // 178: the note of the row that stays disabled, right before it.
      {
        kind: 'note',
        key: 'moe-later-note',
        text: t('model-profile-moe-later'),
      },
      {
        kind: 'number',
        integer: true,
        unit: 'MiB',
        key: 'moe-cache-mib',
        label: t('model-profile-moe-cache'),
        hint: t('model-profile-moe-cache-hint'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'n-cpu-ffn',
        label: t('model-profile-dense-ffn-cpu-layers'),
        hint: t('model-profile-dense-ffn-cpu-layers-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'op-offload',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('model-profile-op-offload'),
        hint: t('model-profile-op-offload-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'repack',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('model-profile-repack'),
        hint: t('model-profile-repack-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'no-host',
        reading: 'flag',
        options: OFF_ON,
        absent: 'false',
        label: t('model-profile-no-host-buffer'),
        hint: t('model-profile-no-host-buffer-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'lazy-mode',
        options: LAZY_MODES,
        absent: 'default',
        label: t('model-profile-lazy-tensors'),
        hint: t('model-profile-lazy-tensors-hint'),
        editable: true,
      },
      {
        // 180: an enabled load-mode select matches the file's value exactly.
        kind: 'select',
        key: 'load-mode',
        options: LOAD_MODES,
        absent: 'default',
        // 25 GT5b.1: an unset key is not llama.cpp's `auto` on a ROCm instance,
        // so the words the option cannot carry sit under the select.
        unsetNote: t('model-profile-load-mode-default'),
        label: t('model-profile-load-mode'),
        hint: t('model-profile-load-mode-hint'),
        editable: true,
      },
      {
        kind: 'checkbox',
        key: 'fit',
        reading: 'fit',
        toggle: t('model-profile-fit-allowed'),
        absent: true,
        onSend: 'on',
        offSend: 'off',
        allowed: {
          needsAbsent: [
            { key: 'ctx-size', setOne: t('model-profile-fit-layers-only') },
            {
              key: 'n-gpu-layers',
              setOne: t('model-profile-fit-context-only'),
            },
          ],
          notAllowed: t('model-profile-fit-not-allowed'),
          setAll: t('model-profile-fit-nothing'),
        },
        label: t('model-profile-fit-to-vram'),
        hint: t('model-profile-fit-to-vram-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        key: 'fit-ctx',
        label: t('model-profile-fit-min-context'),
        hint: t('model-profile-fit-min-context-hint'),
        editable: true,
        enabledWhen: [{ rule: 'checked', key: 'fit' }],
      },
      {
        kind: 'text',
        unit: 'MiB',
        key: 'fit-target',
        label: t('model-profile-fit-free-vram'),
        hint: t('model-profile-fit-free-vram-hint'),
        editable: true,
        enabledWhen: [{ rule: 'checked', key: 'fit' }],
      },
      // 177: the pair works only with a Draft model file, which this page does
      // not pick, so its note sits before Draft device while the file has none.
      {
        kind: 'note',
        key: 'draft-file-off-note',
        text: t('model-profile-draft-file-off'),
        shownWithout: 'model-draft',
      },
      {
        kind: 'device',
        key: 'device-draft',
        empty: t('model-profile-device-same-as-model'),
        label: t('model-profile-draft-device'),
        hint: t('model-profile-draft-device-hint'),
        editable: true,
        enabledWhen: [{ rule: 'file', key: 'model-draft' }],
      },
      {
        kind: 'number',
        integer: true,
        key: 'n-gpu-layers-draft',
        label: t('model-profile-draft-gpu-layers'),
        hint: t('model-profile-draft-gpu-layers-hint'),
        editable: true,
        enabledWhen: [{ rule: 'file', key: 'model-draft' }],
      },
    ],
  },
  {
    id: 'resource-context',
    title: t('model-profile-resource-context'),
    rows: [
      {
        kind: 'number',
        integer: true,
        key: 'ctx-size',
        label: t('model-profile-context-size'),
        hint: t('model-profile-context-size-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        key: 'parallel',
        label: t('model-profile-parallel-slots'),
        hint: t('model-profile-parallel-slots-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'kv-unified',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('model-profile-unified-kv'),
        hint: t('model-profile-unified-kv-hint'),
        editable: true,
        enabledWhen: [{ rule: 'unticked', key: 'parallel' }],
      },
      {
        kind: 'number',
        integer: true,
        key: 'kv-unified-per-slot',
        label: t('model-profile-per-slot-context'),
        hint: t('model-profile-per-slot-context-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        key: 'batch-size',
        label: t('model-profile-batch-size'),
        hint: t('model-profile-batch-size-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        key: 'ubatch-size',
        label: t('model-profile-u-batch-size'),
        hint: t('model-profile-u-batch-size-hint'),
        editable: true,
      },
    ],
  },
  {
    id: 'embeddings',
    title: t('model-profile-embeddings'),
    rows: [
      {
        kind: 'select',
        key: 'embeddings',
        reading: 'flag',
        options: OFF_ON,
        absent: 'false',
        label: t('model-profile-embedding-mode'),
        hint: t('model-profile-embedding-mode-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'pooling',
        options: [
          { value: 'default', text: t('option-default') },
          { value: 'none', text: t('option-none') },
          { value: 'mean', text: t('option-mean') },
          // Not a strings key: a literal of the Embeddings table of
          // fe/briefs/02e-model-profile-cards.md section 1.4.
          { value: 'cls', text: 'cls' },
          { value: 'last', text: t('option-last') },
          { value: 'rank', text: t('option-rank') },
        ],
        absent: 'default',
        label: t('model-profile-pooling'),
        hint: t('model-profile-pooling-hint'),
        editable: true,
      },
    ],
  },
  {
    id: 'kv-cache',
    title: t('model-profile-kv-cache'),
    rows: [
      {
        kind: 'select',
        key: 'cache-type-k',
        options: CACHE_TYPES,
        absent: 'default',
        label: t('model-profile-k-cache-type'),
        hint: t('model-profile-k-cache-type-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'cache-type-v',
        options: CACHE_TYPES,
        absent: 'default',
        label: t('model-profile-v-cache-type'),
        hint: t('model-profile-v-cache-type-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'flash-attn',
        reading: 'valued',
        options: TRISTATE,
        absent: 'default',
        label: t('model-profile-flash-attention'),
        hint: t('model-profile-flash-attention-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        unit: 'MiB',
        key: 'cache-ram',
        label: t('model-profile-cache-ram'),
        hint: t('model-profile-cache-ram-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        key: 'cache-reuse',
        label: t('model-profile-cache-reuse'),
        hint: t('model-profile-cache-reuse-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        key: 'ctx-checkpoints',
        label: t('model-profile-context-checkpoints'),
        hint: t('model-profile-context-checkpoints-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        key: 'checkpoint-min-step',
        label: t('model-profile-checkpoint-step'),
        hint: t('model-profile-checkpoint-step-hint'),
        editable: true,
      },
    ],
  },
  {
    id: 'chat-reasoning',
    title: t('model-profile-chat-reasoning'),
    rows: [
      {
        kind: 'checkbox',
        key: 'jinja',
        reading: 'flag',
        toggle: t('model-profile-chat-template-toggle'),
        absent: true,
        onSend: 'true',
        offSend: 'false',
        label: t('model-profile-chat-template'),
        hint: t('model-profile-chat-template-hint'),
        editable: true,
      },
      {
        kind: 'checkbox',
        key: 'prefill-assistant',
        reading: 'flag',
        toggle: t('model-profile-prefill-assistant-toggle'),
        absent: true,
        onSend: null,
        offSend: 'false',
        label: t('model-profile-prefill-assistant'),
        hint: t('model-profile-prefill-assistant-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'reasoning',
        options: REASONING,
        absent: 'auto',
        ownValue: true,
        label: t('model-profile-reasoning'),
        hint: t('model-profile-reasoning-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'reasoning-format',
        options: REASONING_FORMAT,
        absent: 'auto',
        ownValue: true,
        label: t('model-profile-reasoning-format'),
        hint: t('model-profile-reasoning-format-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'reasoning-effort',
        options: EFFORT,
        absent: 'default',
        label: t('model-profile-reasoning-effort'),
        hint: t('model-profile-reasoning-effort-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'reasoning-preserve',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('model-profile-keep-past-thinking'),
        hint: t('model-profile-keep-past-thinking-hint'),
        editable: true,
      },
      {
        // The note of 90, at the GUI's place (MP 933-938).
        kind: 'note',
        key: 'output-budget-note',
        text: t('model-profile-output-budget-note'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'reasoning-budget',
        label: t('model-profile-thinking-budget'),
        hint: t('model-profile-thinking-budget-hint'),
        editable: true,
      },
      {
        kind: 'text',
        key: 'reasoning-budget-message',
        label: t('model-profile-budget-message'),
        hint: t('model-profile-budget-message-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: true,
        key: 'n-predict',
        label: t('model-profile-max-output'),
        hint: t('model-profile-max-output-hint'),
        editable: true,
      },
    ],
  },
  {
    id: 'sampling-overrides',
    title: t('model-profile-sampling-overrides'),
    rows: [
      {
        kind: 'number',
        integer: false,
        key: 'temp',
        label: t('model-profile-temperature'),
        hint: t('model-profile-temperature-hint'),
        editable: true,
      },
      {
        // Not strings keys: Top-K, Top-P and Min-P are acronyms, so their
        // labels are literals (table 97).
        kind: 'number',
        integer: true,
        key: 'top-k',
        label: 'Top-K',
        hint: t('model-profile-top-k-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: false,
        key: 'top-p',
        label: 'Top-P',
        hint: t('model-profile-top-p-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: false,
        key: 'min-p',
        label: 'Min-P',
        hint: t('model-profile-min-p-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: false,
        key: 'repeat-penalty',
        label: t('model-profile-repeat-penalty'),
        hint: t('model-profile-repeat-penalty-hint'),
        editable: true,
      },
      {
        kind: 'number',
        integer: false,
        key: 'presence-penalty',
        label: t('model-profile-presence-penalty'),
        hint: t('model-profile-presence-penalty-hint'),
        editable: true,
      },
      {
        kind: 'select',
        key: 'backend-sampling',
        reading: 'flag',
        options: TRISTATE,
        absent: 'default',
        label: t('model-profile-backend-sampling'),
        hint: t('model-profile-backend-sampling-hint'),
        editable: true,
      },
    ],
  },
  {
    id: 'advanced',
    title: t('model-profile-advanced'),
    rows: [
      {
        kind: 'select',
        key: 'load-on-startup',
        reading: 'truthy',
        options: OFF_ON,
        absent: 'false',
        label: t('model-profile-load-on-startup'),
        hint: t('model-profile-load-on-startup-hint'),
        editable: true,
      },
      {
        kind: 'text',
        key: 'chat-template-kwargs',
        label: t('model-profile-chat-kwargs'),
        hint: t('model-profile-chat-kwargs-hint'),
        editable: true,
      },
    ],
  },
  {
    id: 'vision',
    title: t('model-profile-vision'),
    // 176: the card works only with an MMProj (the read's value: this page
    // does not edit `mmproj`). Every row carries the same rule; the card
    // shows its note while the rule is not met.
    needsFile: { key: 'mmproj', note: t('model-profile-vision-off') },
    rows: [
      {
        kind: 'checkbox',
        key: 'mmproj-offload',
        reading: 'flag',
        toggle: t('model-profile-vision-offload-toggle'),
        absent: true,
        onSend: 'true',
        offSend: 'false',
        label: t('model-profile-vision-on-gpu'),
        hint: t('model-profile-vision-on-gpu-hint'),
        editable: true,
        enabledWhen: [{ rule: 'file', key: 'mmproj' }],
      },
      {
        kind: 'device',
        key: 'mmproj-device',
        empty: t('model-profile-device-follows-model'),
        // 19 G.17: the file's `none` (the image encoder stays off the GPUs) is
        // one of the options, never an extra one the list has to carry.
        none: t('model-profile-device-none'),
        label: t('model-profile-vision-gpu'),
        hint: t('model-profile-vision-gpu-hint'),
        editable: true,
        // 176: Vision GPU also needs Vision on GPU checked, live from the
        // draft (the switch sits just above, so it needs no note of its own).
        enabledWhen: [
          { rule: 'file', key: 'mmproj' },
          { rule: 'checked', key: 'mmproj-offload' },
        ],
      },
      {
        // The note of 90, at the GUI's place (MP 1099-1104).
        kind: 'note',
        key: 'image-tokens-note',
        text: t('model-profile-image-tokens-note'),
      },
      {
        kind: 'number',
        integer: true,
        key: 'image-min-tokens',
        label: t('model-profile-min-image-tokens'),
        hint: t('model-profile-min-image-tokens-hint'),
        editable: true,
        enabledWhen: [{ rule: 'file', key: 'mmproj' }],
      },
      {
        kind: 'number',
        integer: true,
        key: 'image-max-tokens',
        label: t('model-profile-max-image-tokens'),
        hint: t('model-profile-max-image-tokens-hint'),
        editable: true,
        enabledWhen: [{ rule: 'file', key: 'mmproj' }],
      },
    ],
  },
  {
    id: 'speculative-decoding',
    title: t('model-profile-speculative-decoding'),
    rows: [
      {
        kind: 'group',
        key: 'draft-model',
        title: t('model-profile-draft-model-group'),
        // 190: the group of a draft model file or of a drafter built into the
        // model, enabled while the model profile has one.
        enabledBy: 'draft',
        offNote: t('model-profile-draft-group-off'),
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-draft-n-max',
            label: t('model-profile-draft-n-max'),
            hint: t('model-profile-draft-n-max-hint'),
            editable: true,
          },
          {
            // The card's only float row (table 112).
            kind: 'number',
            integer: false,
            key: 'spec-draft-p-min',
            label: t('model-profile-draft-p-min'),
            hint: t('model-profile-draft-p-min-hint'),
            editable: true,
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-draft-n-min',
            label: t('model-profile-draft-n-min'),
            hint: t('model-profile-draft-n-min-hint'),
            editable: true,
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-draft-n-ubatch',
            label: t('model-profile-draft-n-ubatch'),
            hint: t('model-profile-draft-n-ubatch-hint'),
            editable: true,
          },
          {
            // The note of 90, at the GUI's place: before the two cache types
            // it speaks of.
            kind: 'note',
            key: 'draft-cache-note',
            text: t('model-profile-draft-cache-note'),
          },
          {
            kind: 'select',
            key: 'spec-draft-type-k',
            options: CACHE_TYPES,
            absent: 'default',
            label: t('model-profile-draft-k-cache-type'),
            hint: t('model-profile-draft-k-cache-type-hint'),
            editable: true,
          },
          {
            kind: 'select',
            key: 'spec-draft-type-v',
            options: CACHE_TYPES,
            absent: 'default',
            label: t('model-profile-draft-v-cache-type'),
            hint: t('model-profile-draft-v-cache-type-hint'),
            editable: true,
          },
        ],
      },
      {
        // The four groups are the speculator names the file writes, so they are
        // literals and not strings (114, D7). Each group's rows share their
        // labels with another group's: the group tells them apart (113). 191:
        // each group is enabled while `spec-type` lists its name, and while it
        // does not the group says why (192).
        kind: 'group',
        key: 'ngram-mod',
        title: 'ngram-mod',
        enabledBy: 'ngram-mod',
        offNote: t('model-profile-ngram-off', 'ngram-mod'),
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-mod-n-match',
            label: t('model-profile-match-length'),
            hint: t('model-profile-match-length-hint'),
            editable: true,
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-mod-n-min',
            label: t('model-profile-min-draft'),
            hint: t('model-profile-min-draft-hint'),
            editable: true,
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-mod-n-max',
            label: t('model-profile-max-draft'),
            hint: t('model-profile-max-draft-hint'),
            editable: true,
          },
        ],
      },
      {
        kind: 'group',
        key: 'ngram-simple',
        title: 'ngram-simple',
        enabledBy: 'ngram-simple',
        offNote: t('model-profile-ngram-off', 'ngram-simple'),
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-simple-size-n',
            label: t('model-profile-lookup-n-gram'),
            hint: t('model-profile-ngram-simple-lookup-hint'),
            editable: true,
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-simple-size-m',
            label: t('model-profile-draft-m-gram'),
            hint: t('model-profile-ngram-simple-draft-hint'),
            editable: true,
          },
        ],
      },
      {
        kind: 'group',
        key: 'ngram-map-k',
        title: 'ngram-map-k',
        enabledBy: 'ngram-map-k',
        offNote: t('model-profile-ngram-off', 'ngram-map-k'),
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k-size-n',
            label: t('model-profile-lookup-n-gram'),
            hint: t('model-profile-ngram-map-lookup-hint'),
            editable: true,
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k-size-m',
            label: t('model-profile-draft-m-gram'),
            hint: t('model-profile-ngram-map-draft-hint'),
            editable: true,
          },
        ],
      },
      {
        kind: 'group',
        key: 'ngram-map-k4v',
        title: 'ngram-map-k4v',
        enabledBy: 'ngram-map-k4v',
        offNote: t('model-profile-ngram-off', 'ngram-map-k4v'),
        rows: [
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k4v-size-n',
            label: t('model-profile-lookup-n-gram'),
            hint: t('model-profile-ngram-map-values-lookup-hint'),
            editable: true,
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k4v-size-m',
            label: t('model-profile-draft-m-gram'),
            hint: t('model-profile-ngram-map-values-draft-hint'),
            editable: true,
          },
          {
            kind: 'number',
            integer: true,
            key: 'spec-ngram-map-k4v-min-hits',
            label: t('model-profile-min-hits'),
            hint: t('model-profile-min-hits-hint'),
            editable: true,
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
    ? t('model-profile-written-empty')
    : t('model-profile-written-as', trimmed)
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
      error: falsey || truthy ? undefined : t('model-profile-fit-invalid'),
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
 * (criterion 87). A row whose key is a boolean of table 122 holds it trimmed,
 * as the INI grammar trims it (criterion 124a). */
function shown(
  row: SelectRow,
  value: string | undefined,
): {
  options: Option[]
  value: string
} {
  if (value === undefined) return { options: row.options, value: row.absent }
  const hit = row.options.find((o) => o.value === value)
  if (hit !== undefined) return { options: row.options, value: hit.value }
  const extra = row.reading === undefined ? value : iniTrim(value)
  return {
    options: [...row.options, { value: extra, text: extra }],
    value: extra,
  }
}

/** The select of a row as the form's `values` leave it: the state the control
 * shows (2h.1's 132: the state the page compares, so the extra option of 87
 * that a read value became is a state like any other). A key the file does not
 * hold has the row's absent option selected, so that option is its state: the
 * control shows that one option for the absence, and picking it is no change
 * (133 sends the key's removal, which is what the file already is). A value one
 * of the options carries is that option, the `default` of a tri-state row
 * included (133): only a spelling no option carries is read as the row's key,
 * which for a flag row is the on or off the control shows (125). */
export function selectState(
  row: SelectRow,
  values: Record<string, string>,
): string {
  const value = heldFor(row, values)
  if (value === undefined) return row.absent
  // Brief 14 GD1.2: a spelling the row aliases to one of its options is that
  // option; any other spelling of it is the file's own value, shown as an
  // extra last option.
  const alias = row.alias?.[value]
  if (alias !== undefined) return alias
  if (row.options.some((o) => o.value === value)) return value
  const state =
    row.reading === undefined ? undefined : readAs(row.reading, value)
  if (state !== undefined && state.on !== undefined)
    return state.on ? 'true' : 'false'
  return shown(row, value).value
}

/** The options of an enabled select, built from the state of the value the read
 * gave (132a): the row's own options, plus the extra option of 87 carrying the
 * state when no option carries it. A flag's state is always one of them (125),
 * so its select lists no extra option; a valued key with a spelling in neither
 * list, and a row whose key is not a boolean, get the one. */
function optionsAt(row: SelectRow, state: string): Option[] {
  return row.options.some((o) => o.value === state)
    ? row.options
    : [...row.options, { value: state, text: state }]
}

/** What the form sends for a select at `state` (133): a row whose options are
 * all values the file holds sends the option itself, the absent one included;
 * every other row's absent option is the state of a key the file does not hold
 * (the off of an off / on key, the `default` of a tri-state one), so it is the
 * key's removal. Every other state is the option. */
export function selectSend(row: SelectRow, state: string): string | null {
  if (row.ownValue === true) return state
  return state === row.absent ? null : state
}

/** The checkbox of a row as the form's `values` leave it: the state the control
 * shows (2h.1's 132: the state the page compares), the row's `absent` for a key
 * the file does not hold. */
export function checkboxState(
  row: CheckboxRow,
  values: Record<string, string>,
): boolean {
  const value = heldFor(row, values)
  const state =
    row.reading === undefined || value === undefined
      ? undefined
      : readAs(row.reading, value)
  return state === undefined ? row.absent : state.on === true
}

/** What the form sends for a checkbox at that state (133's checkbox rows): the
 * row's own spelling for each side of the box. */
export function checkboxSend(
  row: CheckboxRow,
  checked: boolean,
): string | null {
  return checked ? row.onSend : row.offSend
}

/** The checkbox of a row as the state the page is dirty against (132), which
 * for the `fit` reading of 2h.8's 185 is a state of its own when the value is
 * one the engine refuses (128): the field loads unchecked and invalid, and
 * that is not the state an `off` value shows, so a click off it is a change
 * even when the box ends where an off value's box sits. */
export function checkboxDirty(
  row: CheckboxRow,
  values: Record<string, string>,
): 'invalid' | 'on' | 'off' {
  const value = heldFor(row, values)
  const state =
    row.reading === undefined || value === undefined
      ? undefined
      : readAs(row.reading, value)
  if (state?.error !== undefined) return 'invalid'
  return (state === undefined ? row.absent : state.on === true) ? 'on' : 'off'
}

/** 2h.8's 185: the field error of an editable checkbox: a value the engine
 * refuses to start with, which the manager's checks do not read, so the page's
 * save must. Read from the draft, so the first click clears it. */
export function checkboxError(
  row: CheckboxRow,
  values: Record<string, string>,
): string | undefined {
  const value = heldFor(row, values)
  return row.reading === undefined || value === undefined
    ? undefined
    : readAs(row.reading, value).error
}

/** The device select of a row as the form's `values` leave it (2h.7's 179):
 * the device the key holds, and the empty string for a key the file lacks,
 * the value of the empty option of 103. */
export function deviceState(
  row: DeviceRow,
  values: Record<string, string>,
): string {
  return held(values, row.key) ?? ''
}

/** What the form sends for a device select at that state (179): picking the
 * empty option sends the key's removal, every other state the device. */
export function deviceSend(state: string): string | null {
  return state === '' ? null : state
}

/** The rows of a card list, in the cards' order, a group's rows in place of the
 *  group: the table nests them, so a caller asks a list which keys it owns. */
function rowsOf(cards: CardFields[]): FieldRow[] {
  const rows: FieldRow[] = []
  for (const card of cards)
    for (const row of card.rows)
      for (const one of row.kind === 'group' ? row.rows : [row]) rows.push(one)
  return rows
}

/** The rows of every card the page draws, in the page's order. */
function pageFieldRows(): FieldRow[] {
  // `CARDS` holds every card the form knows, the placement ones included for
  // the entry page of brief 26. A key that moved to the instance profile has
  // no card on this page, so no field and no row: the lists a page reads are
  // the rows of `pageCards` (brief 22, IP5 review A1).
  return rowsOf(pageCards())
}

/** The select rows the form edits (130), in the page's order. */
export function pageEditableRows(): SelectRow[] {
  return pageFieldRows().filter(
    (one): one is SelectRow => one.kind === 'select' && one.editable === true,
  )
}

/** The checkbox rows the form edits (2h.6's 170), in the page's order. */
export function pageCheckboxRows(): CheckboxRow[] {
  return pageFieldRows().filter(
    (one): one is CheckboxRow =>
      one.kind === 'checkbox' && one.editable === true,
  )
}

/** The device rows the form edits (2h.7's 175, 179), in the page's order. */
export function pageDeviceRows(): DeviceRow[] {
  return pageFieldRows().filter(
    (one): one is DeviceRow => one.kind === 'device' && one.editable === true,
  )
}

/** The number rows the form edits (2h.4's 155), in the page's order: the order
 * their fields sit in, which is the order a save reports its failures in. */
export function pageNumberRows(): NumberRow[] {
  return pageFieldRows().filter(
    (one): one is NumberRow => one.kind === 'number' && one.editable === true,
  )
}

/** The text rows the form edits (2h.5's 166), in the page's order. */
export function pageTextRows(): TextRow[] {
  return pageFieldRows().filter(
    (one): one is TextRow => one.kind === 'text' && one.editable === true,
  )
}

/** The rows the save checks (157, 166, and 2h.8's 185 for a checkbox whose
 * value the engine refuses), in the order the fields sit on screen, which is
 * the order a save reports its first failure in. The cards the page draws, not
 * every card (brief 22, IP5.5 and IP5.7): a key that moved to the instance
 * profile has no field on this page, so a value of one that sits in the file is
 * checked by the entry that shows it, and a failure marks no control the page
 * does not draw. */
export function checkRows(): (NumberRow | TextRow | CheckboxRow)[] {
  return rowsOf(pageCards()).filter(
    (one): one is NumberRow | TextRow | CheckboxRow =>
      (one.kind === 'number' ||
        one.kind === 'text' ||
        one.kind === 'checkbox') &&
      one.editable === true,
  )
}

/** 2h.4's 157: the range of every model profile integer key, the brief's table: the
 * bounds the manager's `checks.rs` `MODEL_PROFILE_INT_KEYS` applies. `null` is no
 * bound; every key's value is at most `I32_MAX`, the largest number the file
 * writes. A later card's keys are here before the card is editable. */
function bounded(keys: string[], min: number | null, max: number | null) {
  return Object.fromEntries(keys.map((key) => [key, { min, max }]))
}

const INT_KEYS: Record<string, { min: number | null; max: number | null }> = {
  // Any whole number: the table's first row, and the layer counts of its last.
  // The two layer counts of that row also take the words of WORD_VALUES,
  // checked and sent trimmed, so a word is never read as an out-of-range value.
  ...bounded(
    [
      'cache-ram',
      'reasoning-budget',
      'n-predict',
      'top-k',
      'n-gpu-layers',
      'n-cpu-ffn',
      'n-gpu-layers-draft',
    ],
    null,
    null,
  ),
  ...bounded(
    [
      'ctx-size',
      'spec-draft-n-min',
      'spec-draft-n-ubatch',
      'moe-cache-mib',
      'ctx-checkpoints',
      'checkpoint-min-step',
    ],
    0,
    null,
  ),
  ...bounded(
    [
      'parallel',
      'kv-unified-per-slot',
      'batch-size',
      'ubatch-size',
      'cache-reuse',
      'fit-ctx',
      'image-min-tokens',
      'image-max-tokens',
      'spec-draft-n-max',
    ],
    1,
    null,
  ),
  ...bounded(
    [
      'spec-ngram-mod-n-match',
      'spec-ngram-simple-size-n',
      'spec-ngram-simple-size-m',
      'spec-ngram-map-k-size-n',
      'spec-ngram-map-k-size-m',
      'spec-ngram-map-k4v-size-n',
      'spec-ngram-map-k4v-size-m',
    ],
    1,
    1024,
  ),
  ...bounded(['spec-ngram-mod-n-min', 'spec-ngram-mod-n-max'], 0, 1024),
  'spec-ngram-map-k4v-min-hits': { min: 1, max: 65535 },
  // Brief 19's 19 G.14: MoE CPU layers joined the editable keys, with the
  // range the manager's `checks.rs` gives it.
  'n-cpu-moe': { min: 0, max: 4096 },
}

/** 2h.5's 165: the model profile float keys, the ones the decimal check of 165
 * applies to, the manager's `checks.rs` `MODEL_PROFILE_FLOAT_KEYS`. A later
 * card's keys are added here before its card is editable, as with `INT_KEYS`. */
const FLOAT_KEYS: string[] = [
  'temp',
  'top-p',
  'min-p',
  'repeat-penalty',
  'presence-penalty',
  'spec-draft-p-min',
]

/** The text of a float field (165), read on the text trimmed and with every `,`
 * turned into `.`: an optional sign, digits with at most one `.`, an optional
 * exponent such as `e-3`. A regular expression on the text, because `Number()`
 * and `parseFloat` accept `Infinity`, hex and trailing junk. */
const DECIMAL_TEXT = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/

/** A float field's text as the check reads it and as the save sends it (165). */
function decimalText(text: string): string {
  return text.trim().replace(/,/g, '.')
}

/** The largest number these keys write (the configurator's `emit_i32`). */
const I32_MAX = 2147483647

/** The smallest number these keys write (the configurator's `emit_i32`). */
const I32_MIN = -2147483648

/** The text of a number field: an optional sign, digits, nothing else. */
const INT_TEXT = /^[+-]?\d+$/

/** The number a number field's text gives, `null` for a text that is no whole
 * number. */
function intValue(text: string): number | null {
  const s = text.trim()
  return INT_TEXT.test(s) ? Number(s) : null
}

/** 2h.7's 157: the two words a layer count takes as its whole value, by key:
 * llama.cpp reads both and neither the bff nor the manager checks these keys,
 * so a file holding `all` must not block a save. Checked on the trimmed text
 * and sent trimmed, so the space around a typed word never reaches the file. */
const WORD_VALUES: Record<string, string[]> = {
  'n-gpu-layers': ['auto', 'all'],
  'n-gpu-layers-draft': ['auto', 'all'],
}

/** The error of a number field's text (157): the trimmed text must be a whole
 * number within the key's range, and for a float key a finite decimal (165). A
 * ticked box says nothing: the save sends no value for it, and the manager
 * checks nothing either. A key that takes the words of `WORD_VALUES` names them
 * in its refusal (2h.8's 182a), so the general text of the other keys stays
 * untouched. */
export function numberError(
  row: NumberRow,
  b: Boxed | undefined,
): string | undefined {
  if (b === undefined || b.ticked) return undefined
  if (FLOAT_KEYS.includes(row.key)) {
    const s = decimalText(b.text)
    return DECIMAL_TEXT.test(s) && Number.isFinite(Number(s))
      ? undefined
      : t('field-error-decimal')
  }
  const r = INT_KEYS[row.key]
  if (r === undefined) return undefined
  if (WORD_VALUES[row.key]?.includes(b.text.trim())) return undefined
  const n = intValue(b.text)
  if (
    n !== null &&
    n >= I32_MIN &&
    n <= I32_MAX &&
    (r.min === null || n >= r.min) &&
    (r.max === null || n <= r.max)
  )
    return undefined
  if (WORD_VALUES[row.key] !== undefined) return t('field-error-gpu-layers')
  return r.min === null
    ? t('field-error-whole-number')
    : r.max === null
      ? t('field-error-number-min', String(r.min))
      : t('field-error-number', String(r.min), String(r.max))
}

/** The keyboard a number field asks for (158): `numeric` only for a key whose
 * range starts at 0 or above, since a phone's numeric keyboard may have no
 * minus key. The keys of the table's rows with no minimum, and every float,
 * get none. Read from `INT_KEYS`, the same table 157 checks the value against. */
function numericMode(row: NumberRow): boolean {
  const min = INT_KEYS[row.key]?.min ?? null
  return row.integer && min !== null && min >= 0
}

/** What a number field sends (157): the number in decimal, with no sign for a
 * positive value and no leading zeros; a ticked box sends the key's removal. A
 * float key sends the text as the check read it (165), so the spelling the user
 * typed survives: `1e-3` stays `1e-3`. One of a key's words of 157 is sent
 * trimmed. */
export function numberSend(row: NumberRow, b: Boxed): string | null {
  if (b.ticked) return null
  if (FLOAT_KEYS.includes(row.key)) return decimalText(b.text)
  const words = WORD_VALUES[row.key]
  if (words !== undefined) {
    const word = b.text.trim()
    if (words.includes(word)) return word
  }
  const n = intValue(b.text)
  return n === null ? b.text : String(n)
}

/** 2h.5's 166: the checks of the editable text fields, keyed by INI key, the
 * data a later card extends with its own text keys. Each check reads the
 * trimmed text and answers the field's error, `undefined` when the text
 * passes. The empty text is checked by nothing, and a key with no entry here
 * has no check. */
const TEXT_KEYS: Record<string, (text: string) => string | undefined> = {
  'chat-template-kwargs': (text) =>
    // The comment markers first (166): a value holding one is cut by the INI
    // grammar before the engine ever reads it as JSON.
    text.includes(';') || text.includes('#')
      ? t('field-error-comment-markers')
      : jsonError(text),
  // 2h.6's 172: the text goes to the engine as it stands, so only the comment
  // markers can break the line.
  'reasoning-budget-message': (text) =>
    text.includes(';') || text.includes('#')
      ? t('field-error-comment-markers')
      : undefined,
  // 2h.8's 186: `checks::fit_target`. The comment markers first, then the list
  // of MiB: whole numbers, no sign, separated by commas with the space around
  // each allowed and no empty item.
  'fit-target': (text) =>
    text.includes(';') || text.includes('#')
      ? t('field-error-comment-markers')
      : FIT_TARGET_TEXT.test(text)
        ? undefined
        : t('field-error-fit-target'),
}

/** The list of MiB of 2h.8's 186, read on the text trimmed (166). */
const FIT_TARGET_TEXT = /^\d+(?:[ \t]*,[ \t]*\d+)*$/

/** The error of a text that is not one JSON object (166): `JSON.parse` must
 * give an object that is not an array and not `null`; a parse error is the
 * same error. */
function jsonError(text: string): string | undefined {
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    return t('field-error-json-object')
  }
  return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
    ? undefined
    : t('field-error-json-object')
}

/** The error of a text field's text (166): the key's check on the text
 * trimmed, nothing for an empty text or a key the table lacks. Checked
 * changed or not, like the number fields. */
export function textError(row: TextRow, text: string): string | undefined {
  const check = TEXT_KEYS[row.key]
  if (check === undefined) return undefined
  const trimmed = text.trim()
  return trimmed === '' ? undefined : check(trimmed)
}

/** What a text field sends (133): the text trimmed, and nothing reformatted;
 * the emptied text is `null`, the key's removal, never `""` (a key the file
 * lacks is not in the draft, so the page sends nothing for it). */
export function textSend(text: string): string | null {
  const trimmed = text.trim()
  return trimmed === '' ? null : trimmed
}

/** The box state of every number field the form edits, as `values` leaves it
 * (156): a key the file lacks has its box ticked, with nothing to show. */
export function numberStates(
  values: Record<string, string>,
): Record<string, Boxed> {
  const states: Record<string, Boxed> = {}
  for (const row of pageNumberRows())
    states[row.key] = boxedFrom(held(values, row.key))
  return states
}

/** The written-as line of an editable row (134): the spelling the file holds,
 * which the save leaves as it is, so it stands only while the control is at
 * the state the read gave. A row whose key is not a boolean has no line. A
 * box compares the third state of 2e's 128 (2h.8's 185): a value the engine
 * refuses is a state no click returns to, so once the box is changed the line
 * goes, whatever the state the file's reading shows. */
function editableWritten(
  row: SelectRow | CheckboxRow,
  draft: Record<string, string>,
  loaded: Record<string, string>,
): string | undefined {
  const same =
    row.kind === 'checkbox'
      ? checkboxDirty(row, draft) === checkboxDirty(row, loaded)
      : selectState(row, draft) === selectState(row, loaded)
  if (!same) return undefined
  const raw = heldFor(row, loaded)
  return row.reading === undefined || raw === undefined
    ? undefined
    : readAs(row.reading, raw).written
}

/** The file's value for a device off the GPUs (19 G.17), the value of the
 * `none` option of a device select. Data, not text: the file holds it and the
 * save sends it back. */
const DEVICE_NONE = 'none'

/** A device select's options and selected value (criterion 103, brief 19's
 * 19 G.16 and 19 G.17): the empty option, the row's `none` where it has one,
 * then every GPU the device read lists, in the probe's order, its id as the
 * option's value and `model-profile-device-option` as its text (the id alone
 * when the probe names no product). The file's device is the last option when
 * no GPU of the list names it: read as `model-profile-device-option-missing`
 * once the probe answered with a time, else the value itself. A GPU the file
 * spells differently (the case only differing) is the option with the file's
 * spelling as its value, so an untouched select holds what the file holds and
 * the form stays clean. The state the control shows, when no option carries it,
 * is added as its own. While the read runs, or when it failed, the list is what
 * it was before the probe (criterion 103): the empty option and the file's
 * device, the page having no list to add. */
function shownDevice(
  row: DeviceRow,
  probe: GpuProbe | null | undefined,
  file: string | undefined,
  state: string | undefined,
): {
  options: Option[]
  value: string
} {
  const value = state ?? ''
  if (probe === undefined || probe === null) {
    const options = [
      { value: '', text: row.empty },
      ...(file === undefined ? [] : [{ value: file, text: file }]),
    ]
    return {
      options: options.some((o) => o.value === value)
        ? options
        : [...options, { value, text: value }],
      value,
    }
  }
  // The GPU the file's device names, matched ignoring case (19 G.16).
  const listed =
    file === undefined
      ? undefined
      : probe.devices.find((one) => one.id.toLowerCase() === file.toLowerCase())
  const options = [
    { value: '', text: row.empty },
    ...(row.none === undefined ? [] : [{ value: DEVICE_NONE, text: row.none }]),
    ...probe.devices.map((one) => ({
      value: listed === one && file !== undefined ? file : one.id,
      text:
        one.name === null
          ? one.id
          : t('model-profile-device-option', one.id, one.name),
    })),
  ]
  // The file's device, when no option carries it, is the extra one at the end.
  if (
    file !== undefined &&
    listed === undefined &&
    !options.some((one) => one.value.toLowerCase() === file.toLowerCase())
  ) {
    options.push({
      value: file,
      text:
        probe.probedAt === null
          ? file
          : t('model-profile-device-option-missing', file),
    })
  }
  return {
    options: options.some((o) => o.value === value)
      ? options
      : [...options, { value, text: value }],
    value,
  }
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
    : { id: `model-profile-full-${row.key}`, text }
}

/** The id of the field a row renders (the Field's convention: its control is
 * `<id>-control`): the page marks a field by the key of its row. */
export function fieldId(key: string): string {
  return `model-profile-${key}`
}

/** 2h.8's 184: is the number field a row's `allowed` rule reads at default
 * right now? A field the form edits answers with its box (unticked: the user
 * set it), a key the form does not own with the value the control shows. */
function atDefault(
  key: string,
  values: Record<string, string>,
  boxed: Record<string, Boxed>,
): boolean {
  const b = boxed[key]
  return b === undefined ? held(values, key) === undefined : b.ticked
}

/** The `spec-type` entries, split as `model_profiles.rs` does: on `,`, trimmed, the
 * empty ones and `none` dropped (2-models' 75). The page's one split of the
 * list: the Assets card shows the Drafter and the N-gram drafters with it, and
 * the group rule of 190 and 191 enables a group of the Speculative decoding
 * card with it, so the page and the list never part ways. */
export function specTypeEntries(specType: string): string[] {
  return specType
    .split(',')
    .map((entry) => entry.trim())
    .filter((entry) => entry !== '' && entry !== 'none')
}

/** 2h.9's 190 and 191: does the read switch this group on? Only the values the
 * read gave are read, never the draft: `spec-type` cannot change on this page
 * (192, the Assets card is 2i), so a group's state is the file's for the page's
 * life. `draft` is enabled by a draft model file, or by a drafter of the list,
 * an entry that starts with `draft-`: the GUI's half of the rule, "the model
 * embeds MTP heads", is a GGUF fact the page reads only from 2g, so a drafter
 * named in `spec-type` stands in for it (the GUI's save drops a `draft-mtp` the
 * model cannot use, so a file it saved names one only for a model with the
 * heads). A deliberate difference: an MTP model whose `spec-type` names no
 * drafter has the group off here, on in the GUI. Each n-gram group is enabled
 * by its own entry, matched whole on the split of 75. The draft file is tested
 * on its trimmed value: the manager's INI reader trims every value it reads
 * (`crates/core/src/ini.rs`), so a `model-draft` of only spaces reaches the
 * page as the empty one, and 190's "not empty" is not met. */
function specGroupOn(group: SpecGroup, loaded: Record<string, string>) {
  const entries = specTypeEntries(held(loaded, 'spec-type') ?? '')
  if (group === 'draft')
    return (
      (held(loaded, 'model-draft') ?? '').trim() !== '' ||
      entries.some((entry) => entry.startsWith('draft-'))
    )
  return entries.includes(group)
}

/** The rule of the group a row sits in, when that group has one of 190 or 191:
 * `ruleLocked` reads it, so a group's fields move together and the page's gate
 * needs no knowledge of the card. */
function specGroupRule(key: string): SpecGroup | undefined {
  for (const card of CARDS)
    for (const row of card.rows)
      if (
        row.kind === 'group' &&
        row.enabledBy !== undefined &&
        row.rows.some((nested) => nested.kind !== 'note' && nested.key === key)
      )
        return row.enabledBy
  return undefined
}

/** 2h.7's 176: is the row's control disabled by a rule of its `enabledWhen`, or
 * by the rule of the group it sits in (190, 191)? The `unticked` rules read the
 * box state of the number fields (155), the `checked` rule the draft, live from
 * the box the user sees, and the `file` rules and the group rule the values the
 * read gave (their keys are not editable on this page). A
 * locked control keeps the state it holds: its draft stays, and a change
 * made while it was enabled is sent as for any other editable row. The page's
 * Save applies the same gate to its check of 157 and 166: what the page sends,
 * it checks, so a locked field is checked when its draft differs from the
 * loaded value (the save sends it) and skipped when it is the loaded value
 * (176, 2026-11-18). */
export function ruleLocked(
  row: RowBase & { moeFacts?: true },
  draft: Record<string, string>,
  loaded: Record<string, string>,
  boxed: Record<string, Boxed>,
  facts: FactsState,
): boolean {
  // 19 G.14: a row whose key only means something for a model with experts is
  // closed while the read has not answered, and while it answered a dense file.
  // The note right before the field is the reason. A read that answered a
  // reason, or nothing at all, leaves the row open: it said no model.
  if (row.moeFacts === true && (facts === 'reading' || facts === 'dense'))
    return true
  const group = specGroupRule(row.key)
  if (group !== undefined) return !specGroupOn(group, loaded)
  return (row.enabledWhen ?? []).some((one) => {
    if (one.rule === 'unticked') return boxed[one.key]?.ticked === true
    if (one.rule === 'file') return held(loaded, one.key) === undefined
    const box = pageCheckboxRows().find((r) => r.key === one.key)
    return box === undefined || !checkboxState(box, draft)
  })
}

/** 176: the control a rule follows, the one control that unlocks a field the
 * rule locks, read from the row's `enabledWhen` and not from its key: the
 * checkbox of a `checked` rule (Fit to VRAM for the Fit pair, Vision on GPU for
 * Vision GPU) and the default box of the field an `unticked` rule names
 * (Parallel slots for Unified KV). A `file` rule names a key this page does not
 * edit, so it gives no control. The id of a Field's control: a checkbox is
 * `<id>-control`, a number field's default box `<id>-box`. */
export function unlockControl(row: RowBase): string | undefined {
  const rule = (row.enabledWhen ?? []).find((one) => one.rule !== 'file')
  if (rule === undefined) return undefined
  return `${fieldId(rule.key)}-${rule.rule === 'checked' ? 'control' : 'box'}`
}

/** One field of a card, in the form its row names. `values` is what the control
 * shows: the form's draft for an editable row, the read's values for every
 * other one. `loaded` is the value the read gave, which the written-as line of
 * an editable row describes, and `onEdit` what a pick of an editable control
 * leaves in the draft. `boxed` is the box state of the number fields the form
 * edits (2h.4's 156), `onTick` and `onBoxText` what its controls report; a row
 * that is not in `boxed` shows the file and is disabled. `errors` is the field
 * error of a save refusal, by key (2h's 142): it marks the field the refusal
 * named. */
function ModelProfileField({
  row,
  values,
  loaded,
  onEdit,
  boxed,
  onTick,
  onBoxText,
  errors,
  gpu,
  tensor,
}: {
  row: FieldRow
  values: Record<string, string>
  loaded: Record<string, string>
  onEdit: (key: string, value: string) => void
  boxed: Record<string, Boxed>
  onTick: (key: string, ticked: boolean) => void
  onBoxText: (key: string, text: string) => void
  errors: Partial<Record<string, string>>
  gpu?: GpuCardProps
  tensor?: TensorCardProps
}) {
  if (row.kind === 'note') {
    // 177: a note that speaks for a rule sits only while the rule is not met.
    if (
      row.shownWithout !== undefined &&
      held(loaded, row.shownWithout) !== undefined
    )
      return null
    // 19 G.14: a note that speaks for a state of the model file's facts sits
    // only while the read is at it.
    if (
      row.shownFacts !== undefined &&
      (gpu === undefined || factsRead(gpu.modelInfo) !== row.shownFacts)
    )
      return null
    // 181: a plain paragraph in reading order, never a live region and never a
    // control's description.
    return <p className="note">{row.text}</p>
  }
  if (row.kind === 'facts') {
    // Brief 19 (fe/briefs/19-layer-sliders.md), 19 G.2: what the model file is,
    // the page's one read of it. A plain muted paragraph in reading order, after
    // the card's intro and before its first field, never a live region and never
    // a control's description.
    return gpu === undefined ? null : (
      <p className="muted">{factsLine(gpu.modelInfo)}</p>
    )
  }
  if (row.kind === 'gpu') {
    // Brief 14: the GPUs fieldset. Its state is the page's (the device read,
    // the rows' weights, the last move), and the fieldset composes the `device`
    // and `tensor-split` keys from it, so it writes no `onEdit`. Brief 22,
    // IP5.5 moved the keys to the instance profile: the model profile page
    // draws no such card, and the renderer stays for the entry page of brief 26
    // that draws it.
    return gpu === undefined ? null : (
      <GpuFieldset
        probe={gpu.probe}
        modelInfo={gpu.modelInfo}
        device={values[row.key] ?? null}
        loadedDevice={gpu.loadedDevice}
        mode={values['split-mode'] ?? null}
        tensorSplit={values['tensor-split'] ?? null}
        layers={gpu.layers}
        weights={gpu.weights}
        deviceError={errors[row.key]}
        splitError={errors['tensor-split']}
        splitErrorIds={gpu.weightIds ?? undefined}
        onToggle={gpu.onToggle}
        onMove={gpu.onMove}
        onWeight={gpu.onWeight}
        onBlock={gpu.onBlock}
        onBlockCommit={gpu.onBlockCommit}
        onBlockBase={gpu.onBlockBase}
        onBlockRelease={gpu.onBlockRelease}
        onAuto={gpu.onAuto}
        onEven={gpu.onEven}
        moved={gpu.moved}
      />
    )
  }
  if (row.kind === 'tensor') {
    // Brief 21 (fe/briefs/21-tensor-placement.md), 21 T.2: the rules of
    // `override-tensor`. Its state is the page's (the rows and the device read),
    // and the fieldset composes the key from the rows, so it writes no `onEdit`.
    // Brief 22, IP5.5 moved the key to the instance profile: the renderer stays
    // for the entry page of brief 26 that draws the card.
    return tensor === undefined ? null : <TensorPlacement {...tensor} />
  }
  // What every control of a read-only card shares: its id, the texts of its
  // field, and the native disabled of criterion 85. Each control's own
  // handler does nothing: the page sends nothing for it. An editable row's
  // control is enabled and its handler writes the draft (130, 164).
  const editable = row.editable === true
  // 19 G.13, 19 G.14: where the model file's facts read is, for the rows that
  // follow it.
  const factsState = gpu === undefined ? 'reading' : factsRead(gpu.modelInfo)
  // 2h.7's 176: a row the rules of 155 and 176 disable is natively disabled
  // and keeps the state it holds.
  const locked = ruleLocked(row, values, loaded, boxed, factsState)
  const common = {
    id: fieldId(row.key),
    label: row.label,
    hint: row.hint,
    hintSingle: row.hintSingle === true,
    disabled: true,
    error: errors[row.key],
  }
  const value = heldFor(row, values)
  // A row whose key is a boolean: the reading gives the state the engine acts as
  // and the spelling the file holds (criterion 122).
  const state =
    row.reading === undefined || value === undefined
      ? undefined
      : readAs(row.reading, value)
  if (row.kind === 'number') {
    // A number field the form edits (155, 156): the page owns the box state, so
    // the row is in `boxed`; the input is empty and disabled while the box is
    // ticked, and shows the text it holds when it is not.
    const b = boxed[row.key]
    if (b !== undefined) {
      // The same gate as the select's: the input is natively disabled when the
      // rule of 176 locks it, and the full-value line of 118 shows under a
      // control that is disabled now (134).
      const disabled = !editable || locked
      const text = boxText(b)
      // 19 G.13, 19 G.14: a layer row's slider reaches every block of the model
      // file, so it sits only where the read answered the counts, and for MoE
      // only where it answered experts. Its words are the key's own (157).
      const facts = gpu === undefined ? null : gpuFacts(gpu.modelInfo)
      const range =
        row.slider === undefined || facts === null
          ? undefined
          : row.slider === 'moe' && !facts.moe
            ? undefined
            : {
                max: facts.blockCount,
                atDefault: row.slider === 'blocks' ? facts.blockCount : 0,
                words: WORD_VALUES[row.key],
              }
      return (
        <DefaultBoxField
          {...common}
          disabled={disabled}
          box={t('field-default')}
          numeric={numericMode(row)}
          unit={row.unit}
          range={range}
          ticked={b.ticked}
          text={text}
          full={disabled ? fullValue(row, text) : undefined}
          onTick={(ticked) => onTick(row.key, ticked)}
          onText={(text) => onBoxText(row.key, text)}
        />
      )
    }
    const text = value ?? ''
    return (
      <DefaultBoxField
        {...common}
        box={t('field-default')}
        numeric={numericMode(row)}
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
    // text, and 2h.8's 184 gives the text that says what the fit can still do
    // once the box is checked. The rule says nothing about the box itself: what
    // the box shows is what the engine does with the file's value, allowed or
    // not (127), and a value the engine refuses carries the field's error (128).
    const checked = state === undefined ? row.absent : state.on === true
    const rule = row.allowed
    const set =
      rule === undefined
        ? []
        : rule.needsAbsent.filter((one) => !atDefault(one.key, values, boxed))
    const toggle =
      rule === undefined || set.length === 0
        ? row.toggle
        : !checked
          ? rule.notAllowed
          : set.length === rule.needsAbsent.length
            ? rule.setAll
            : set[0]!.setOne
    // An editable box's line is the loaded value's spelling, which the save
    // leaves as it is, so it stands only while the box is at the state the read
    // gave (134).
    const written = editable
      ? editableWritten(row, values, loaded)
      : state?.written
    return (
      <CheckboxField
        {...common}
        disabled={!editable || locked}
        toggle={toggle}
        checked={checked}
        written={
          written === undefined
            ? undefined
            : { id: `model-profile-written-${row.key}`, text: written }
        }
        error={common.error ?? state?.error}
        onChange={(checked) =>
          editable ? onEdit(row.key, checked ? 'true' : 'false') : undefined
        }
      />
    )
  }
  if (row.kind === 'text') {
    // No placeholder: an example in a field nobody can type into reads as a
    // value (criterion 87). An editable text field (164) holds the text the
    // form's draft gives, typed as it stands, and its input is open.
    const text = editable ? (values[row.key] ?? '') : (value ?? '')
    // The same gate as the select's: 134 keeps the full-value line of 118 under
    // a control that is disabled now, one a rule of 176 locks included.
    const disabled = !editable || locked
    return (
      <TextField
        {...common}
        disabled={disabled}
        value={text}
        unit={row.unit}
        full={disabled ? fullValue(row, text) : undefined}
        placeholder=""
        onChange={(text) => (editable ? onEdit(row.key, text) : undefined)}
      />
    )
  }
  const select =
    row.kind === 'device'
      ? // The device select's list is the host's GPUs once the device read
        // answered, and the state it shows from the form's draft (103, 132a,
        // 19 G.16).
        shownDevice(row, gpu?.probe, heldFor(row, loaded), value)
      : // An enabled select shows the state the page is dirty against (132),
        // which is the state `selectSend` writes back: `selectState` decides
        // both, so the option the user reads and the value the save sends are
        // the same one. Its option-first match keeps a spelling one of the
        // options carries, the `default` of a tri-state row included, as that
        // option, and not the on the flag's reading gives it (133). The list
        // is built from the state of the value the read gave, so the extra
        // option of 87 stays last in it whatever is picked (132a), and a flag
        // keeps the options of 1.3 (125): an unlisted spelling is a state
        // among them, never an extra option.
        editable
        ? {
            options: optionsAt(row, selectState(row, loaded)),
            value: selectState(row, values),
          }
        : // A disabled row shows the read's values: a flag's select shows on
          // or off by the reading and lists no extra option (criterion 125). A
          // valued key whose value is in neither list keeps criterion 87: the
          // extra option carries it, and its list is the one `shown` gave.
          state !== undefined && state.on !== undefined
          ? { options: row.options, value: state.on ? 'true' : 'false' }
          : {
              options: shown(row, heldFor(row, loaded)).options,
              value: shown(row, value).value,
            }
  // What the user reads for a select is the selected option's text, which for
  // an unlisted value is the value itself (criterion 87). A device select
  // reads the file's device, and has nothing to repeat when the file names
  // none (criterion 103).
  const text =
    row.kind === 'device'
      ? (value ?? '')
      : (select.options.find((o) => o.value === select.value)?.text ??
        select.value)
  // A control is natively disabled when its row is not editable, and right now
  // when a rule of 176 locks it: the same gate for the full-value line of 118.
  const disabled = !editable || locked
  // The full-value line of 118: an enabled select opens, so its whole value
  // shows, and 134 keeps the line under a control that is disabled now, one a
  // rule of 176 locks included.
  const full = disabled ? fullValue(row, text) : undefined
  // The written-as line of criterion 124: the control shows what the engine does
  // with the value, the line what the file holds. A flag's control shows on or
  // off, so any value the configurator would not write needs the line. A valued
  // key whose value is in neither list is the extra option, so the select shows
  // it itself and needs no line (124a): what the select clips, the full-value
  // line of 118 repeats. An editable row's line is the loaded value's spelling,
  // which the save leaves as it is, so it stands only while the control is at
  // the state the read gave (134).
  const written =
    row.kind === 'select' && editable
      ? editableWritten(row, values, loaded)
      : state?.written
  // 25 GT5b.1 (the review's D1): the row that words its absent key shows those
  // words under the select while the control is at the absent option, and names
  // them as its description. They go with the first value picked.
  const note =
    row.kind === 'select' &&
    row.unsetNote !== undefined &&
    select.value === row.absent
      ? { id: `${common.id}-unset`, text: row.unsetNote }
      : undefined
  return (
    <SelectField
      {...common}
      disabled={disabled}
      options={select.options}
      value={select.value}
      full={full}
      note={note}
      written={
        written === undefined
          ? undefined
          : { id: `model-profile-written-${row.key}`, text: written }
      }
      onChange={(value, picked) =>
        editable && picked ? onEdit(row.key, value) : undefined
      }
    />
  )
}

/** One row of a card: a group is its heading and the division the heading
 * names, holding its own rows; every other row is the field of
 * `ModelProfileField`. */
function ModelProfileRow({
  row,
  values,
  loaded,
  onEdit,
  boxed,
  onTick,
  onBoxText,
  errors,
  gpu,
  tensor,
}: {
  row: Row
  values: Record<string, string>
  loaded: Record<string, string>
  onEdit: (key: string, value: string) => void
  boxed: Record<string, Boxed>
  onTick: (key: string, ticked: boolean) => void
  onBoxText: (key: string, text: string) => void
  errors: Partial<Record<string, string>>
  gpu?: GpuCardProps
  tensor?: TensorCardProps
}) {
  if (row.kind === 'group') {
    const heading = `model-profile-group-${row.key}`
    // 192: a group the rule of 190 or 191 leaves off says why, in a plain
    // paragraph before its first field, inside the division the heading names.
    // Shown only while the group is off, never a live region and never the
    // description of anything (181).
    const off =
      row.enabledBy !== undefined && !specGroupOn(row.enabledBy, loaded)
    return (
      <div className="subgroup">
        <h4 id={heading}>{row.title}</h4>
        <div role="group" aria-labelledby={heading}>
          {off && row.offNote !== undefined && (
            <p className="note">{row.offNote}</p>
          )}
          {row.rows.map((nested) => (
            <ModelProfileRow
              key={nested.key}
              row={nested}
              values={values}
              loaded={loaded}
              onEdit={onEdit}
              boxed={boxed}
              onTick={onTick}
              onBoxText={onBoxText}
              errors={errors}
              gpu={gpu}
              tensor={tensor}
            />
          ))}
        </div>
      </div>
    )
  }
  return (
    <ModelProfileField
      row={row}
      values={values}
      loaded={loaded}
      onEdit={onEdit}
      boxed={boxed}
      onTick={onTick}
      onBoxText={onBoxText}
      errors={errors}
      gpu={gpu}
      tensor={tensor}
    />
  )
}

/** 2h.10's 196: the id of a card's heading, the jump list's target for that
 * card. The page's Assets card and the cards of this file share the shape. */
export function cardHeadingId(id: string): string {
  return `model-profile-card-${id}`
}

/** The keys a model profile keeps (brief 22, IP5.5): the mirror of
 * `MODEL_PROFILE_STAY_KEYS` in `crates/core/src/checks.rs`, which says why each
 * key stays and lists the keys that move. The page filters its cards by this
 * list; the moved keys are set per model in the instance profile's entry
 * (brief 26). Keep the two lists in step. */
const STAY_KEYS = [
  'model',
  'hf-repo',
  'hf-file',
  'mmproj',
  'model-draft',
  'temp',
  'top-k',
  'top-p',
  'min-p',
  'repeat-penalty',
  'presence-penalty',
  'backend-sampling',
  'n-predict',
  'jinja',
  'chat-template-kwargs',
  'prefill-assistant',
  'reasoning',
  'reasoning-format',
  'reasoning-effort',
  'reasoning-preserve',
  'reasoning-budget',
  'reasoning-budget-message',
  'embeddings',
  'pooling',
  'cache-reuse',
  'image-min-tokens',
  'image-max-tokens',
  'spec-type',
  'spec-draft-n-max',
  'spec-draft-n-min',
  'spec-draft-p-min',
  'spec-ngram-mod-n-match',
  'spec-ngram-mod-n-min',
  'spec-ngram-mod-n-max',
  'spec-ngram-simple-size-n',
  'spec-ngram-simple-size-m',
  'spec-ngram-map-k-size-n',
  'spec-ngram-map-k-size-m',
  'spec-ngram-map-k4v-size-n',
  'spec-ngram-map-k4v-size-m',
  'spec-ngram-map-k4v-min-hits',
]

/** The notes the model profile page keeps (IP5.5): a note reads no key, so the
 * rule above cannot keep it. Each speaks for the fields of its card: the output
 * budget for the Chat / reasoning card, the image tokens for the Vision card.
 * The notes of the keys that moved go with them. */
const STAY_NOTES = ['output-budget-note', 'image-tokens-note']

/** The row the model profile page keeps (IP5.5's one rule): a field whose key
 * is in `STAY_KEYS`, a note whose key is in `STAY_NOTES`. The GPU fieldset, the
 * rules of `override-tensor` and the model file's facts line go with the cards
 * that moved: the first two compose a moved key, the last reads the model file
 * the page no longer reads. A group is kept while a field of its key is left in
 * it. */
function stays(row: Row): boolean {
  if (row.kind === 'group')
    return row.rows.some((one) => stays(one) && one.kind !== 'note')
  if (row.kind === 'note') return STAY_NOTES.includes(row.key)
  if (row.kind === 'facts') return false
  return STAY_KEYS.includes(row.key)
}

/** The rows of one card as the model profile page draws them, in today's order:
 * the rows of `stays`, a group with its own rows filtered the same way. */
function stayRows(rows: Row[]): Row[] {
  return rows
    .map((row) =>
      row.kind === 'group' ? { ...row, rows: row.rows.filter(stays) } : row,
    )
    .filter(stays)
}

/** A card the model profile page draws: one with a field left in it (IP5.5). A
 * card of notes alone says nothing on its own, so it is not drawn. */
function hasField(card: CardFields): boolean {
  return card.rows.some((row) => stays(row) && row.kind !== 'note')
}

/** The cards of the model profile page (brief 22, IP5.5): `CARDS` filtered by
 * `STAY_KEYS`, in today's order, with a card left with no field dropped. The
 * four cards of the placement are in `CARDS` and not on this page: they move to
 * the instance profile's entry (brief 26), which draws them from the same list.
 * The KV cache card keeps `cache-reuse` alone, so its intro says where the rest
 * of its rows went. */
export function pageCards(): CardFields[] {
  return CARDS.map((card) => {
    const rows = stayRows(card.rows)
    if (card.id !== 'kv-cache') return { ...card, rows }
    return {
      ...card,
      rows,
      introLines: t('model-profile-kv-cache-reuse-intro').split('\n'),
      about: undefined,
    }
  }).filter(hasField)
}

/** The jump list of 196, in card order: each card's heading id and the title
 * its jump button is named by. The page's jump list is the list of the cards it
 * draws (IP5.5: "On this page" names the eight cards of the page). */
export function cardJumps(): { id: string; title: string }[] {
  return pageCards().map((card) => ({
    id: cardHeadingId(card.id),
    title: card.title,
  }))
}

/** Where a field of a card lives: the card's title, and the label of its row.
 *  A note and a group carry no field, and a key the cards do not have (an
 *  Assets key, or one the table does not know) is on no card: null. */
export function fieldPlace(
  key: string,
): { card: string; label: string } | null {
  for (const card of CARDS) {
    for (const row of card.rows) {
      if (row.key === key) {
        // A note, a facts line and a group carry no field: none has a label to
        // name.
        return row.kind === 'note' ||
          row.kind === 'facts' ||
          row.kind === 'group'
          ? null
          : { card: card.title, label: row.label }
      }
      if (row.kind === 'group') {
        for (const inner of row.rows) {
          if (inner.key === key) {
            return inner.kind === 'note' || inner.kind === 'facts'
              ? null
              : { card: card.title, label: inner.label }
          }
        }
      }
    }
  }
  return null
}

/** A card's intro: its one sentence, and the card's own About panel (22 D4.31,
 * as its D1 review of 2026-11-27 amends it): a card that opens with three
 * hundred words pushes its first field down the screen, so the card shows the
 * sentence and keeps the rest in the panel. The panel is the field hint of
 * today, one paragraph per line, closed by the toggle of the heading row
 * (`ProfileCard`, 22 D4.31 review A2): no new word, no new look. */
function CardIntro({
  lines,
  about,
  hintId,
  open,
}: {
  /** The intro as the strings file gives it, one entry per line. */
  lines: string[]
  /** 22 D4.31 review D1: the words of the panel, `\n` breaks included. */
  about?: string
  /** The panel's id, named by the heading row's toggle. */
  hintId: string
  open: boolean
}) {
  if (lines.length === 0) return null
  return (
    <>
      <p className="intro">{lines[0]}</p>
      {about === undefined ? null : (
        <Hint id={hintId} text={about} open={open} />
      )}
    </>
  )
}

/** What a page gives the card list: the form it edits (`values`, the draft, and
 * `loaded`, the read's values), the writes, and the two cards that read the
 * page's own state (`gpu`, `tensor`). */
type CardsProps = {
  values: Record<string, string>
  loaded: Record<string, string>
  onEdit: (key: string, value: string) => void
  boxed: Record<string, Boxed>
  onTick: (key: string, ticked: boolean) => void
  onBoxText: (key: string, text: string) => void
  errors: Partial<Record<string, string>>
  /** Brief 14: the GPU distribution card's fieldset, the page's GPU state. A
   * page that only shows the file passes none: the fieldset shows nothing. */
  gpu?: GpuCardProps
  /** Brief 21 (fe/briefs/21-tensor-placement.md): the Tensor placement card's
   * rules, the page's rows. A page that only shows the file passes none: the
   * rules show nothing, as the GPUs' fieldset does. */
  tensor?: TensorCardProps
}

/** One card of the list, with its own About open state: the toggle sits in the
 * row of the card's h3 (22 D4.31 review A2) and the panel in the card's body,
 * so the state is the card's, and one component holds both. */
function ProfileCard({ card, page }: { card: CardFields; page: CardsProps }) {
  const [open, setOpen] = useState(false)
  const headingId = cardHeadingId(card.id)
  const hintId = `${headingId}-about`
  const {
    values,
    loaded,
    onEdit,
    boxed,
    onTick,
    onBoxText,
    errors,
    gpu,
    tensor,
  } = page
  return (
    <Card
      title={card.title}
      // 196: the heading is this card's jump target, so it carries the id
      // and the tabindex -1 of the rule.
      headingId={headingId}
      // 22 D4.31 review A2: the card's About stands beside the title, the way a
      // field's About follows its label, and is the card's first Tab stop
      // (the designer's ruling of 2026-11-27, WCAG 2.1.1): a keyboard reader
      // reaches the card's words as they reach the card.
      headingExtra={
        card.about === undefined ? undefined : (
          <HintToggle
            label={card.title}
            open={open}
            hintId={hintId}
            onToggle={() => setOpen(!open)}
          />
        )
      }
      // 176, 181: while the card's file rule is not met, its note sits
      // right after the h3, before the first field: a plain paragraph in
      // reading order, not a live region, no control's description.
      intro={
        card.needsFile !== undefined &&
        held(loaded, card.needsFile.key) === undefined
          ? card.needsFile.note
          : undefined
      }
    >
      {card.introLines === undefined ? null : (
        <CardIntro
          lines={card.introLines}
          about={card.about}
          hintId={hintId}
          open={open}
        />
      )}
      {card.rows.map((row) => (
        <ModelProfileRow
          key={row.key}
          row={row}
          values={values}
          loaded={loaded}
          onEdit={onEdit}
          boxed={boxed}
          onTick={onTick}
          onBoxText={onBoxText}
          errors={errors}
          gpu={gpu}
          tensor={tensor}
        />
      ))}
    </Card>
  )
}

/** The cards, in the order of the table, each with the fields of its rows. The
 * page's form (`values`, the draft it edits, and `loaded`, the read's values)
 * writes a change of an editable row into `onEdit`, and the box state of its
 * number fields into `boxed` through `onTick` and `onBoxText`; a page that only
 * shows the file leaves every control disabled and passes an empty `boxed`.
 * `errors` marks the fields a save refusal named (2h's 142), by key. */
export default function ModelProfileCards(page: CardsProps) {
  return (
    <>
      {pageCards().map((card) => (
        <ProfileCard key={card.title} card={card} page={page} />
      ))}
    </>
  )
}
