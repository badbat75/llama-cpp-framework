// The Tensor placement card of the model profile page (brief 21,
// fe/briefs/21-tensor-placement.md, 21 T.2 to 21 T.13): the rules of
// `override-tensor`, one row each, the Add rule button and, after the Rules
// fieldset, the whole value as text (21 T.10), the way it is pasted from a
// guide and reordered.
//
// The text is the page's, as the rows are: the card shows it and reports
// typing, Enter and the focus leaving; the page applies the text to the rows.
//
// A control the page's own check at Save (21 T.11) or the manager's refusals
// (21 T.12) name is marked: `errors` is keyed by the control's id, and the
// marked control carries a Field Error of its own, on a line of its own at the
// end of its row, named by the control's `aria-describedby`.
//
// The rules are `tensorRules.ts`'s, the mirror of the configurator's table, so a
// row shows the rule the file says and the save sends the rule the row shows.
// The rows are the page's state, as the GPU list's rows are (brief 14): the card
// composes the one key out of them, and writes no `onEdit`.
//
// What a row is made of (21 T.6): the Tensor kind, the layers and the device.
// The rule's pattern is composed from them, so a Layers text `parseLayers` reads
// moves the rule to the kind composed with the layers it reads, and one it
// cannot read leaves the pattern as the last readable text composed it (the
// GUI's "last good value"; the page's own check refuses the text at Save,
// 21 T.11).
// A Custom rule keeps the text of its Pattern field, which becomes the rule at
// each input (21 T.6).
//
// TP3 adds the coverage line under each rule (21 T.15a): what the rule places on
// the model the page read, counted by `tensorRules.ts`'s `coverage` from the
// tensor list of the facts the page already reads for the GPU card (21 T.14a: no
// second request). The line is not a check: it marks nothing and blocks no Save.
import { useDeferredValue } from 'react'
import { Icon } from './Icon.tsx'
import type { GpuProbe } from './GpuSplit.tsx'
import type { ModelInfoRead } from './api.ts'
import {
  KINDS,
  CUSTOM,
  compose,
  coverage,
  layersText,
  parse,
  parseLayers,
  render,
  splitPattern,
} from './tensorRules.ts'
import type { Layers, TensorRule } from './tensorRules.ts'
import { FieldError, TextField } from './Field.tsx'
import { t } from './strings.ts'

/** One row of the card. `id` is the page's, so a removal renumbers the rows
 *  without moving a control (21 T.6), and the fields carry it in their ids. */
export type TensorRow = {
  id: number
  /** A kind's id of `KINDS`; `custom` for a pattern no kind owns. */
  kind: string
  /** The Layers field's text as typed. A text `parseLayers` cannot read is
   *  shown as it stands and changes nothing: `layers` keeps what the last text
   *  that read returned. */
  layersText: string
  /** The layers the rule is made of: what the last Layers text `parseLayers`
   *  read returned (null for every layer), so an unreadable text leaves the
   *  pattern as that text composed it. Kept while the field is hidden, so a
   *  kind without a block and back loses nothing (21 T.6). */
  layers: Layers
  /** The Pattern field's text, and the rule's pattern while the row is at
   *  Custom (21 T.6). Kept when the row leaves Custom, so a later visit to
   *  Custom shows it again. */
  customText: string | undefined
  /** The device the rule goes on; `''` for none. */
  device: string
}

/** The value → its rows, in file order (21 T.3). */
export function tensorRows(value: string | null): TensorRow[] {
  return (value === null ? [] : parse(value)).map((rule, i) => {
    const [kind, layers] = splitPattern(rule.pattern)
    const id = KINDS[kind]!.id
    return {
      id: i + 1,
      kind: id,
      layers,
      layersText: layersText(layers),
      customText: id === CUSTOM ? rule.pattern : undefined,
      device: rule.device,
    }
  })
}

/** The row's kind. */
function kindOf(row: TensorRow) {
  return KINDS.find((one) => one.id === row.kind) ?? KINDS[KINDS.length - 1]!
}

/** The rule a row stands for now (21 T.6): the kind composed with the layers
 *  the row carries, or the Custom text as committed. */
