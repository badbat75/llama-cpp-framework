// The block arithmetic of the GPU card (fe/briefs/19-layer-sliders.md, 19 G.3):
// the mirror of `crates/app/src/gpu_split.rs`'s `Layout`, `block_counts`,
// `block_ranges` and `set_blocks`, kept pure so every rule is a unit test. The
// vectors of `gpuBlocks.test.ts` are that module's own, so the page cuts the
// model exactly where the configurator's table does.
//
// The card shows block COUNTS while the key holds a weight RATIO: the counts
// are the projection of the ratio over the layout's positions, and an edit
// writes the counts back as the weights, exactly as the Rust side does.
import { t } from './strings.ts'

/** A block layout: the positions the GPUs share, out of the whole model. The
 *  last one is the output layer, a position that is not a block. */
export type Layout = { start: number; count: number }

/** The layout of `blocks` transformer blocks with `ngl` layers on the GPU (a
 *  negative `ngl` is all of them, llama.cpp's own default). `null` when there
 *  is nothing to draw: no model read, or an offload of zero layers. Mirrors
 *  `Layout::new`. */
export function layout(blocks: number, ngl: number): Layout | null {
  if (blocks <= 0) return null
  const positions = blocks + 1
  const layers = ngl < 0 ? positions : Math.min(ngl, positions)
  if (layers <= 0) return null
  return { start: positions - layers, count: layers }
}

/** `text` as an integer scaled by `scale`, keeping at most `digits` decimals
 *  and dropping the digits after them, as the Rust side parses a weight. An
 *  empty text is 0, the way llama.cpp pads a short vector, and a weight below
 *  0 is clamped there, as `block_counts` clamps it. */
function decimal(text: string, digits: number, scale: bigint): bigint {
  const source = text.trim()
  const negative = source.startsWith('-')
  const rest = negative || source.startsWith('+') ? source.slice(1) : source
  const dot = rest.indexOf('.')
  const head = dot < 0 ? rest : rest.slice(0, dot)
  const tail = dot < 0 ? '' : rest.slice(dot + 1, dot + 1 + digits)
  let whole = 0n
  for (const c of head) {
    if (c < '0' || c > '9') break
    whole = whole * 10n + BigInt(c.charCodeAt(0) - 48)
  }
  let part = 0n
  for (const c of tail) {
    if (c < '0' || c > '9') break
    part = part * 10n + BigInt(c.charCodeAt(0) - 48)
  }
  for (let i = tail.length; i < digits; i += 1) part *= 10n
  const value = whole * scale + part
  return negative && value > 0n ? 0n : value
}

/** The weights as integers, every one scaled by the largest power of ten that
 *  makes them all whole, so a decimal vector projects exactly (`0.6,0.4` and
 *  `3,2` are the same launch), with no rounding anywhere. */
function whole(weights: string[]): bigint[] {
  let digits = 0
  for (const w of weights) {
    const text = w.trim()
    const dot = text.indexOf('.')
    if (dot >= 0) digits = Math.max(digits, text.length - dot - 1)
  }
  const scale = 10n ** BigInt(digits)
  return weights.map((w) => decimal(w, digits, scale))
}

/** How many positions each row gets, the projection of the ratio: position
 *  `pos` goes to the first row whose cumulative share passes it, compared in
 *  integers as `cum * count > pos * total`. `null` for an empty vector, an
 *  all-zero one (Auto: llama.cpp splits by free memory at load) or a layout
 *  with no position. Mirrors `block_counts`. */
export function blockCounts(weights: string[], lay: Layout): number[] | null {
  if (weights.length === 0 || lay.count <= 0) return null
  const values = whole(weights)
  const cum: bigint[] = []
  let total = 0n
  for (const value of values) {
    total += value
    cum.push(total)
  }
  if (total <= 0n) return null
  const count = BigInt(lay.count)
  const out = weights.map(() => 0)
  for (let pos = 0; pos < lay.count; pos += 1) {
    const scaled = BigInt(pos) * total
    let row = cum.findIndex((c) => c * count > scaled)
    if (row < 0) row = weights.length - 1
    out[row] += 1
  }
  return out
}

/** The range label of each row, from its count and the layout: `blocks 0-28`,
 *  `blocks 29-32 + output`, `output layer`, `nothing`. The output layer is
 *  named by itself because it is a position and not a block. Mirrors
 *  `block_ranges`. */
export function blockRanges(counts: number[], lay: Layout): string[] {
  const output = lay.start + lay.count - 1
  const out: string[] = []
  let pos = lay.start
  for (const c of counts) {
    const first = pos
    const last = pos + c - 1
    pos += c
    if (c <= 0) out.push(t('model-profile-gpu-blocks-nothing'))
    else if (last >= output && c === 1)
      out.push(t('model-profile-gpu-output-layer'))
    else if (last >= output && c === 2)
      out.push(t('model-profile-gpu-block-plus-output', `${first}`))
    else if (last >= output)
      out.push(
        t('model-profile-gpu-blocks-plus-output', `${first}`, `${last - 1}`),
      )
    else if (c === 1) out.push(t('model-profile-gpu-block-one', `${first}`))
    else out.push(t('model-profile-gpu-blocks-range', `${first}`, `${last}`))
  }
  return out
}

/** `count` positions over `n` rows, the remainder to the back rows: what an
 *  all-zero vector is seeded with before an edit is applied. Mirrors
 *  `even_counts`. */
function evenCounts(n: number, count: number): number[] {
  if (n === 0) return []
  const base = Math.floor(count / n)
  const rem = count - base * n
  return Array.from({ length: n }, (_, i) => base + (i >= n - rem ? 1 : 0))
}

/** The weights with the `index`-th row's blocks set to `blocks`: the counts
 *  are the starting point (an all-zero vector seeds an even cut), the row takes
 *  what it asked for, and the difference is absorbed from the back, the last
 *  row that is not the edited one first, walking towards the head and taking
 *  from each row only what it has. The counts are the new weights: a vector
 *  summing to the positions assigns exactly itself. Mirrors `set_blocks`. */
export function setBlocks(
  weights: string[],
  index: number,
  blocks: number,
  lay: Layout,
): string[] {
  if (index < 0 || index >= weights.length) return [...weights]
  const n = weights.length
  const counts = blockCounts(weights, lay) ?? evenCounts(n, lay.count)
  const want = Math.min(Math.max(blocks, 0), lay.count)
  let delta = want - counts[index]!
  counts[index] = want
  for (let j = n - 1; j >= 0; j -= 1) {
    if (j === index || delta === 0) continue
    const has = counts[j]!
    if (delta > 0) {
      const take = Math.min(delta, has)
      counts[j] = has - take
      delta -= take
    } else {
      counts[j] = has - delta
      delta = 0
    }
  }
  // Every other row was already at zero: the edited row keeps only what was
  // actually free.
  counts[index] = counts[index]! - delta
  return counts.map((c) => `${c}`)
}
