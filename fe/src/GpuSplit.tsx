// The GPU distribution card of the model profile page (brief 14): the split
// mode select, in ModelProfileCards (whose `alias` carries `layer` to the
// default), and the GPUs fieldset, written here: the host's GPUs read once per
// arrival (GD1.3), the ones this profile names, the split order of the checked
// rows and their weights (GD2). `device` and `tensor-split` are composed from
// this state and never typed (GD1.10, GD2.5), so they are no field of the
// cards' table and the page composes them with the keys the form sends.
// Every text is a key of en_US.json.
import { useState } from 'react'
import { t } from './strings.ts'
import {
  isRecord,
  type ApiError,
  type ModelFacts,
  type ModelInfoRead,
} from './api.ts'
import { blockCounts, blockRanges, layout, type Layout } from './gpuBlocks.ts'

/** One GPU of the engine's probe. `name` is null when the answer carries none
 * the page can show; the memory figures when it carries them. */
export type GpuDevice = {
  id: string
  name: string | null
  totalMib: number | null
  freeMib: number | null
}

/** What the device read answered: the GPUs in the engine's probe order, and
 * the moment the probe ran (null when it gave no answer). `probeAnswered` is
 * that moment non-null: a row the probe does not list says so only once the
 * probe answered (GD1.8); `asOfShown` is the same moment readable as a date,
 * which is the only moment the as-of line shows (GD1.7). */
export type GpuProbe = {
  devices: GpuDevice[]
  probedAt: string | null
  probeAnswered: boolean
  asOfShown: boolean
}

/** One row of the list: an id, and the probe's device behind it when the probe
 * lists one. */
export type GpuRow = { id: string; device: GpuDevice | null }

/** A number of the answer, `null` for anything else (GD1.6). */
function number_(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

/** The device read's body, read: `null` for a body that is no device list at
 * all (GD1.7). A device with no id, and the CPU, take no row (GD1.5). */
export function parseDeviceProbe(body: unknown): GpuProbe | null {
  if (!isRecord(body) || !Array.isArray(body.devices)) return null
  const probedAt = typeof body.probedAt === 'string' ? body.probedAt : null
  const devices: GpuDevice[] = []
  for (const item of body.devices) {
    if (!isRecord(item)) continue
    const id = typeof item.id === 'string' ? item.id.trim() : ''
    if (id === '' || id.toUpperCase() === 'CPU') continue
    devices.push({
      id,
      name:
        typeof item.name === 'string' && item.name !== '' ? item.name : null,
      totalMib: number_(item.totalMib),
      freeMib: number_(item.freeMib),
    })
  }
  return {
    devices,
    probedAt,
    probeAnswered: probedAt !== null,
    asOfShown: probedAt !== null && !Number.isNaN(new Date(probedAt).getTime()),
  }
}

/** The device read's limit (GD1.3): the bff answers a cold probe within 35 s,
 * so 40 s with no answer is a read that answered nothing. */
const DEVICE_LIMIT_MS = 40_000

/** The host's GPUs, read once (GD1.3): `GET /api/node/devices`, its own body
 * parsed. `null` is a read that answered nothing: no session, no manager, a
 * body that is no device list, no answer in time, no network. It is no
 * session-ended answer: a page that cannot read its GPUs keeps its session
 * (GD1.7). */
export async function readGpuProbe(): Promise<GpuProbe | null> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), DEVICE_LIMIT_MS)
  try {
    const reply = await fetch('/api/node/devices', {
      signal: controller.signal,
    })
    if (reply.status !== 200) return null
    return parseDeviceProbe(await reply.json())
  } catch {
    return null
  } finally {
    clearTimeout(timer)
  }
}

/** The ids a `device` names: on `,`, the parts trimmed, the empty ones
 * dropped, each spelling kept once, in the file's order (GD1.5). A `/` is part
 * of an id here, where llama.cpp's device names are the whole entry. */
export function gpuFileIds(device: string | null): string[] {
  const ids: string[] = []
  for (const part of (device ?? '').split(/,+/)) {
    const id = part.trim()
    if (id !== '' && !gpuHas(ids, id)) ids.push(id)
  }
  return ids
}

/** Whether `ids` holds `id`, ignoring case: a device name of the file and the
 * probe's are the same GPU when they differ only in case. */
export function gpuHas(ids: string[], id: string): boolean {
  return ids.some((one) => one.toLowerCase() === id.toLowerCase())
}

/** Where `id` sits among the checked ids, read as `gpuHas` reads them (the
 * probe's spelling and the file's differ in case): -1 when it is not one. A
 * row's block count is read out of the vector at this place (19 G.5), and the
 * page writes it back at the same one (19 G.6). */
export function gpuIndexOf(ids: string[], id: string): number {
  return ids.findIndex((one) => one.toLowerCase() === id.toLowerCase())
}

