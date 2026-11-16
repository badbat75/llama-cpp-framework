// The Field of fe/DESIGN.md, section 5.2: one setting, its label with an
// About toggle that reveals the hint, the control, then the hint and the
// error. The form owns the values; a field only shows them and reports
// changes. Each field's elements take their ids from `id`: the main control
// `<id>-control`, the default box `<id>-box`, the rows `<id>-row-<n>`.
import { useState, type ReactNode } from 'react'
import { t } from './strings.ts'

type Common = {
  id: string
  label: string
  hint: string
  /** The error shown under the control, when there is one. */
  error?: string
}

/** What marks a control invalid and names its error. */
function invalid(id: string, error: string | undefined) {
  return error === undefined
    ? {}
    : { 'aria-invalid': true, 'aria-describedby': `${id}-error` }
}

/** The About button after the label; its name carries the label. */
function HintToggle({
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
function Hint({ id, text, open }: { id: string; text: string; open: boolean }) {
  return (
    <div id={id} className="field-hint" hidden={!open}>
      {text.split('\n').map((line, i) => (
        <p key={i}>{line}</p>
      ))}
    </div>
  )
}

/** A field of one control named by its label, `<id>-control`. */
function LabelField({
  id,
  label,
  hint,
  error,
  children,
}: Common & { children: ReactNode }) {
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
  aria?: ReturnType<typeof invalid>
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

/**
 * A default box (`default`, `auto`) before a text input: ticked, the input is
 * disabled and shows `text`, the default the API gave or nothing.
 */
export function DefaultBoxField({
  id,
  label,
  hint,
  error,
  box,
  ticked,
  text,
  numeric = false,
  unit,
  disabled,
  onTick,
  onText,
}: Common & {
  box: string
  ticked: boolean
  text: string
  numeric?: boolean
  unit?: string
  /** Omitted: the box is enabled and the input follows it. */
  disabled?: boolean
  onTick: (ticked: boolean) => void
  onText: (text: string) => void
}) {
  return (
    <GroupField id={id} label={label} hint={hint} error={error}>
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
        {...invalid(id, error)}
      />
      {unit !== undefined && <span className="unit">{unit}</span>}
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
        {...invalid(id, error)}
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
    <GroupField id={id} label={label} hint={hint} error={error}>
      <Check
        id={`${id}-control`}
        text={toggle}
        checked={checked}
        disabled={disabled}
        inactive={inactive}
        onChange={onChange}
        aria={invalid(id, error)}
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
            aria={r.checked ? invalid(id, error) : undefined}
          />
        ))}
      </div>
    </GroupField>
  )
}

/** A path: a text input, the placeholder saying what an empty one means. */
export function TextField({
  id,
  label,
  hint,
  error,
  value,
  placeholder,
  disabled,
  onChange,
}: Common & {
  value: string
  placeholder: string
  /** Omitted: the input is enabled. */
  disabled?: boolean
  onChange: (value: string) => void
}) {
  return (
    <LabelField id={id} label={label} hint={hint} error={error}>
      <input
        id={`${id}-control`}
        type="text"
        className="mono"
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        {...invalid(id, error)}
      />
    </LabelField>
  )
}

/** A native select. */
export function SelectField({
  id,
  label,
  hint,
  error,
  options,
  value,
  disabled,
  onChange,
}: Common & {
  options: { value: string; text: string }[]
  value: string
  /** Omitted: the select is enabled. */
  disabled?: boolean
  onChange: (value: string) => void
}) {
  return (
    <LabelField id={id} label={label} hint={hint} error={error}>
      <select
        id={`${id}-control`}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        {...invalid(id, error)}
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

/** A field's error: under its control, an icon with no text of its own first. */
export function FieldError({ id, text }: { id: string; text: string }) {
  return (
    <p id={id} className="field-error">
      <svg
        className="error-icon"
        viewBox="0 0 16 16"
        width="16"
        height="16"
        aria-hidden="true"
      >
        <circle cx="8" cy="8" r="7" fill="currentColor" />
        <rect x="7" y="3.5" width="2" height="6" fill="var(--bg)" />
        <rect x="7" y="10.5" width="2" height="2" fill="var(--bg)" />
      </svg>
      {text}
    </p>
  )
}
