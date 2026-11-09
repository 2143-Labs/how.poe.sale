import type { EdgeView } from '../types'

/** What a pinned path between two nodes adds up to. */
export interface PathChance {
  /** The chance the path happens: the product of its steps. */
  p: number
  steps: number
  /** Whether any step's chance came from a simulation. */
  approx: boolean
}

type Neighbour = { edge: EdgeView; to: string }
type Arrival = { edge: EdgeView; came: string }

/**
 * The chance of the pinned path between `a` and `b`: the product of its steps' chances, when
 * every step runs the same way (all crafting forward or all crafting back). A path that mixes
 * directions is not one sequence of methods, so it has no single chance and `null` is returned.
 * Ties between several paths are settled by the order the steps were pinned.
 */
export function pathChance(edges: EdgeView[], a: string, b: string): PathChance | null {
  if (a === b) return null
  const around = new Map<string, Neighbour[]>()
  const link = (from: string, to: string, edge: EdgeView) => {
    const list = around.get(from) ?? []
    list.push({ edge, to })
    around.set(from, list)
  }
  for (const edge of edges) {
    link(edge.from, edge.to, edge)
    link(edge.to, edge.from, edge)
  }

  const arrives = new Map<string, Arrival>()
  const seen = new Set<string>([a])
  const queue = [a]
  while (queue.length > 0) {
    const here = queue.shift() as string
    if (here === b) break
    for (const { edge, to } of around.get(here) ?? []) {
      if (seen.has(to)) continue
      seen.add(to)
      arrives.set(to, { edge, came: here })
      queue.push(to)
    }
  }

  const steps: EdgeView[] = []
  let forward = 0
  let here = b
  while (here !== a) {
    const arrival = arrives.get(here)
    if (!arrival) return null
    steps.push(arrival.edge)
    // The step runs forward along the walk when it leaves the node we came from.
    if (arrival.edge.from === arrival.came) forward += 1
    here = arrival.came
  }
  if (forward !== 0 && forward !== steps.length) return null
  let p = 1
  for (const edge of steps) p *= edge.p
  return { p, steps: steps.length, approx: steps.some((edge) => edge.interval !== null) }
}