/** The list's ids: the ids the file names, in its order, then the probe's GPUs
 * the file does not name, in probe order (GD1.5). The file's own ids are the
 * LOADED ones, so a row the user unchecked keeps its place in the list (GD1.8);
 * before the read lands the list is the file's ids alone, so a profile with
 * nothing checked needs no read (GD1.1). */
export function gpuRowIds(
  loaded: string[],
  probe: GpuProbe | null | undefined,
): string[] {
  const fresh = (probe?.devices ?? []).filter(
    (device) => !gpuHas(loaded, device.id),
  )
  return [...loaded, ...fresh.map((device) => device.id)]
}

/** The rows of `ids`, each with the probe's device that answers to it (the
 * ids of the file and the probe are the same GPU when they differ only in
 * case). */
export function gpuRows(
  ids: string[],
  probe: GpuProbe | null | undefined,
): GpuRow[] {
  return ids.map((id) => ({
    id,
    device:
      probe?.devices.find((one) => one.id.toLowerCase() === id.toLowerCase()) ??
      null,
  }))
}

/** What `device` sends: the checked ids in the split order, `null` with none
 * (GD1.10). */
export function gpuComposeDevice(checked: string[]): string | null {
  return checked.length === 0 ? null : checked.join(',')
}

/** The file's `tensor-split` on the rows' weights: split on `,` and `/`, each
 * entry trimmed, entry `i` the weight of the `i`-th of `ids` exactly as
 * written; a missing entry is `""` and extra entries are not shown (GD2.1). */
export function gpuFileWeights(
  value: string | null,
  ids: string[],
): Record<string, string> {
  if (value === null || value === '') return {}
  const parts = gpuSplitParts(value)
  return Object.fromEntries(ids.map((id, index) => [id, parts[index] ?? '']))
}

/** The parts of a `tensor-split` as written, trimmed. A note between
 * parentheses after the last weight is not part of the value. */
function gpuSplitParts(value: string): string[] {
  return value
    .replace(/\s*\([^()]*\)\s*$/, '')
    .split(/[,/]+/)
    .map((part) => part.trim())
}

/** What the file's `tensor-split` counts as loaded (GD2.5): its entries
 * trimmed and joined with `,`, `null` when the key is absent or empty. */
export function gpuSplitLoaded(value: string | null): string | null {
  return value === null || value === '' ? null : gpuSplitParts(value).join(',')
}

/** What a row that joins the checked ones starts at (GD2.3): 1 when another
 * checked row has a weight, empty when none has. */
export function gpuNewWeight(
  checked: string[],
  weights: Record<string, string>,
): string {
  return checked.some((id) => (weights[id] ?? '') !== '') ? '1' : ''
}

/** What `tensor-split` sends: the checked rows' weights in the split order,
 * `null` with fewer than two checked rows (the vector has nothing to say
 * there) and with every weight empty (the Auto state, GD2.5). */
export function gpuComposeSplit(
  checked: string[],
  weights: Record<string, string>,
): string | null {
  if (checked.length < 2) return null
  const list = checked.map((id) => (weights[id] ?? '').trim())
  return list.every((weight) => weight === '') ? null : list.join(',')
}

/** The shape of a weight of 0 or more: digits with at most one dot (GD2.6).
 * The check and the share read every weight through it. */
export const GPU_WEIGHT_RULE = /^(?:\d+(?:\.\d*)?|\.\d+)$/

/** A row's weight as the check and the share read it: trimmed, so what is
 * typed around a number is not part of it (GD2.5 sends ' 5 ' as '5'). */
function gpuWeight(weights: Record<string, string>, id: string): string {
  return (weights[id] ?? '').trim()
}

/** Whether every written weight of the vector passes the rule. An empty weight
 * is no number, and is left out here: the all-empty vector is the Auto state,
 * which says `even` or `auto` (GD2.2) and is checked as nothing (GD2.6). */
function gpuWeightsKnown(list: string[]): boolean {
  return list.every((weight) => weight === '' || GPU_WEIGHT_RULE.test(weight))
}

/** The weights that fail the fe's check (GD2.6). The check runs on the
 * composed vector, so a weight that is empty beside a written one does not
 * match the rule and fails: the one exempt state is the vector with nothing
 * written, which composes `null`, the Auto state (GD2.5). */
export function gpuWeightFailures(
  checked: string[],
  weights: Record<string, string>,
): string[] {
  const list = checked.map((id) => [id, gpuWeight(weights, id)] as const)
  if (list.every(([, weight]) => weight === '')) return []
  return list
    .filter(([, weight]) => !GPU_WEIGHT_RULE.test(weight))
    .map(([id]) => id)
}

/** The split mode as the fieldset reads it: `none` is the one mode that hides
 * the weights (GD2.1). Any other spelling, the default included, splits. */
function splits(mode: string | null): boolean {
  return gpuSplitModeText(mode) !== 'none'
}

