// The Field of fe/DESIGN.md, section 5.2: one setting, its label with an
// About toggle that reveals the hint, the control, then the hint and the
// error. The form owns the values; a field only shows them and reports
// changes. Each field's elements take their ids from `id`: the main control
// `<id>-control`, the default box `<id>-box`, the rows `<id>-row-<n>`.
import { useState, type ReactNode } from 'react'
import { Icon } from './Icon.tsx'
import { t } from './strings.ts'

type Common = {
  id: string
  label: string
  hint: string
  /** The error shown under the control, when there is one. */
  error?: string
  /** A value longer than the control shows, repeated in full under it, after
   * the control and its unit and before the hint (the full-value line). Only a
   * read-only page passes it: an editable control can be scrolled. */
  full?: { id: string; text: string }
  /** The spelling a value of the file holds, under the control (the written-as
   * line). A read-only page passes it for a control that shows what the engine
   * does with the value instead of the spelling, as a checkbox and a select can
   * only do: unlike the full-value line it stays in the accessibility tree,
   * being the only place that spelling is read. */
  written?: { id: string; text: string }
}

/** What a control's accessibility tree names: in `aria-describedby` the unit
 * first, then the unset note, then the error, then the written-as line, keeping
 * the ids already there and their order; `aria-invalid` only when there is an
 * error. The full-value line is named by no control, since the control already
 * exposes that value. A unit is a description (criterion 159): "Cache RAM, edit
 * text, MiB" is read, so the unit element takes the id `<field id>-unit` and is
 * never named by `aria-hidden`. */
function describes(
  id: string,
  error: string | undefined,
  unitId?: string,
  writtenId?: string,
  noteId?: string,
) {
  const ids: string[] = []
  if (unitId !== undefined) ids.push(unitId)
  if (noteId !== undefined) ids.push(noteId)
  if (error !== undefined) ids.push(`${id}-error`)
  if (writtenId !== undefined) ids.push(writtenId)
  return ids.length === 0
    ? {}
    : {
        ...(error === undefined ? {} : { 'aria-invalid': true }),
        'aria-describedby': ids.join(' '),
      }
}

/** The About button after the label; its name carries the label. */
export function HintToggle({
  label,
  open,
  hintId,
  onToggle,
}: {
  label: string
  open: boolean
  hintId: string
  onToggle: () => void
}) {
  return (
    <button
      type="button"
      className="about"
      aria-expanded={open}
      aria-controls={hintId}
      onClick={onToggle}
    >
      {t('field-about')} <span className="visually-hidden">{label}</span>
    </button>
  )
}

/** The hint, one paragraph per line of its text. */
export function Hint({
  id,
  text,
  open,
}: {
  id: string
  text: string
  open: boolean
}) {
  return (
    <div id={id} className="field-hint" hidden={!open}>
      {text.split('\n').map((line, i) => (
        <p key={i}>{line}</p>
      ))}
    </div>
  )
}

/** The full-value line: the value as the reply has it, under the control. The
 * control already exposes it, so it is hidden from assistive technology and
 * named by no `aria-describedby`. It is a paragraph, so it takes no tab stop. */
function FullValue({ id, text }: { id: string; text: string }) {
  return (
    <p id={id} className="full-value" aria-hidden="true">
      {text}
    </p>
  )
}

/** The written-as line: the value as the file writes it, under the control,
 * styled as the full-value line. Not `aria-hidden`, since the control does not
 * expose the spelling. It is a paragraph, so it takes no tab stop. */
function WrittenValue({ id, text }: { id: string; text: string }) {
  return (
    <p id={id} className="full-value">
      {text}
    </p>
  )
}

/** The unset line: what the control's default state means, under the control,
 * as muted text in the flow (brief 25 GT5b.1, the review's D1). A native select
 * cannot wrap it, so it sits outside the control and shows only while the
 * control is at that state; the control names it as its description. It is a
 * paragraph, so it takes no tab stop, and no live role, being a description and
 * not an announcement. */
function UnsetNote({ id, text }: { id: string; text: string }) {
  return (
    <p id={id} className="field-hint">
      {text}
    </p>
  )
}

