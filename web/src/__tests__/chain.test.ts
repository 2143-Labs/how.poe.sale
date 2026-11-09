import { describe, expect, it } from 'vitest'
import type { EdgeView } from '../types'
import { chainFrom } from '../flow/chain'

function edge(from: string, to: string, method: string, p: number): EdgeView {
  return { from, to, method, name: method.toUpperCase(), p, interval: null, repeatable: true }
}

describe('chainFrom', () => {
  it('totals the Alteration loop with an Augmentation on a suffix-only item', () => {
    // s = 1/30: the wanted prefix's share of the helmet prefixes.
    const alteration = (from: string) => [
      edge(from, 'XA', 'alt', 1 / 120),
      edge(from, 'XS', 'alt', 1 / 60),
      edge(from, 'O', 'alt', 29 / 120),
      edge(from, 'R', 'alt', 29 / 60),
      edge(from, 'S', 'alt', 1 / 4),
    ]
    const edges = [...alteration('R'), ...alteration('O'), edge('S', 'XS', 'aug', 1 / 30), edge('S', 'R', 'aug', 29 / 30)]
    const chain = chainFrom(edges, 'R')
    if (chain.kind !== 'totals') throw new Error(chain.kind)
    expect(chain.uses.map((u) => u.method)).toEqual(['alt', 'aug'])
    expect(chain.uses[0].n).toBeCloseTo(30, 9)
    expect(chain.uses[1].n).toBeCloseTo(7.5, 9)
    expect(chain.ends.map((e) => e.node)).toEqual(['XS', 'XA'])
    expect(chain.ends[0].p).toBeCloseTo(0.75, 9)
    expect(chain.ends[1].p).toBeCloseTo(0.25, 9)
    expect(chain.off).toBeCloseTo(0, 9)
    expect(chain.approx).toBe(false)
    expect(chain.ignored).toEqual([])
  })

  it('counts what leaves the pinned steps', () => {
    const chain = chainFrom([edge('R', 'A', 'm', 0.4)], 'R')
    if (chain.kind !== 'totals') throw new Error(chain.kind)
    expect(chain.ends).toEqual([{ node: 'A', p: 0.4 }])
    expect(chain.off).toBeCloseTo(0.6, 12)
    expect(chain.uses).toEqual([{ method: 'm', name: 'M', n: 1 }])
  })

  it('finds a loop with no way out', () => {
    expect(chainFrom([edge('R', 'R', 'm', 1)], 'R')).toEqual({ kind: 'endless' })
  })

  it('has nothing to total without a pinned step', () => {
    expect(chainFrom([edge('A', 'B', 'm', 1)], 'R')).toEqual({ kind: 'none' })
  })

  it('follows the first pinned method and names the others', () => {
    const chain = chainFrom([edge('R', 'A', 'm', 1), edge('R', 'B', 'k', 1)], 'R')
    if (chain.kind !== 'totals') throw new Error(chain.kind)
    expect(chain.uses.map((u) => u.method)).toEqual(['m'])
    expect(chain.ends).toEqual([{ node: 'A', p: 1 }])
    expect(chain.ignored).toEqual([{ node: 'R', follows: 'M', others: ['K'] }])
  })
})
