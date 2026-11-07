import { describe, expect, it } from 'vitest'
import type { EdgeView } from '../types'
import { pathChance } from '../flow/path'

function edge(from: string, to: string, p: number, simulated = false): EdgeView {
  return {
    from,
    to,
    method: `currency/${from}${to}`,
    name: 'a step',
    p,
    interval: simulated ? [p - 0.01, p + 0.01] : null,
  }
}

describe('pathChance', () => {
  it('multiplies the steps of a forward path', () => {
    const edges = [edge('a', 'b', 0.5), edge('b', 'c', 0.2)]
    expect(pathChance(edges, 'a', 'c')).toEqual({ p: 0.1, steps: 2, approx: false })
  })

  it('walks a backward path in the same way', () => {
    // Pinned backward: the input is `from`, the item it produced is `to`.
    const edges = [edge('input', 'target', 0.25)]
    expect(pathChance(edges, 'input', 'target')).toEqual({ p: 0.25, steps: 1, approx: false })
  })

  it('has no chance for a path that mixes directions', () => {
    const edges = [edge('a', 'b', 0.5), edge('c', 'b', 0.5)]
    expect(pathChance(edges, 'a', 'c')).toBeNull()
  })

  it('has no chance for a node against itself', () => {
    expect(pathChance([edge('a', 'b', 0.5)], 'a', 'a')).toBeNull()
  })

  it('has no chance for two nodes that are not joined', () => {
    expect(pathChance([edge('a', 'b', 0.5)], 'a', 'c')).toBeNull()
  })

  it('marks a path with a simulated step', () => {
    expect(pathChance([edge('a', 'b', 0.5, true)]  , 'a', 'b')?.approx).toBe(true)
  })
})