/** A field of one control named by its label, `<id>-control`. */
function LabelField({
  id,
  label,
  hint,
  error,
  full,
  written,
  note,
  children,
}: Common & { children: ReactNode; note?: { id: string; text: string } }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="field">
      <div className="field-label">
        <label htmlFor={`${id}-control`}>{label}</label>
        <HintToggle
          label={label}
          open={open}
          hintId={`${id}-hint`}
          onToggle={() => setOpen((o) => !o)}
        />
      </div>
      <div className="field-control">{children}</div>
      {note !== undefined && <UnsetNote id={note.id} text={note.text} />}
      {full !== undefined && <FullValue id={full.id} text={full.text} />}
      {written !== undefined && (
        <WrittenValue id={written.id} text={written.text} />
      )}
      <Hint id={`${id}-hint`} text={hint} open={open} />
      {error !== undefined && <FieldError id={`${id}-error`} text={error} />}
    </div>
  )
}

/**
 * A field of several controls, or a checkbox with its own text: a group named
 * by the label, `<id>-label`, which also names the main text input.
 */
export function GroupField({
  id,
  label,
  hint,
  error,
  full,
  written,
  children,
}: Common & { children: ReactNode }) {
  const [open, setOpen] = useState(false)
  return (
    <fieldset className="field" aria-labelledby={`${id}-label`}>
      <legend className="field-label">
        <span id={`${id}-label`}>{label}</span>
        <HintToggle
          label={label}
          open={open}
          hintId={`${id}-hint`}
          onToggle={() => setOpen((o) => !o)}
        />
      </legend>
      <div className="field-control">{children}</div>
      {full !== undefined && <FullValue id={full.id} text={full.text} />}
      {written !== undefined && (
        <WrittenValue id={written.id} text={written.text} />
      )}
      <Hint id={`${id}-hint`} text={hint} open={open} />
      {error !== undefined && <FieldError id={`${id}-error`} text={error} />}
    </fieldset>
  )
}

/** A checkbox with its text. */
function Check({
  id,
  text,
  checked,
  disabled,
  inactive,
  onChange,
  aria,
}: {
  id: string
  text: string
  checked: boolean
  /** Omitted: the control is enabled. */
  disabled?: boolean
  /** aria-disabled: focusable, the caller ignores changes. */
  inactive?: boolean
  onChange: (checked: boolean) => void
  aria?: ReturnType<typeof describes>
}) {
  return (
    <label className="check">
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-disabled={inactive || undefined}
        onChange={(e) => onChange(e.target.checked)}
        {...aria}
      />
      {text}
    </label>
  )
}

/** The range of the slider a number field takes beside its text input (brief 19,
 * 19 G.13), as the caller's key takes it: the model file's layer count is the
 * top, and the key's own words say the top too. */
export type FieldRange = {
  /** The slider's `max`: every layer of the model file. */
  max: number
  /** What the slider reads while the default box is ticked, and the value it
   * sits at before it has read a number: the top for a key whose default is
   * every layer, 0 for one whose default is none. */
  atDefault: number
  /** The words the key's text takes for the top of the range (`auto`, `all`). A
   * negative number says the same, so it reads as the top with them; a key
   * without words reads one as 0. */
  words?: string[]
}

/** The number the slider reads from the field's text (19 G.13): the ticked box
 * is `atDefault`; one of the key's words, and a negative number with them, is
 * the top of the range; a whole number is clamped into it; any other text
 * (empty, `x`) leaves the slider at `last`, the number it read last. */
function rangeValue(
  text: string,
  ticked: boolean,
  range: FieldRange,
  last: number,
): number {
  if (ticked) return range.atDefault
  const s = text.trim()
  if (range.words?.includes(s)) return range.max
  if (!/^[+-]?\d+$/.test(s)) return last
  const n = Number(s)
  if (n < 0) return range.words === undefined ? 0 : range.max
  return Math.min(n, range.max)
}

/**
 * A default box (`default`, `auto`) before a text input: ticked, the input is
 * disabled and shows `text`, the default the API gave or nothing. With `range`,
 * a slider of that range sits after the input, in the same control row: it reads
 * the text and, moved, writes its value as the text (19 G.13).
 */
