import type { EdgeView } from '../types'

/** An item the pinned steps stop at, and the chance the craft ends there. */
export interface ChainEnd {
  node: string
  p: number
}

/** How many times a method is used on average before the craft ends. */
export interface ChainUse {
  method: string
  name: string
  n: number
}

/** An item with more than one pinned method: the one the totals follow, and the others (method names). */
export interface ChainIgnored {
  node: string
  follows: string
  others: string[]
}

export type Chain =
  | { kind: 'none' }
  | { kind: 'endless' }
  | { kind: 'totals'; ends: ChainEnd[]; off: number; uses: ChainUse[]; approx: boolean; ignored: ChainIgnored[] }

/**
 * Following the pinned steps from `start` until they stop: where the craft ends, and how many times each method is used
 * on average. Each item follows the first method pinned from it. The items with a followed method are the transient
 * states of an absorbing Markov chain; the expected visits `x` solve `(I − Q)ᵀ x = e_start`.
 */
export function chainFrom(edges: EdgeView[], start: string): Chain {
  const follow = new Map<string, string>()
  for (const edge of edges) if (!follow.has(edge.from)) follow.set(edge.from, edge.method)
  if (!follow.has(start)) return { kind: 'none' }
  const followed = (edge: EdgeView) => follow.get(edge.from) === edge.method

  // The walk: every item the followed steps reach from `start`, the start first.
  const reached = [start]
  const seen = new Set(reached)
  for (let i = 0; i < reached.length; i++) {
    for (const edge of edges) {
      if (edge.from === reached[i] && followed(edge) && !seen.has(edge.to)) {
        seen.add(edge.to)
        reached.push(edge.to)
      }
    }
  }
  const transient = reached.filter((node) => follow.has(node))
  const ends = reached.filter((node) => !follow.has(node))
  const index = new Map(transient.map((node, i) => [node, i]))
  const n = transient.length

  const q = transient.map(() => new Array<number>(n).fill(0))
  const r = transient.map(() => new Map<string, number>())
  const out = new Array<number>(n).fill(0)
  let approx = false
  for (const edge of edges) {
    const i = index.get(edge.from)
    if (i === undefined || !followed(edge)) continue
    out[i] += edge.p
    if (edge.interval !== null) approx = true
    const j = index.get(edge.to)
    if (j !== undefined) q[i][j] += edge.p
    else r[i].set(edge.to, (r[i].get(edge.to) ?? 0) + edge.p)
  }

  // (I − Q)ᵀ x = e_start, by Gaussian elimination with partial pivoting.
  const a = transient.map((_, i) => transient.map((_, j) => (i === j ? 1 : 0) - q[j][i]))
  const b: number[] = transient.map((_, i) => (i === 0 ? 1 : 0))
  for (let col = 0; col < n; col++) {
    let pivot = col
    for (let row = col + 1; row < n; row++) if (Math.abs(a[row][col]) > Math.abs(a[pivot][col])) pivot = row
    if (Math.abs(a[pivot][col]) < 1e-12) return { kind: 'endless' }
    ;[a[col], a[pivot]] = [a[pivot], a[col]]
    ;[b[col], b[pivot]] = [b[pivot], b[col]]
    for (let row = col + 1; row < n; row++) {
      const factor = a[row][col] / a[col][col]
      if (factor === 0) continue
      for (let k = col; k < n; k++) a[row][k] -= factor * a[col][k]
      b[row] -= factor * b[col]
    }
  }
  const x = new Array<number>(n).fill(0)
  for (let row = n - 1; row >= 0; row--) {
    let sum = b[row]
    for (let k = row + 1; k < n; k++) sum -= a[row][k] * x[k]
    x[row] = Math.max(0, sum / a[row][row])
  }

  const endChance = new Map<string, number>(ends.map((node) => [node, 0]))
  let off = 0
  transient.forEach((_, i) => {
    for (const [node, p] of r[i]) endChance.set(node, (endChance.get(node) ?? 0) + x[i] * p)
    off += x[i] * Math.max(0, 1 - out[i])
  })

  // Uses per method, in the order each method is first followed among the edges.
  const uses: ChainUse[] = []
  for (const edge of edges) {
    if (!index.has(edge.from) || !followed(edge) || uses.some((u) => u.method === edge.method)) continue
    const visits = transient.reduce((sum, node, i) => (follow.get(node) === edge.method ? sum + x[i] : sum), 0)
    uses.push({ method: edge.method, name: edge.name, n: visits })
  }

  const ignored: ChainIgnored[] = []
  for (const node of transient) {
    const others: string[] = []
    for (const edge of edges) {
      if (edge.from !== node || followed(edge) || others.includes(edge.name)) continue
      others.push(edge.name)
    }
    // The first edge from an item is the step it follows.
    const first = edges.find((edge) => edge.from === node)!
    if (others.length > 0) ignored.push({ node, follows: first.name, others })
  }

  return {
    kind: 'totals',
    ends: [...endChance]
      .map(([node, p]) => ({ node, p: Math.max(0, p) }))
      .sort((left, right) => right.p - left.p || left.node.localeCompare(right.node)),
    off: Math.max(0, off),
    uses,
    approx,
    ignored,
  }
}