/** The split mode's value the select shows for the file's spelling (GD1.2). */
export function gpuSplitModeText(mode: string | null | undefined): string {
  const raw = mode ?? ''
  return raw.trim().toLowerCase() === 'layer' ? '' : raw
}

/** The weights show with a mode that splits, and two rows checked (GD2.1): a
 * split of one GPU has no weights to give, whatever the file's `device` names,
 * so a row unchecked out of a two GPU profile shows no field beside the one
 * left. The weights are kept as typed and come back when this holds again. */
export function gpuWeightsShown(mode: string | null, checked: string[]) {
  return splits(mode) && checked.length >= 2
}

/** The split mode the file spells as its own layer mode (brief 19, 19 G.4):
 * the key absent, empty, or the word `layer` exactly as the file writes it. Any
 * other spelling, an added option like `Layer` included, is a mode the fe does
 * not know how to cut into blocks, so its rows keep their weights. */
function gpuLayerMode(mode: string | null): boolean {
  const raw = (mode ?? '').trim()
  return raw === '' || raw === 'layer'
}

/** GPU layers as the layout reads it (brief 19, 19 G.4): the default box,
 * `auto`, `all` and anything that is not a whole number are every layer, and a
 * whole number of 0 or more is that number, 0 with it. */
function gpuLayersCount(layers: string | null): number {
  const text = (layers ?? '').trim()
  return /^\d+$/.test(text) ? Number(text) : -1
}

/** The model file's layer facts, when the read answered them (brief 19,
 * 19 G.4): `null` while it runs and for a read that answered a reason, the two
 * states that leave the rows as brief 14 built them. */
function gpuFacts(read: ModelInfoRead | undefined): ModelFacts | null {
  if (read === undefined || read === null) return null
  return 'reason' in read ? null : read.facts
}

/** What a row's count is worth of the places, in GD2.2's share (brief 19,
 * 19 G.5): the count over the places the layout has, rounded to a percent. */
function gpuCountShare(count: number, places: number): string {
  return t('model-profile-gpu-share', `${Math.round((count * 100) / places)}%`)
}

/** The layout the blocks are cut over (brief 19, 19 G.4): GPU layers of the
 * model file with the blocks the facts read counted, when the mode is the
 * file's own and two rows are checked. `null` is not blocks mode: a read that
 * runs or answered a reason, a mode the fe cuts no blocks with, a split of one
 * row, an offload of no layer. The page reads it too, so the counts a row shows
 * and the counts a save writes are the same cut. */
export function gpuLayout(
  modelInfo: ModelInfoRead | undefined,
  layers: string | null,
  mode: string | null,
  rows: number,
): Layout | null {
  const facts = gpuFacts(modelInfo)
  if (facts === null || !gpuLayerMode(mode) || rows < 2) return null
  return layout(facts.blockCount, gpuLayersCount(layers))
}

/** What each checked row's weight is worth, as the text beside the field
 * (GD2.2): with `none`, the first row is all of it and the rest go unused;
 * with one row, that row is all of it; a vector of nothing is `even` under
 * `tensor` and `auto` under every other mode; a weight the check's rule
 * refuses, an empty one beside a written one, says nothing; the others are
 * their share. A row with nothing to say shows no line. */
export function gpuShares(
  checked: string[],
  weights: Record<string, string>,
  mode: string | null,
): Record<string, string> {
  const state: Record<string, string> = {}
  if (checked.length === 0) return state
  if (checked.length === 1 || !splits(mode)) {
    checked.forEach((id, index) => {
      state[id] =
        index === 0
          ? t('model-profile-gpu-share', '100%')
          : t('model-profile-gpu-share-unused')
    })
    return state
  }
  const list = checked.map((id) => gpuWeight(weights, id))
  const words = list.map((weight) => Number(weight))
  const sum = words.reduce((all, word) => all + word, 0)
  // The shares show only on a vector every weight of which passes the rule:
  // what JavaScript reads out of `-1` or `1e3` is not the number llama.cpp
  // reads, so a row beside it has no share to give.
  const known = gpuWeightsKnown(list)
  // Nothing written, or every weight a 0: the split is the mode's own. Only
  // `tensor` cuts evenly on a blank vector; every other mode, the default
  // `layer` included, spreads the layers by the free VRAM each GPU has at load
  // (GD2.2), which the row calls `auto`.
  const flat = known && sum === 0
  // A weight that is empty beside a written one leaves the cut unknown, so no
  // row says anything.
  const exact = known && sum > 0 && list.every((weight) => weight !== '')
  const word =
    gpuSplitModeText(mode) === 'tensor'
      ? t('model-profile-gpu-share-even')
      : t('model-profile-gpu-share-auto')
  checked.forEach((id, index) => {
    if (flat) state[id] = word
    else if (exact)
      state[id] = t(
        'model-profile-gpu-share',
        `${Math.round((words[index]! * 100) / sum)}%`,
      )
  })
  return state
}