export function DefaultBoxField({
  id,
  label,
  hint,
  error,
  full,
  box,
  ticked,
  text,
  numeric = false,
  unit,
  range,
  disabled,
  onTick,
  onText,
}: Common & {
  box: string
  ticked: boolean
  text: string
  numeric?: boolean
  unit?: string
  /** Omitted: the field has no slider. */
  range?: FieldRange
  /** Omitted: the box is enabled and the input follows it. */
  disabled?: boolean
  onTick: (ticked: boolean) => void
  onText: (text: string) => void
}) {
  const unitId = unit === undefined ? undefined : `${id}-unit`
  // 19 G.13: the number the slider read last, so a text that is no number
  // leaves it where it stands. Kept as the field renders, the way the GPU row
  // keeps what is typed (19 G.7): the slider never moves on its own.
  const [last, setLast] = useState(range?.atDefault ?? 0)
  const value = range === undefined ? 0 : rangeValue(text, ticked, range, last)
  if (range !== undefined && value !== last) setLast(value)
  return (
    <GroupField id={id} label={label} hint={hint} error={error} full={full}>
      <Check
        id={`${id}-box`}
        text={box}
        checked={ticked}
        disabled={disabled}
        onChange={onTick}
      />
      <input
        id={`${id}-control`}
        type="text"
        className={numeric ? 'number' : 'mono'}
        inputMode={numeric ? 'numeric' : undefined}
        aria-labelledby={`${id}-label`}
        value={text}
        disabled={disabled || ticked}
        onChange={(e) => onText(e.target.value)}
        {...describes(id, error, unitId)}
      />
      {range === undefined ? null : (
        <input
          type="range"
          id={`${id}-slider`}
          aria-labelledby={`${id}-label`}
          min={0}
          max={range.max}
          step={1}
          value={value}
          disabled={disabled || ticked}
          aria-valuetext={t(
            'model-profile-layers-value',
            `${value}`,
            `${range.max}`,
          )}
          onChange={(e) => onText(e.target.value)}
        />
      )}
      {unit !== undefined && (
        <span id={unitId} className="unit">
          {unit}
        </span>
      )}
    </GroupField>
  )
}

/**
 * A secret: a `No key` box, the masked input and a Show toggle. Ticked, the
 * input and Show are disabled (5.3), since there is nothing to show.
 */
export function SecretField({
  id,
  label,
  hint,
  error,
  box,
  ticked,
  value,
  onTick,
  onText,
}: Common & {
  box: string
  ticked: boolean
  value: string
  onTick: (ticked: boolean) => void
  onText: (text: string) => void
}) {
  const [shown, setShown] = useState(false)
  return (
    <GroupField id={id} label={label} hint={hint} error={error}>
      <Check id={`${id}-box`} text={box} checked={ticked} onChange={onTick} />
      <input
        id={`${id}-control`}
        type={shown ? 'text' : 'password'}
        className="mono"
        autoComplete="off"
        aria-labelledby={`${id}-label`}
        value={value}
        disabled={ticked}
        onChange={(e) => onText(e.target.value)}
        {...describes(id, error)}
      />
      <button
        type="button"
        className="secondary"
        aria-pressed={shown}
        disabled={ticked}
        onClick={() => setShown((s) => !s)}
      >
        {t('field-show')}
      </button>
    </GroupField>
  )
}

/** A switch: one checkbox with its own text, then `children` if any. */
export function CheckboxField({
  id,
  label,
  hint,
  error,
  written,
  toggle,
  checked,
  disabled,
  inactive,
  onChange,
  children,
}: Common & {
  toggle: string
  checked: boolean
  /** Omitted: the switch is enabled. */
  disabled?: boolean
  inactive?: boolean
  onChange: (checked: boolean) => void
  children?: ReactNode
}) {
  return (
    <GroupField
      id={id}
      label={label}
      hint={hint}
      error={error}
      written={written}
    >
      <Check
        id={`${id}-control`}
        text={toggle}
        checked={checked}
        disabled={disabled}
        inactive={inactive}
        onChange={onChange}
        aria={describes(id, error, undefined, written?.id)}
      />
      {children}
    </GroupField>
  )
}

