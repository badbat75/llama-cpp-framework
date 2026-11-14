// The Server form of an instance (fe/briefs/01-server.md, section 5): the
// cards and fields of pai-config's Server tab, in its order, each control
// showing the file's value. Read-only: every control is disabled.
import Card from './Card.tsx'
import {
  CheckboxField,
  CheckListField,
  DefaultBoxField,
  SecretField,
  SelectField,
  TextField,
} from './Field.tsx'
import { t } from './strings.ts'

/** Values by INI key name, typed JSON. */
type Keys = Record<string, unknown>

/** The server document of GET /api/instances/{id}/server, as far as the form reads it. */
export type ServerDoc = {
  values: Keys
  defaults: Keys
  interfaces: { value: string; label: string }[]
}

/** llama-server's log levels, by number (data, not text). */
const LOG_LEVELS = [
  'OUTPUT:0',
  'ERROR:1',
  'WARN:2',
  'INFO:3',
  'TRACE:4',
  'DEBUG:5',
]

/** The text of a value from the file: a number or a string, else nothing. */
function shown(value: unknown): string {
  return typeof value === 'number' || typeof value === 'string'
    ? String(value)
    : ''
}

/** A key with a default box: unset shows the default the API gave. */
function withDefault(value: unknown, fallback: unknown) {
  const ticked = value === null || value === undefined
  return { ticked, text: shown(ticked ? fallback : value) }
}

/** A thread count: unset is auto, with an empty input. */
function auto(value: unknown) {
  const ticked = value === null || value === undefined
  return { ticked, text: ticked ? '' : shown(value) }
}

/** The log level's option: the number clamped to 0..5, unset TRACE:4. */
function logLevel(value: unknown): string {
  if (typeof value !== 'number') return '4'
  return String(Math.min(5, Math.max(0, Math.trunc(value))))
}

/** A select's value: one of `known`, anything else `default`. */
function oneOf(value: unknown, known: string[]): string {
  return typeof value === 'string' && known.includes(value) ? value : 'default'
}

/**
 * The Bind to rows: localhost, 0.0.0.0, each chosen value matching no row,
 * then the interfaces; a row is checked when the comma list names it (no
 * case), localhost when the list is empty.
 */
function bindRows(hostname: unknown, interfaces: ServerDoc['interfaces']) {
  const list = (typeof hostname === 'string' ? hostname : '')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s !== '')
  const chosen = list.length > 0 ? list : ['localhost']
  const fixed = [
    { value: 'localhost', text: t('server-bind-localhost') },
    { value: '0.0.0.0', text: t('server-bind-all') },
  ]
  const ifaces = interfaces.map((i) => ({ value: i.value, text: i.label }))
  const known = [...fixed, ...ifaces].map((r) => r.value.toLowerCase())
  const gone = chosen
    .filter((v) => !known.includes(v.toLowerCase()))
    .filter(
      (v, i, all) =>
        all.findIndex((w) => w.toLowerCase() === v.toLowerCase()) === i,
    )
    .map((v) => ({ value: v, text: t('server-bind-gone', v) }))
  const picked = chosen.map((v) => v.toLowerCase())
  return [...fixed, ...gone, ...ifaces].map((r) => ({
    ...r,
    checked: picked.includes(r.value.toLowerCase()),
  }))
}