/** The GiB line of a probe row, in the brief's figures (GD1.6). A device with
 * no memory figure shows no line. */
function memoryOf(device: GpuDevice): string | null {
  const total = device.totalMib
  if (total === null || total <= 0) return null
  return t(
    'model-profile-gpu-memory',
    (total / 1024).toFixed(1),
    ((device.freeMib ?? 0) / 1024).toFixed(1),
  )
}

/** The as-of line of a probe the engine answered (GD1.7): the sentence, with a
 * `time` element in its `{0}` slot: the element's text is the moment in the
 * locale's date and short time, its `datetime` the moment as the engine sent
 * it. The element sits inside the sentence, so the paragraph names the moment
 * once and a screen reader reads the date from the `time`. */
function AsOfLine({ probedAt }: { probedAt: string }) {
  const shown = new Date(probedAt).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
  const parts = t('model-profile-gpu-probed-at', shown).split(shown)
  return (
    <p className="muted">
      {parts[0]}
      <time dateTime={probedAt}>{shown}</time>
      {parts[1]}
    </p>
  )
}

/** A direction of a move. */
export type GpuDirection = 'up' | 'down'

/** The page's GPU state, as the cards' table passes it to the fieldset: the
 * device read, the rows' weights, the marks of the last check, and the
 * handlers. The rows and their checks are the draft's `device`, which the page
 * writes: the fieldset composes it, it is never typed. */
export type GpuCardProps = {
  /** The device read: `undefined` while it runs, `null` when it failed. */
  probe: GpuProbe | null | undefined
  /** The model file's layer facts, read once per arrival beside the device
   * read (brief 19, 19 G.1): `undefined` while it runs. The card's facts line
   * speaks it (19 G.2), and blocks mode needs it, so a read that answered
   * nothing leaves the weights as ratios (19 G.4). */
  modelInfo: ModelInfoRead | undefined
  /** The `n-gpu-layers` the blocks are cut over, as the field holds it (brief
   * 19, 19 G.11): `''` is its default, every block. Changing it re-projects the
   * rows, and writes nothing of its own. */
  layers: string
  /** The rows' weights, by id, dropped when its row is unchecked. */
  weights: Record<string, string>
  /** The ids of the weights that fail the fe's check; `null` marks every
   * weight field, which is what a refusal of the server does (GD2.6, GD2.7). */
  weightIds?: string[] | null
  onToggle: (id: string) => void
  onMove: (id: string, dir: GpuDirection) => void
  onWeight: (id: string, value: string) => void
  /** A blocks slider moved (19 G.6): the page writes the new counts into the
   * weights, from the base `onBlockBase` took. */
  onBlock: (id: string, raw: string) => void
  /** A blocks number applied (19 G.7): Enter or blur, clamped to the places. */
  onBlockCommit: (id: string, raw: string) => void
  /** The weights a drag starts from, kept until it ends. */
  onBlockBase: () => void
  onBlockRelease: () => void
  onAuto: () => void
  onEven: () => void
  /** The `device` as the read gave it: its ids are the list's, so a row the
   * user unchecked keeps its place until the next read (GD1.8). */
  loadedDevice: string | null
  /** What the last move announced, `''` before one (GD1.9). */
  moved: string
}

/** The reason a facts read answered, in its own sentence: the strings key the
 * read names, filled with its args, inside `model-profile-model-facts-none`.
 * One literal `t()` call per key, as the save refusals do. A key outside the
 * four of section 0 is a read that answered nothing. */
function factsReason(reason: ApiError): string {
  const args = reason.args
  switch (reason.key) {
    case 'model-info-fetched':
      return t(
        'model-profile-model-facts-none',
        t('api-model-info-fetched', ...args),
      )
    case 'model-profile-no-model':
      return t(
        'model-profile-model-facts-none',
        t('api-model-profile-no-model', ...args),
      )
    case 'model-info-missing':
      return t(
        'model-profile-model-facts-none',
        t('api-model-info-missing', ...args),
      )
    case 'model-info-unreadable':
      return t(
        'model-profile-model-facts-none',
        t('api-model-info-unreadable', ...args),
      )
    default:
      return t('model-profile-model-facts-failed')
  }
}

/** What the GPU card's facts line says for `read` (brief 19, 19 G.2): the file
 * is being read while the read runs, the blocks it counted when it answered,
 * the reason there are none when the file has none, and the failed line for a
 * read that answered nothing. A MoE file with no expert counts says it has
 * experts and no numbers. */