export function rowRule(row: TensorRow): TensorRule {
  if (row.kind === CUSTOM)
    return { pattern: row.customText ?? '', device: row.device }
  return {
    pattern: compose(KINDS.indexOf(kindOf(row)), row.layers),
    device: row.device,
  }
}

/** The rows → the value, and null when no rule is left, which is the key's
 *  removal (21 T.8). */
export function rowsValue(rows: TensorRow[]): string | null {
  const rules = rows.map(rowRule)
  return rules.length === 0 ? null : render(rules)
}

/** A new rule (21 T.7): the embedding table, on the device the page gives. */
export function otNewRule(id: number, device: string): TensorRow {
  const embd = KINDS.find((one) => one.id === 'embd') ?? KINDS[0]!
  return {
    id,
    kind: embd.id,
    layers: null,
    layersText: '',
    customText: undefined,
    device,
  }
}

/** The Layers field at each input (21 T.6): the text is shown as typed, and a
 *  text `parseLayers` reads moves the rule's layers to what it reads. A text it
 *  cannot read leaves them where the last readable one left them, so the
 *  pattern stays as that composed it: loaded `blk\.(0|1|2)...`, typed `10-12,
 *  15` then `10-`, the rule holds blocks 10, 11, 12 and 15 (the GUI's last good
 *  value). The text on screen is what the Save checks (21 T.11). */
export function otSetLayersText(row: TensorRow, text: string): TensorRow {
  const read = parseLayers(text)
  return {
    ...row,
    layersText: text,
    ...('unreadable' in read ? {} : { layers: read.layers }),
  }
}

/** The id of the format paragraph, the description of every Layers and Pattern
 *  field (21 T.4, WCAG 3.3.2). One card, so one id on the page. */
export const FORMAT_ID = 'model-profile-ot-format'

/** What a row's control carries when the page marked it (21 T.11, 21 T.12):
 *  `aria-invalid`, and the Field Error named by `aria-describedby` beside the
 *  ids that described the control before. An unmarked control keeps exactly the
 *  description it had (`described` is empty for a select). */
function marked(
  described: string[],
  error: string | undefined,
  id: string,
): { 'aria-invalid'?: true; 'aria-describedby'?: string } {
  const ids = error === undefined ? described : [...described, `${id}-error`]
  return {
    ...(error === undefined ? {} : { 'aria-invalid': true as const }),
    ...(ids.length === 0 ? {} : { 'aria-describedby': ids.join(' ') }),
  }
}

/** The Add rule button's id: the focus of a removal that leaves no rule (21 T.7). */
export const ADD_ID = 'model-profile-ot-add'

/** The Rules as text field's key (21 T.10), as the page's field keys are
 *  spelled: `fieldId` of it is the Field's id, so a refusal naming the whole
 *  value marks and focuses this field the way it marks any other field. */
export const OT_TEXT = 'ot-text'

/** The id of the control a Field Error key marks (21 T.11, 21 T.12): a row's
 *  control carries its own id, and the Rules as text Field owns its input, as
 *  every Field does, so its control is `<field id>-control`. */
export function otControlElementId(key: string): string {
  return key === OT_TEXT ? `model-profile-${OT_TEXT}-control` : key
}

/** A control's id, unique on the page: the row's fields carry the row's id, so a
 *  removal leaves the other rows alone and the page can focus a control by id. */
export function otControlId(field: string, id: number): string {
  return `model-profile-ot-${field}-${id}`
}

/** The Tensor select's focus id, where the page puts the focus after an Add or a
 *  Remove (21 T.7). */
export function otTensorId(id: number): string {
  return otControlId('tensor', id)
}

/** The option text of each kind. `KINDS` carries the strings key of each; the
 *  key is named here as a `t(...)` call, so every text is reachable from the
 *  code by its key. */