/**
 * A list of checkboxes, each with its own text; an error marks the checked
 * rows, which are the value.
 */
export function CheckListField({
  id,
  label,
  hint,
  error,
  rows,
  onToggle,
}: Common & {
  rows: { value: string; text: string; checked: boolean; disabled: boolean }[]
  onToggle: (row: number) => void
}) {
  return (
    <GroupField id={id} label={label} hint={hint} error={error}>
      <div className="check-list">
        {rows.map((r, i) => (
          <Check
            key={r.value}
            id={`${id}-row-${i}`}
            text={r.text}
            checked={r.checked}
            disabled={r.disabled}
            onChange={() => onToggle(i)}
            aria={r.checked ? describes(id, error) : undefined}
          />
        ))}
      </div>
    </GroupField>
  )
}

/** A path: a text input, the placeholder saying what an empty one means. The
 * text is typed by hand, so nothing spells, capitalizes, autocorrects it or
 * offers a value for it. The label is tied to the input, on a read-only page
 * and on a form alike. */
export function TextField({
  id,
  label,
  hint,
  error,
  full,
  value,
  placeholder,
  unit,
  disabled,
  onChange,
  onApply,
}: Common & {
  value: string
  placeholder: string
  /** The literal unit after the input (5.2), where the caller gives one. */
  unit?: string
  /** Omitted: the input is enabled. */
  disabled?: boolean
  onChange: (value: string) => void
  /** Omitted: every change applies the text, as the other fields do. Where it
   * is given the text is a draft of its own: Enter and the focus leaving apply
   * it (21 T.10's Rules as text). */
  onApply?: () => void
}) {
  const unitId = unit === undefined ? undefined : `${id}-unit`
  return (
    <LabelField id={id} label={label} hint={hint} error={error} full={full}>
      <input
        id={`${id}-control`}
        type="text"
        className="mono"
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        autoComplete="off"
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && onApply !== undefined) onApply()
        }}
        onBlur={onApply}
        {...describes(id, error, unitId)}
      />
      {unit !== undefined && (
        <span id={unitId} className="unit">
          {unit}
        </span>
      )}
    </LabelField>
  )
}

/** A native select. `note` is the unset line of 25 GT5b.1: the caller passes it
 * only while the select is at the state an absent key shows, and the select
 * names it as its description. */
export function SelectField({
  id,
  label,
  hint,
  error,
  full,
  written,
  note,
  options,
  value,
  disabled,
  onChange,
}: Common & {
  options: { value: string; text: string }[]
  value: string
  /** Omitted: the select is enabled. */
  disabled?: boolean
  note?: { id: string; text: string }
  /** `picked` is false when the change left the select with no selected
   * option: the value is not in its list, so it is not a pick the control
   * holds. A page that shows the value it lists (as an extra option) never
   * sees a false, only a value set from outside the list. */
  onChange: (value: string, picked: boolean) => void
}) {
  return (
    <LabelField
      id={id}
      label={label}
      hint={hint}
      error={error}
      full={full}
      written={written}
      note={note}
    >
      <select
        id={`${id}-control`}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value, e.target.selectedIndex >= 0)}
        {...describes(id, error, undefined, written?.id, note?.id)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.text}
          </option>
        ))}
      </select>
    </LabelField>
  )
}

/** A field's error: under its control, an icon with no text of its own first.
 *  The icon is the shared one (08-hearth-h8-icons.md H8b.6), so the error is
 *  drawn like every other icon and takes the error's colour from the text.
 *  `alert` makes it the page's alert too, for a refusal the control names: the
 *  words are read once, by the control that they describe. */
export function FieldError({
  id,
  text,
  alert,
}: {
  id: string
  text: string
  alert?: boolean
}) {
  return (
    <p
      id={id}
      className="field-error"
      role={alert === true ? 'alert' : undefined}
    >
      <Icon name="circle-alert" />
      {text}
    </p>
  )
}