export function factsLine(read: ModelInfoRead | undefined): string {
  if (read === undefined) return t('model-profile-model-facts-reading')
  if (read === null) return t('model-profile-model-facts-failed')
  if ('reason' in read) return factsReason(read.reason)
  const facts = read.facts
  if (!facts.moe) return t('model-profile-model-facts', `${facts.blockCount}`)
  return facts.expertCount > 0
    ? t(
        'model-profile-model-facts-moe',
        `${facts.blockCount}`,
        `${facts.expertCount}`,
        `${facts.expertUsed}`,
      )
    : t('model-profile-model-facts-moe-plain', `${facts.blockCount}`)
}

export type GpuFieldProps = GpuCardProps & {
  /** The draft's keys the fieldset reads, as the file spells them. */
  device: string | null
  mode: string | null
  tensorSplit: string | null
  /** The `device` refusal's text, the fieldset speaks it (GD1.11). */
  deviceError?: string
  /** The `tensor-split` refusal's text, in the weights place (GD2.7). */
  splitError?: string
  /** The ids the split error marks; absent for a refusal, which marks every
   * weight field that shows (GD2.6, GD2.7). */
  splitErrorIds?: string[]
}

/** The GPUs fieldset of the GPU distribution card, holding the card's one
 * visually hidden `role="status"` that says a move (GD1.9): after the list, in
 * the DOM and empty from the card's first render, whatever the read's state and
 * the checked rows, so the first move is announced. Nothing else in the card is
 * a live region (GD1.3, GD1.7, GD2.9). */