const KIND_TEXT: Record<string, string> = {
  embd: t('model-profile-ot-kind-embd'),
  ple: t('model-profile-ot-kind-ple'),
  output: t('model-profile-ot-kind-output'),
  layer: t('model-profile-ot-kind-layer'),
  attn: t('model-profile-ot-kind-attn'),
  ssm: t('model-profile-ot-kind-ssm'),
  ffn: t('model-profile-ot-kind-ffn'),
  ffn_up: t('model-profile-ot-kind-ffn_up'),
  ffn_gate: t('model-profile-ot-kind-ffn_gate'),
  ffn_down: t('model-profile-ot-kind-ffn_down'),
  exps: t('model-profile-ot-kind-exps'),
  exps_up: t('model-profile-ot-kind-exps_up'),
  exps_gate: t('model-profile-ot-kind-exps_gate'),
  exps_down: t('model-profile-ot-kind-exps_down'),
  shexp: t('model-profile-ot-kind-shexp'),
  router: t('model-profile-ot-kind-router'),
  custom: t('model-profile-ot-kind-custom'),
}

/** The Goes on options of a rule (21 T.5): `CPU` with its own text, then every
 *  GPU the device read lists, in the probe's order, by the shared device option
 *  text. A device no option's value equals exactly, the file's spelling
 *  included, is the last option, read "not detected" once the probe answered
 *  with a time and the device itself while it gave none. A rule with no device
 *  leads with the empty option whatever the read (the TP1 review's D3): while
 *  the read is pending the select's value has no other option to match, and it
 *  would read CPU for a rule on no device. */
function goesOnOptions(device: string, probe: GpuProbe | null | undefined) {
  const options: { value: string; text: string }[] = [
    { value: 'CPU', text: t('model-profile-ot-cpu') },
  ]
  for (const one of probe?.devices ?? [])
    options.push({
      value: one.id,
      text:
        one.name === null
          ? one.id
          : t('model-profile-device-option', one.id, one.name),
    })
  if (device === '') {
    options.unshift({ value: '', text: t('model-profile-ot-no-device') })
  } else if (!options.some((one) => one.value === device)) {
    options.push({
      value: device,
      text:
        typeof probe?.probedAt === 'string'
          ? t('model-profile-device-option-missing', device)
          : device,
    })
  }
  return options
}

/** The device of a rule Add appends (21 T.7): the first GPU the probe lists,
 *  `CPU` when it lists none or the read has not answered. While the read is
 *  pending the card has no GPU to offer, and the device of a rule on screen can
 *  be a GPU the bound refuses, so a new rule starts on CPU, as the configurator's
 *  `add` does. */
export function otNewDevice(probe: GpuProbe | null | undefined): string {
  return (probe?.devices ?? [])[0]?.id ?? 'CPU'
}

/** What the page gives the card: its rows, the device read the Goes on selects
 *  list, the Rules as text field's text, what each control does to a row, and
 *  the Field Errors by control id. */
export type TensorCardProps = {
  rows: TensorRow[]
  probe: GpuProbe | null | undefined
  /** The model file's facts the page reads once per arrival for the GPU card
   *  (brief 21, 21 T.14a): the card counts each rule over its tensor list, so
   *  it asks for nothing of its own. `undefined` while the read runs, and a read
   *  that answered nothing, a reason, or no tensor list leaves every rule with
   *  no line (21 T.16). */
  modelInfo: ModelInfoRead | undefined
  onKind: (id: number, kind: string) => void
  onLayersText: (id: number, text: string) => void
  onPatternText: (id: number, text: string) => void
  onDevice: (id: number, device: string) => void
  onAdd: () => void
  onRemove: (id: number) => void
  /** The Rules as text field, shown as it is (21 T.10): the page keeps the text
   *  typed in, and the loaded value as the file writes it while the key is
   *  unchanged (21 T.8). */
  text: string
  /** Typing: only the field's text changes, the rows keep what they show. */
  onText: (text: string) => void
  /** Enter, or the focus leaving: the page applies the text to the rows. */
  onApply: () => void
  /** The Field Errors (21 T.11, 21 T.12), keyed by the id of the control each
   *  one marks: `otControlId` of a row's field, and `OT_TEXT` for Rules as
   *  text. */
  errors: Partial<Record<string, string>>
}

/** The card's block: the Rules fieldset, its format paragraph, the rules as one
 *  list of rows, and the Add rule button last (21 T.2). The fieldset is a
 *  `.field`, so from 720 px the wrapper div is placed in the control column,
 *  beside the legend (the TP1 review's D1). After the fieldset, the Rules as
 *  text Field (21 T.10), a Field of the card like any other. */
