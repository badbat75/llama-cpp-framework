// The Field of fe/DESIGN.md, section 5.2: one setting, its label with an
// About toggle that reveals the hint, the control, then the hint. The
// controls are read-only here: every one is disabled.
import { useId, useState, type ReactNode } from 'react'
import { t } from './strings.ts'

type Common = { label: string; hint: string }

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

/** A field of one control named by its label; `children` gets the control's id. */
function LabelField({
  label,
  hint,
  children,
}: Common & { children: (id: string) => ReactNode }) {
  const id = useId()
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
      <div className="field-control">{children(`${id}-control`)}</div>
      <Hint id={`${id}-hint`} text={hint} open={open} />
    </div>
  )
}

/**
 * A field of several controls, or a checkbox with its own text: a group named
 * by the label; `children` gets the label's id, which also names the main
 * text input.
 */
function GroupField({
  label,
  hint,
  children,
}: Common & { children: (labelId: string) => ReactNode }) {
  const id = useId()
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
      <div className="field-control">{children(`${id}-label`)}</div>
      <Hint id={`${id}-hint`} text={hint} open={open} />
    </fieldset>
  )
}

/** A checkbox with its text. */
function Check({ text, checked }: { text: string; checked: boolean }) {
  return (
    <label className="check">
      <input type="checkbox" checked={checked} disabled />
      {text}
    </label>
  )
}

/**
 * A default box (`default`, `auto`) before a text input: ticked, the input
 * shows `text`, the default the API gave or nothing.
 */
export function DefaultBoxField({
  label,
  hint,
  box,
  ticked,
  text,
  numeric = false,
  unit,
}: Common & {
  box: string
  ticked: boolean
  text: string
  numeric?: boolean
  unit?: string
}) {
  return (
    <GroupField label={label} hint={hint}>
      {(labelId) => (
        <>
          <Check text={box} checked={ticked} />
          <input
            type="text"
            className={numeric ? 'number' : 'mono'}
            inputMode={numeric ? 'numeric' : undefined}
            aria-labelledby={labelId}
            value={text}
            disabled
          />
          {unit !== undefined && <span className="unit">{unit}</span>}
        </>
      )}
    </GroupField>
  )
}

/** A secret: a `No key` box, the masked input and a Show toggle. */
export function SecretField({
  label,
  hint,
  box,
  value,
}: Common & { box: string; value: string }) {
  const [shown, setShown] = useState(false)
  return (
    <GroupField label={label} hint={hint}>
      {(labelId) => (
        <>
          <Check text={box} checked={value === ''} />
          <input
            type={shown ? 'text' : 'password'}
            className="mono"
            autoComplete="off"
            aria-labelledby={labelId}
            value={value}
            disabled
          />
          <button
            type="button"
            className="secondary"
            aria-pressed={shown}
            onClick={() => setShown((s) => !s)}
          >
            {t('field-show')}
          </button>
        </>
      )}
    </GroupField>
  )
}

/** A switch: one checkbox with its own text. */
export function CheckboxField({
  label,
  hint,
  toggle,
  checked,
}: Common & { toggle: string; checked: boolean }) {
  return (
    <GroupField label={label} hint={hint}>
      {() => <Check text={toggle} checked={checked} />}
    </GroupField>
  )
}

/** A list of checkboxes, each with its own text. */
export function CheckListField({
  label,
  hint,
  rows,
}: Common & { rows: { value: string; text: string; checked: boolean }[] }) {
  return (
    <GroupField label={label} hint={hint}>
      {() => (
        <div className="check-list">
          {rows.map((r) => (
            <Check key={r.value} text={r.text} checked={r.checked} />
          ))}
        </div>
      )}
    </GroupField>
  )
}

/** A path: a text input, the placeholder saying what an empty one means. */
export function TextField({
  label,
  hint,
  value,
  placeholder,
}: Common & { value: string; placeholder: string }) {
  return (
    <LabelField label={label} hint={hint}>
      {(id) => (
        <input
          id={id}
          type="text"
          className="mono"
          value={value}
          placeholder={placeholder}
          disabled
        />
      )}
    </LabelField>
  )
}

/** A native select. */
export function SelectField({
  label,
  hint,
  options,
  value,
}: Common & { options: { value: string; text: string }[]; value: string }) {
  return (
    <LabelField label={label} hint={hint}>
      {(id) => (
        <select id={id} value={value} disabled>
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.text}
            </option>
          ))}
        </select>
      )}
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