export function GpuFieldset({
  probe,
  modelInfo,
  device,
  loadedDevice,
  mode,
  tensorSplit,
  layers,
  weights,
  deviceError,
  splitError,
  splitErrorIds,
  onToggle,
  onMove,
  onWeight,
  onBlock,
  onBlockCommit,
  onBlockBase,
  onBlockRelease,
  onAuto,
  onEven,
  moved,
}: GpuFieldProps) {
  const checked = gpuFileIds(device)
  // The read's own lines: while it runs, that it is running; when it answered
  // nothing, that it failed; when it answered, the empty list, the moment the
  // probe ran, and the absence of one (GD1.6, GD1.7).
  const read = probe !== null && probe !== undefined
  // The empty list is the answer of a probe that RAN and found no GPU: a probe
  // that never answered says `no-probe` alone, never both (GD1.7).
  const emptyList =
    read && probe.probedAt !== null && probe.devices.length === 0
  const asOf = read && probe.asOfShown ? probe.probedAt : null
  const noProbe = read && probe.probedAt === null
  // The list's ids are the file's own, in its order, and the probe's GPUs the
  // file does not name, in probe order; the checked rows come first, in the
  // split order, the others after them (GD1.5, GD1.8). A probe that answered no
  // moment is no answer at all: only the file's ids are listed (GD1.7).
  const ids = noProbe
    ? gpuFileIds(loadedDevice)
    : gpuRowIds(gpuFileIds(loadedDevice), probe)
  const list = gpuRows(ids, probe)
  const on = checked
    .map((id) => list.find((row) => gpuHas([row.id], id)))
    .filter((row) => row !== undefined)
    .map((row) => row!)
  const off = list.filter((row) => !gpuHas(checked, row.id))
  // The three places of GD1.5: the checked rows in the split order, then the
  // GPUs the reply lists, then the file's ids the reply does not list, which
  // an unchecked row joins rather than keeping (GD1.8).
  const all = [
    ...on,
    ...off.filter((row) => row.device !== null),
    ...off.filter((row) => row.device === null),
  ]
  const shares = gpuShares(checked, weights, mode)
  // The weights fields and the two split buttons follow the one rule of GD2.1
  // and GD2.4: the fields show when the mode splits and the split has two rows
  // checked to divide between, and Auto and Even are shown while the fields
  // are.
  const fields = gpuWeightsShown(mode, checked)
  const splitButtons = fields
  // 19 G.4: blocks mode. The layout is the facts read and GPU layers, the
  // vector is the checked rows' weights: a vector of numbers projects to block
  // counts, which replace the weight fields; a vector of nothing is Auto's, and
  // says the way in; a vector with a word in it is malformed, and says that.
  // The three states are read from the file's `device` order, the same vector
  // the page composes `tensor-split` from, so the counts the rows show are the
  // counts a save writes.
  const lay = gpuLayout(modelInfo, layers, mode, checked.length)
  const vec = checked.map((id) => gpuWeight(weights, id))
  // A vector with a word in it: `tensor-split` as no number can be cut into
  // blocks, so the rows show no editor (19 G.4).
  const malformed = vec.some((w) => w !== '' && !GPU_WEIGHT_RULE.test(w))
  // Auto's vector: nothing written, or the zeros Auto leaves in the file. The
  // shares read auto for it too (GD2.2). A word the rule refuses is read first:
  // `Number` sees 0 in `0x0` and `0e1`, so a vector can be Auto's only in
  // words, and the malformed line wins over Auto's.
  const autoSplit = !malformed && vec.every((w) => w === '' || Number(w) === 0)
  const counts =
    lay === null || autoSplit || malformed ? null : blockCounts(vec, lay)
  const ranges = counts === null || lay === null ? [] : blockRanges(counts, lay)
  // The all-zero vector is Auto's, and a facts read that has not answered (or
  // a checked row of a split, while it runs) shows no editor at all: only a
  // settled read outside blocks mode leaves brief 14's weight fields.
  const weightFields = fields && lay === null && modelInfo !== undefined
  return (
    <fieldset className="field" id="model-profile-gpu-devices">
      <legend>{t('model-profile-gpus')}</legend>
      {probe === undefined ? (
        <p className="muted">{t('model-profile-gpu-detecting')}</p>
      ) : null}
      {probe === null ? (
        <p>{t('model-profile-gpu-failed')}</p>
      ) : emptyList ? (
        <p className="muted">{t('model-profile-gpu-none-found')}</p>
      ) : null}
      {asOf !== null ? <AsOfLine probedAt={asOf} /> : null}
      {noProbe ? (
        <p className="muted">{t('model-profile-gpu-no-probe')}</p>
      ) : null}
      {checked.length === 0 && (tensorSplit ?? '') !== '' ? (
        <p className="muted">
          {t('model-profile-gpu-split-without-device', tensorSplit!)}
        </p>
      ) : null}
      {counts === null && lay !== null && malformed ? (
        // 19 G.4: a vector with a word in it. Its own sentence says the state,
        // and the rows show no editor: Even is the way out.
        <p className="muted gpu-blocks-line">
          {t('model-profile-gpu-blocks-malformed', vec.join(','))}
        </p>
      ) : null}
      {counts === null && lay !== null && autoSplit ? (
        // 19 G.4: Auto. llama.cpp spreads the places by free memory at load, so
        // the fe has no counts to show: Even sets them.
        <p className="muted gpu-blocks-line">
          {t('model-profile-gpu-blocks-auto', `${lay.count}`)}
        </p>
      ) : null}
      {counts !== null ? (
        // 19 G.5: what the checked rows share, so a count typed in a row reads
        // against it.
        <p className="muted gpu-blocks-line">
          {t(
            'model-profile-gpu-blocks-budget',
            `${lay!.count}`,
            `${lay!.count - 1}`,
          )}
        </p>
      ) : null}
      <ul role="list" className="check-list">
        {all.map((row, i) => {
          // One list, keyed by id, holding the GPU rows and nothing else
          // (GD1.4): a row that is checked and one that is unchecked are the
          // same element, so the check keeps its focus through the move
          // (GD1.8) and a move moves the row, not a new one.
          const onRow = i < on.length
          // 19 G.5: the row's block count, range and share, read out of the
          // vector the page composes, in the file's order.
          const at = counts === null ? -1 : gpuIndexOf(checked, row.id)
          return (
            <GpuRowItem
              key={row.id}
              row={row}
              probe={probe}
              checked={onRow}
              at={onRow ? i : -1}
              count={checked.length}
              weight={weights[row.id] ?? ''}
              share={shares[row.id]}
              deviceError={deviceError}
              splitError={splitError}
              splitErrorIds={splitErrorIds}
              weightsShown={onRow ? weightFields : false}
              blocks={
                onRow && at >= 0 && lay !== null && at < counts!.length
                  ? {
                      lay,
                      count: counts![at]!,
                      range: ranges[at]!,
                      share: gpuCountShare(counts![at]!, lay.count),
                    }
                  : null
              }
              onToggle={onToggle}
              onMove={onMove}
              onWeight={onWeight}
              onBlock={onBlock}
              onBlockCommit={onBlockCommit}
              onBlockBase={onBlockBase}
              onBlockRelease={onBlockRelease}
            />
          )
        })}
      </ul>
      {/* The move is announced by this line (GD1.9): the card's one status
      region, in the DOM and empty from the card's first render, whatever the
      read's state and the number of checked rows, so the first move finds a
      live region to speak through. Nothing else in the card is one: the
      detecting line and the read's outcome are plain paragraphs (GD1.3,
      GD1.7), and so is the share (GD2.9). */}
      <p className="visually-hidden" id="model-profile-gpu-moved" role="status">
        {moved}
      </p>
      {splitButtons ? (
        // Auto and Even, after the list and after the status region (GD1.4,
        // GD2.4): they follow every row in the tab order (GD1.12, GD2.9), and
        // the list stays a list of GPU rows alone.
        <div className="check-list-actions">
          <button
            className="secondary"
            type="button"
            aria-label={t('model-profile-gpu-auto')}
            onClick={onAuto}
          >
            {t('model-profile-gpu-auto')}
          </button>
          <button
            className="secondary"
            type="button"
            aria-label={t('model-profile-gpu-even')}
            onClick={onEven}
          >
            {t('model-profile-gpu-even')}
          </button>
        </div>
      ) : null}
      {deviceError ? (
        <p className="field-error" id="model-profile-device">
          {deviceError}
        </p>
      ) : null}
      {splitError ? (
        <p className="field-error" id="model-profile-tensor-split">
          {splitError}
        </p>
      ) : null}
    </fieldset>
  )
}