export function TensorPlacement({
  rows,
  probe,
  modelInfo,
  onKind,
  onLayersText,
  onPatternText,
  onDevice,
  onAdd,
  onRemove,
  text,
  onText,
  onApply,
  errors,
}: TensorCardProps) {
  // The facts the page reads for the GPU card carry the model's tensors
  // (21 T.14a): that list is what every rule is counted over, and no list at all
  // is no line (21 T.16).
  const facts =
    modelInfo !== undefined && modelInfo !== null && 'facts' in modelInfo
      ? modelInfo.facts
      : null
  // The rows received when the manager cut the list, `null` when it did not
  // (21 T.16): the counts then cover only those.
  const list = facts?.tensors ?? null
  const tensors = list ?? []
  const cut =
    list !== null && facts?.tensorsTruncated === true ? list.length : null
  // The rules as the save writes them, so a line counts the rule its row stands
  // for: the kind composed with its layers, the Custom text as typed. The count
  // runs on a deferred copy of that value (21 T.15, fe/DESIGN.md 5.3), so typing
  // into a Pattern never waits on the 20 000 names.
  const value = rowsValue(rows) ?? ''
  const lines = coverage(useDeferredValue(value), tensors)
  return (
    <>
      <fieldset
        className="field"
        id="model-profile-ot-rules"
        aria-describedby={FORMAT_ID}
      >
        <legend className="field-label">{t('model-profile-ot-rules')}</legend>
        <div>
          <p className="muted" id={FORMAT_ID}>
            {t('model-profile-ot-format')}
          </p>
          {/* 21 T.16: the manager cut the list, so the counts below cover only
              the rows the page received. */}
          {cut !== null && (
            <p className="muted">{t('model-profile-ot-truncated', `${cut}`)}</p>
          )}
          {rows.length === 0 && (
            <p className="muted">{t('model-profile-ot-none')}</p>
          )}
          {rows.length > 0 && (
            <ul className="check-list" role="list">
              {rows.map((row, i) => (
                <TensorRuleRow
                  key={row.id}
                  row={row}
                  n={i + 1}
                  probe={probe}
                  line={lines[i] ?? ''}
                  onKind={onKind}
                  onLayersText={onLayersText}
                  onPatternText={onPatternText}
                  onDevice={onDevice}
                  onRemove={onRemove}
                  errors={errors}
                />
              ))}
            </ul>
          )}
          <button type="button" className="ot-add" id={ADD_ID} onClick={onAdd}>
            <Icon name="plus" />
            <span className="visually-hidden">{t('model-profile-ot-add')}</span>
          </button>
        </div>
      </fieldset>
      <TextField
        id={`model-profile-${OT_TEXT}`}
        label={t('model-profile-ot-text')}
        hint={t('model-profile-ot-text-hint')}
        error={errors[OT_TEXT]}
        value={text}
        placeholder=""
        onChange={onText}
        onApply={onApply}
      />
    </>
  )
}

/** One rule: a plain fieldset (no `.field`: from 720 px that is the card's
 *  two-column grid, and its control column is 0 px wide inside a rule) with its
 *  legend above, and a wrapping flex row whose every item is one visible label
 *  and the control it labels (the TP1 review's D1, 21 T.4): its Tensor select,
 *  its Layers field where the kind has blocks, its Goes on select, its Remove
 *  button and, for a Custom rule, its Pattern field last, on a line of its own
 *  (21 T.4, 21 T.9: the tab path in that order).
 *
 *  TP3: the row ends with its coverage line (21 T.15a), after the Field Errors,
 *  on a line of its own like they are, named last by the control that describes
 *  the rule: the Pattern input where the row has one, the Tensor select where it
 *  has none. The line is no live region, and a mark does not take it. */