export default function ServerForm({
  doc,
  canConfigure,
}: {
  doc: ServerDoc
  canConfigure: boolean
}) {
  const v = doc.values
  const d = doc.defaults
  return (
    <>
      {!canConfigure && <p className="notice">{t('server-read-only')}</p>}
      <Card title={t('server-network')}>
        <DefaultBoxField
          label={t('server-port')}
          hint={t('server-port-hint')}
          box={t('field-default')}
          numeric
          {...withDefault(v.Port, d.Port)}
        />
        <CheckListField
          label={t('server-bind-to')}
          hint={t('server-bind-to-hint')}
          rows={bindRows(v.Hostname, doc.interfaces)}
        />
        <DefaultBoxField
          label={t('server-base-url')}
          hint={t('server-base-url-hint')}
          box={t('field-default')}
          {...withDefault(v.OpencodeBaseUrl, d.OpencodeBaseUrl)}
        />
        <SecretField
          label={t('server-api-key')}
          hint={t('server-api-key-hint')}
          box={t('field-no-key')}
          value={shown(v.OpencodeApiKey)}
        />
      </Card>
      <Card title={t('server-resources')}>
        <DefaultBoxField
          label={t('server-cpu-threads')}
          hint={t('server-cpu-threads-hint')}
          box={t('field-auto')}
          numeric
          {...auto(v.Threads)}
        />
        <DefaultBoxField
          label={t('server-batch-threads')}
          hint={t('server-batch-threads-hint')}
          box={t('field-auto')}
          numeric
          {...auto(v.ThreadsBatch)}
        />
        <DefaultBoxField
          label={t('server-max-models')}
          hint={t('server-max-models-hint')}
          box={t('field-default')}
          numeric
          {...withDefault(v.ModelsMax, d.ModelsMax)}
        />
        <DefaultBoxField
          label={t('server-memory-margin')}
          hint={t('server-memory-margin-hint')}
          box={t('field-default')}
          numeric
          unit="MiB"
          {...withDefault(v.ModelsMemoryMargin, d.ModelsMemoryMargin)}
        />
        <TextField
          label={t('server-models-folder')}
          hint={t('server-models-folder-hint')}
          value={shown(v.ModelsDir)}
          placeholder={t('server-folder-instance-default', shown(d.ModelsDir))}
        />
      </Card>
      <Card title={t('server-advanced')}>
        <CheckboxField
          label={t('server-web-ui')}
          hint={t('server-web-ui-hint')}
          toggle={t('server-web-ui-toggle')}
          checked={v.Webui === true}
        />
        <CheckboxField
          label={t('server-mcp-proxy')}
          hint={t('server-mcp-proxy-hint')}
          toggle={t('server-mcp-proxy-toggle')}
          checked={v.WebuiMcpProxy === true}
        />
        <SelectField
          label={t('server-log-level')}
          hint={t('server-log-level-hint')}
          options={LOG_LEVELS.map((text, i) => ({ value: String(i), text }))}
          value={logLevel(v.LogVerbosity)}
        />
      </Card>
      <Card title={t('server-snapshots')} intro={t('server-snapshots-intro')}>
        <CheckboxField
          label={t('server-save-on-shutdown')}
          hint={t('server-save-on-shutdown-hint')}
          toggle={t('server-save-on-shutdown-toggle')}
          checked={v.SaveStateOnShutdown === true}
        />
        <TextField
          label={t('server-snapshot-folder')}
          hint={t('server-snapshot-folder-hint')}
          value={shown(v.StateDir)}
          placeholder={t('server-folder-instance-default', shown(d.StateDir))}
        />
      </Card>
      <Card title={t('server-sdk')} intro={t('server-sdk-intro')}>
        <div className="subgroup">
          <h4>{'CUDA'}</h4>
          <p className="intro">{t('server-cuda-nothing')}</p>
        </div>
        <div className="subgroup">
          <h4>{'ROCm'}</h4>
          <SelectField
            label="hipBLASLt"
            hint={t('server-hipblaslt-hint')}
            options={[
              { value: 'default', text: t('option-default') },
              { value: 'on', text: t('option-on') },
              { value: 'off', text: t('option-off') },
            ]}
            value={oneOf(v.RocblasUseHipblaslt, ['on', 'off'])}
          />
          <CheckboxField
            label={t('server-profiler-attach')}
            hint={t('server-profiler-attach-hint')}
            toggle={t('server-profiler-attach-toggle')}
            checked={v.RocprofAttach === true}
          />
        </div>
        <div className="subgroup">
          <h4>{t('server-cuda-and-rocm')}</h4>
          <SelectField
            label={t('server-allreduce')}
            hint={t('server-allreduce-hint')}
            options={[
              { value: 'default', text: t('option-default') },
              { value: 'nccl', text: 'nccl' },
              { value: 'internal', text: t('option-internal') },
              { value: 'none', text: t('option-none') },
            ]}
            value={oneOf(v.CudaAllreduce, ['nccl', 'internal', 'none'])}
          />
          <CheckboxField
            label={t('server-gpu-graphs')}
            hint={t('server-gpu-graphs-hint')}
            toggle={t('server-gpu-graphs-toggle')}
            checked={v.CudaDisableGraphs === true}
          />
        </div>
        <div className="subgroup">
          <h4>{'llama.cpp'}</h4>
          <CheckboxField
            label={t('server-kv-rotation')}
            hint={t('server-kv-rotation-hint')}
            toggle={t('server-kv-rotation-toggle')}
            checked={v.AttnRotDisable === true}
          />
        </div>
      </Card>
    </>
  )
}