/** What a checked row shows in blocks mode (brief 19, 19 G.5): the layout's
 * places, the row's count of them, the range of blocks it takes, and the share
 * of that count. `null` is brief 14: the weight field. */
type GpuBlocks = {
  lay: Layout
  count: number
  range: string
  share: string
}

/** One row of the list: its checkbox and name, its memory line, its weight and
 * share, and its two Move buttons (GD1.5 to GD1.9, GD2.1). In blocks mode the
 * weight field is the row's block count, with a slider beside it (19 G.5). */
function GpuRowItem({
  row,
  probe,
  checked,
  at,
  count,
  weight,
  share,
  deviceError,
  splitError,
  splitErrorIds,
  weightsShown,
  blocks,
  onToggle,
  onMove,
  onWeight,
  onBlock,
  onBlockCommit,
  onBlockBase,
  onBlockRelease,
}: {
  row: GpuRow
  probe: GpuProbe | null | undefined
  checked: boolean
  at: number
  count: number
  weight: string
  share: string | undefined
  deviceError: string | undefined
  splitError: string | undefined
  splitErrorIds: string[] | undefined
  weightsShown: boolean
  blocks: GpuBlocks | null
  onToggle: (id: string) => void
  onMove: (id: string, dir: GpuDirection) => void
  onWeight: (id: string, value: string) => void
  onBlock: (id: string, raw: string) => void
  onBlockCommit: (id: string, raw: string) => void
  onBlockBase: () => void
  onBlockRelease: () => void
}) {
  const { id } = row
  const box = `model-profile-gpu-box-${id}`
  const memory = row.device === null ? null : memoryOf(row.device)
  const memoryId =
    memory === null ? undefined : `model-profile-gpu-memory-${id}`
  const shareId = `model-profile-gpu-share-${id}`
  const weightId = `model-profile-gpu-weight-${id}`
  const blocksId = `model-profile-gpu-blocks-${id}`
  const blocksLabelId = `model-profile-gpu-blocks-label-${id}`
  const sliderId = `model-profile-gpu-blocks-slider-${id}`
  const rangeId = `model-profile-gpu-blocks-text-${id}`
  const weightError = weightErrorOf(id, splitError, splitErrorIds)
  const describedBy = [
    ...(memoryId !== undefined ? [memoryId] : []),
    ...(deviceError && checked ? ['model-profile-device'] : []),
  ]
  // 19 G.7: what is typed, until Enter or blur applies it and the count comes
  // back. A row is no state the page keeps: the count is the weight's own.
  const [typed, setTyped] = useState<string | null>(null)
  // Leaving blocks mode (Auto, a mode that hides the weights, the row
  // unchecked) drops what is typed: the editor is gone, so a focus event that
  // reaches this row later finds no stale count to apply. Dropped as the row
  // renders, the same render that leaves no editor.
  if (blocks === null && typed !== null) setTyped(null)
  const commitBlocks = () => {
    if (typed === null || blocks === null) return
    setTyped(null)
    // 19 G.7: the count the row already shows is no edit: nothing is written,
    // so the ratio the file has survives and a save sends no `tensor-split`.
    const text = typed.trim()
    if (/^\d+$/.test(text) && Number(text) === blocks.count) return
    onBlockCommit(id, typed)
  }
  // The share line: in the weight's group where the group shows, and an item
  // of the row on its own where it does not (a mode that hides the weights
  // still gives a share to a checked row: GD2.2).
  const shareLine =
    share === undefined ? null : (
      <p className="muted" id={shareId}>
        {share}
      </p>
    )
  return (
    <li>
      <label className="check" htmlFor={box}>
        <input
          className="check-item"
          type="checkbox"
          id={box}
          data-gpu-box=""
          checked={checked}
          aria-invalid={checked && deviceError ? 'true' : undefined}
          aria-describedby={
            describedBy.length > 0 ? describedBy.join(' ') : undefined
          }
          onChange={() => onToggle(id)}
        />
        {/* The id in its own span (DESIGN.md 5.2 "GPU list": monospace, weight
            600); the text the label reads is the id, the name and the
            "(not detected)" of GD1.6, with the spaces outside the spans. */}
        <span className="gpu-id">{row.id}</span>
        {row.device?.name ? ` ${row.device.name}` : ''}
        {probe?.probeAnswered === true && row.device === null
          ? ` ${t('model-profile-gpu-not-detected')}`
          : ''}
      </label>
      {memory === null ? null : (
        <p className="muted" id={memoryId}>
          {memory}
        </p>
      )}
      {blocks !== null ? (
        // 19 G.5: the block count in place of the weight, a slider of the
        // layout's places beside it, and the range and the share in one plain
        // text item after them. The field answers to `tensor-split` as the
        // weight does (19 G.10), so it carries the same mark and the same
        // place in the focus order.
        <>
          <span className="weight-group">
            <label htmlFor={blocksId} id={blocksLabelId}>
              {t('model-profile-gpu-blocks')}{' '}
              <span className="visually-hidden">{id}</span>
            </label>
            <input
              type="text"
              className="gpu-weight"
              size={4}
              inputMode="numeric"
              id={blocksId}
              data-gpu-weight=""
              value={typed ?? `${blocks.count}`}
              aria-invalid={weightError ? 'true' : undefined}
              aria-describedby={[
                shareId,
                rangeId,
                ...(weightError ? ['model-profile-tensor-split'] : []),
              ].join(' ')}
              onChange={(event) => setTyped(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault()
                  commitBlocks()
                }
              }}
              onBlur={commitBlocks}
            />
          </span>
          <input
            className="blocks-slider"
            type="range"
            id={sliderId}
            aria-labelledby={blocksLabelId}
            min={0}
            max={blocks.lay.count}
            step={1}
            value={blocks.count}
            aria-valuetext={t(
              'model-profile-gpu-blocks-value',
              `${blocks.count}`,
              `${blocks.lay.count}`,
              blocks.range,
            )}
            onChange={(event) => {
              // A drag keeps the focus on the slider (19 G.6): the counts of
              // every row move under it, and the row it sits in is re-drawn.
              event.currentTarget.focus({ preventScroll: true })
              onBlock(id, event.target.value)
            }}
            onPointerDown={onBlockBase}
            onPointerUp={onBlockRelease}
            onPointerCancel={onBlockRelease}
          />
          <span className="muted" id={rangeId}>
            {blocks.range} <span id={shareId}>{blocks.share}</span>
          </span>
        </>
      ) : weightsShown ? (
        // The label, the field and the share in one span (DESIGN.md 5.2 "GPU
        // list"): the row's wrapping flex sees ONE item there, so it breaks
        // around the weight and never between its label and its field.
        <span className="weight-group">
          <label htmlFor={weightId}>
            {t('model-profile-gpu-weight')}{' '}
            <span className="visually-hidden">{id}</span>
          </label>
          <input
            type="text"
            className="gpu-weight"
            size={6}
            inputMode="decimal"
            id={weightId}
            data-gpu-weight=""
            value={weight}
            aria-invalid={weightError ? 'true' : undefined}
            aria-describedby={
              [
                ...(share === undefined ? [] : [shareId]),
                ...(weightError ? ['model-profile-tensor-split'] : []),
              ].join(' ') || undefined
            }
            onChange={(event) => onWeight(id, event.target.value)}
          />
          {shareLine}
        </span>
      ) : (
        shareLine
      )}
      {checked && count >= 2 ? (
        <>
          <MoveButton
            id={id}
            dir="up"
            label={t('model-profile-gpu-move-up')}
            at={at}
            onMove={onMove}
          />
          <MoveButton
            id={id}
            dir="down"
            label={t('model-profile-gpu-move-down')}
            at={at}
            last={count - 1}
            onMove={onMove}
          />
        </>
      ) : null}
    </li>
  )
}