function TensorRuleRow({
  row,
  n,
  probe,
  line,
  onKind,
  onLayersText,
  onPatternText,
  onDevice,
  onRemove,
  errors,
}: {
  row: TensorRow
  n: number
  probe: GpuProbe | null | undefined
  /** What this rule places, as the card counted it; `''` for no line. */
  line: string
  onKind: TensorCardProps['onKind']
  onLayersText: TensorCardProps['onLayersText']
  onPatternText: TensorCardProps['onPatternText']
  onDevice: TensorCardProps['onDevice']
  onRemove: TensorCardProps['onRemove']
  errors: TensorCardProps['errors']
}) {
  const rule = t('model-profile-ot-rule', `${n}`)
  const kind = kindOf(row)
  const custom = kind.id === CUSTOM
  const withLayers = kind.block !== ''
  const tensorId = otTensorId(row.id)
  const layersId = otControlId('layers', row.id)
  const goesOnId = otControlId('goes-on', row.id)
  const patternId = otControlId('pattern', row.id)
  const coverageId = otControlId('coverage', row.id)
  // The line's id, only while the row shows a line (21 T.15): a control never
  // names a description that is not there.
  const described = line === '' ? null : coverageId
  // The marked controls of this row, in the order they are on screen (21 T.11,
  // 21 T.12): each gets its Field Error, at the end of the row, on a line of
  // its own. Only the controls the row shows are looked for.
  const marks = (
    [
      [tensorId, errors[tensorId]],
      ...(withLayers ? [[layersId, errors[layersId]]] : []),
      [goesOnId, errors[goesOnId]],
      ...(custom ? [[patternId, errors[patternId]]] : []),
    ] as [string, string | undefined][]
  ).filter((one): one is [string, string] => one[1] !== undefined)
  return (
    <li>
      <fieldset>
        <legend>{rule}</legend>
        <div className="ot-rule-row">
          <span className="ot-tensor">
            <label htmlFor={tensorId}>{t('model-profile-ot-tensor')}</label>
            <select
              id={tensorId}
              value={row.kind}
              {...marked(
                described === null || custom ? [] : [described],
                errors[tensorId],
                tensorId,
              )}
              onChange={(e) => onKind(row.id, e.target.value)}
            >
              {KINDS.map((one) => (
                <option key={one.id} value={one.id}>
                  {KIND_TEXT[one.id]}
                </option>
              ))}
            </select>
          </span>
          {withLayers && (
            <span className="ot-layers">
              <label htmlFor={layersId}>{t('model-profile-ot-layers')}</label>
              <input
                id={layersId}
                className="number"
                type="text"
                value={row.layersText}
                {...marked([FORMAT_ID], errors[layersId], layersId)}
                onChange={(e) => onLayersText(row.id, e.target.value)}
              />
            </span>
          )}
          <span className="ot-goes-on">
            <label htmlFor={goesOnId}>{t('model-profile-ot-goes-on')}</label>
            <select
              id={goesOnId}
              value={row.device}
              {...marked([], errors[goesOnId], goesOnId)}
              onChange={(e) => onDevice(row.id, e.target.value)}
            >
              {goesOnOptions(row.device, probe).map((one) => (
                <option key={one.value} value={one.value}>
                  {one.text}
                </option>
              ))}
            </select>
          </span>
          <button
            type="button"
            className="ot-remove"
            onClick={() => onRemove(row.id)}
          >
            <Icon name="x" />
            <span className="visually-hidden">
              {t('model-profile-ot-remove', rule)}
            </span>
          </button>
          {custom && (
            <span className="ot-pattern">
              <label htmlFor={patternId}>{t('model-profile-ot-pattern')}</label>
              <input
                id={patternId}
                className="mono"
                type="text"
                spellCheck={false}
                autoCapitalize="off"
                value={row.customText ?? ''}
                {...marked(
                  described === null ? [FORMAT_ID] : [FORMAT_ID, described],
                  errors[patternId],
                  patternId,
                )}
                onChange={(e) => onPatternText(row.id, e.target.value)}
              />
            </span>
          )}
          {marks.map(([id, text]) => (
            <FieldError key={id} id={`${id}-error`} text={text} />
          ))}
          {described !== null && (
            <p className="ot-coverage" id={coverageId}>
              {line}
            </p>
          )}
        </div>
      </fieldset>
    </li>
  )
}