/** The Move up (or Move down) of a checked row: an `aria-disabled` end is kept
 * in the tab order and answers nothing (GD1.9, D5.2). The ends are the first
 * checked row's Move up and the last checked row's Move down, whatever the
 * number of checked rows: with two of them, each row's other button moves the
 * neighbour, so the way back is never marked. */
function MoveButton({
  id,
  dir,
  label,
  at,
  last = 0,
  onMove,
}: {
  id: string
  dir: GpuDirection
  label: string
  at: number
  last?: number
  onMove: (id: string, dir: GpuDirection) => void
}) {
  const end = dir === 'up' ? at === 0 : at === last
  return (
    <button
      type="button"
      className="secondary"
      aria-label={`${label} ${id}`}
      aria-disabled={end ? 'true' : undefined}
      onClick={() => {
        if (!end) onMove(id, dir)
      }}
    >
      {label} <span className="visually-hidden">{id}</span>
    </button>
  )
}

/** Which weight fields a `tensor-split` error marks: the ones that failed the
 * check, and with a refusal that names none of them, every weight field (GD2.6,
 * GD2.7). */
function weightErrorOf(
  id: string,
  splitError: string | undefined,
  splitErrorIds: string[] | undefined,
): string | undefined {
  if (splitError === undefined) return undefined
  if (splitErrorIds === undefined) return splitError
  return gpuHas(splitErrorIds, id) ? splitError : undefined
}
