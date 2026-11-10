import { describe, expect, it } from 'vitest'
import { applyMerges, autoTarget, mergeBlocker, mergedInto, mergesOf, resolveId } from '../flow/merge'
import type { AbsSpec, EdgeView, NodeView, SlotSpec } from '../types'

const base: AbsSpec = {
  base: 'Helmet',
  itemLevel: 87,
  rarity: 'rare',
  influences: [],
  prefixes: [],
  suffixes: [],
  anyPrefixes: 0,
  anySuffixes: 0,
  corrupted: false,
}

function card(id: string, spec: Partial<AbsSpec>): NodeView {
  const full = { ...base, ...spec }
  return {
    id,
    spec: full,
    view: {
      base: full.base,
      itemLevel: full.itemLevel,
      rarity: full.rarity,
      corrupted: full.corrupted,
      influences: full.influences,
      prefixes: full.prefixes.map((slot) => slot.id),
      suffixes: full.suffixes.map((slot) => slot.id),
      anyPrefixes: full.anyPrefixes,
      anySuffixes: full.anySuffixes,
    },
    members: null,
    approx: false,
    source: null,
  }
}

const life: SlotSpec = { id: 'SimpleLife4' }
const veiled: SlotSpec = { id: 'VeiledPrefix' }

function edge(from: string, to: string, p: number, interval: [number, number] | null = null): EdgeView {
  return { from, to, method: 'm', name: 'M', p, interval, repeatable: true }
}

describe('merging cards', () => {
  it('lets a card merge into one that names less', () => {
    const hit = card('hit', { prefixes: [life], anyPrefixes: 1, anySuffixes: 2 })
    const empty = card('empty', { anyPrefixes: 2, anySuffixes: 2 })
    expect(mergeBlocker(hit, empty)).toBeNull()
    expect(mergeBlocker(empty, hit)).toBe('the target names a modifier this item lacks')
  })

  it('says why two cards cannot merge', () => {
    const a = card('a', { anyPrefixes: 1, anySuffixes: 3 })
    const b = card('b', { anyPrefixes: 2, anySuffixes: 2 })
    expect(mergeBlocker(a, b)).toBe('different number of prefixes')
    expect(mergeBlocker(card('m', { rarity: 'magic', anyPrefixes: 2, anySuffixes: 2 }), b)).toBe('different rarity')
  })

  it('reads a spec from the engine with its empty fields left out', () => {
    const wire = card('wire', {})
    wire.spec = { base: 'Helmet', itemLevel: 87, rarity: 'rare', anyPrefixes: 2, anySuffixes: 2 } as AbsSpec
    expect(mergeBlocker(wire, card('b', { anyPrefixes: 2, anySuffixes: 2 }))).toBeNull()
  })

  it('follows a chain of merges and stops on a cycle', () => {
    expect(resolveId([{ from: 'a', into: 'b' }, { from: 'b', into: 'c' }], 'a')).toBe('c')
    expect(['a', 'b']).toContain(resolveId([{ from: 'a', into: 'b' }, { from: 'b', into: 'a' }], 'a'))
  })

  it('adds the outcomes that land on one card', () => {
    const nodes = [card('R', {}), card('A', {}), card('B', {})]
    const shown = applyMerges(nodes, [edge('R', 'A', 0.2), edge('R', 'B', 0.3)], [{ from: 'B', into: 'A' }])
    expect(shown.nodes.map((n) => n.id)).toEqual(['R', 'A'])
    expect(shown.edges).toHaveLength(1)
    expect(shown.edges[0]).toMatchObject({ from: 'R', to: 'A' })
    expect(shown.edges[0].p).toBeCloseTo(0.5)
  })

  it('counts the same step from merged cards once', () => {
    const nodes = [card('X', {}), card('Y', {}), card('Z', {})]
    const shown = applyMerges(nodes, [edge('X', 'Z', 0.2), edge('Y', 'Z', 0.4)], [{ from: 'X', into: 'Y' }])
    expect(shown.edges).toEqual([edge('Y', 'Z', 0.4)])
  })

  it('turns a step into a merged card into a loop', () => {
    const nodes = [card('X', {}), card('Y', {})]
    const shown = applyMerges(nodes, [edge('Y', 'X', 0.3)], [{ from: 'X', into: 'Y' }])
    expect(shown.edges).toMatchObject([{ from: 'Y', to: 'Y', p: 0.3 }])
  })

  it('adds the ranges, reading a missing one as its chance', () => {
    const nodes = [card('R', {}), card('A', {}), card('B', {})]
    const shown = applyMerges(
      nodes,
      [edge('R', 'A', 0.15, [0.1, 0.2]), edge('R', 'B', 0.3)],
      [{ from: 'B', into: 'A' }],
    )
    expect(shown.edges[0].interval?.[0]).toBeCloseTo(0.4)
    expect(shown.edges[0].interval?.[1]).toBeCloseTo(0.5)
  })

  it('never merges away a tracked modifier on its own', () => {
    const tracked = new Set(['SimpleLife4'])
    const from = card('v', { prefixes: [life, veiled], anySuffixes: 2 })
    const hit = card('hit', { prefixes: [life], anyPrefixes: 1, anySuffixes: 2 })
    const miss = card('miss', { anyPrefixes: 2, anySuffixes: 2 })
    expect(autoTarget(from, [miss, hit], tracked)?.id).toBe('hit')
    expect(autoTarget(from, [miss], tracked)).toBeNull()
  })

  it('reads saved merges', () => {
    expect(mergesOf([{ from: 'a', into: 'b' }, { from: 1, into: 'b' }, { from: 'a' }, null])).toEqual([
      { from: 'a', into: 'b' },
    ])
    expect(mergesOf('nope')).toEqual([])
  })

  it('never drops a crafted metamod', () => {
    const lock: SlotSpec = { id: 'StrMasterItemGenerationCannotChangePrefixes' }
    const locked = card('locked', { suffixes: [lock], anyPrefixes: 2, anySuffixes: 2 })
    const plain = card('plain', { anyPrefixes: 2, anySuffixes: 3 })
    // Same counts, and the target names nothing: without the guard this fold looked allowed.
    expect(mergeBlocker(locked, plain)).toBe('only one carries a crafted metamod')
    // A card that keeps the lock is still a legitimate target.
    const same = card('same', { suffixes: [lock], anyPrefixes: 2, anySuffixes: 2 })
    expect(mergeBlocker(locked, same)).toBeNull()
  })

  it('reads influences without caring for their order', () => {
    const a = card('a', { influences: ['shaper', 'elder'], anyPrefixes: 2, anySuffixes: 2 })
    const b = card('b', { influences: ['elder', 'shaper'], anyPrefixes: 2, anySuffixes: 2 })
    const c = card('c', { influences: ['shaper'], anyPrefixes: 2, anySuffixes: 2 })
    expect(mergeBlocker(a, b)).toBeNull()
    expect(mergeBlocker(a, c)).toBe('different influences')
  })

  it('will not fold a family slot into an exact one', () => {
    const family = card('family', { prefixes: [{ id: 'SimpleLife4', orBetter: true }], anyPrefixes: 1, anySuffixes: 2 })
    const exact = card('exact', { prefixes: [life], anyPrefixes: 1, anySuffixes: 2 })
    expect(mergeBlocker(exact, family)).toBe('the target names a modifier this item lacks')
    expect(mergeBlocker(family, exact)).toBe('the target names a modifier this item lacks')
    expect(mergeBlocker(family, card('two', { anyPrefixes: 2, anySuffixes: 2 }))).toBeNull()
  })

  it('follows a chain of folds to the card at its end', () => {
    const nodes = [card('R', {}), card('A', {}), card('B', {}), card('C', {})]
    const edges = [edge('R', 'A', 1), edge('A', 'B', 1), edge('B', 'C', 1)]
    const merges = [
      { from: 'A', into: 'B' },
      { from: 'B', into: 'C' },
    ]
    const shown = applyMerges(nodes, edges, merges)
    expect(shown.nodes.map((node) => node.id)).toEqual(['R', 'C'])
    // The step from R survives, and the steps between the folded cards become a loop on C.
    expect(shown.edges.map((e) => [e.from, e.to])).toEqual([
      ['R', 'C'],
      ['C', 'C'],
    ])
    expect(mergedInto(nodes, merges, 'C').map((node) => node.id)).toEqual(['A', 'B'])
    // Nothing resolves to `B`: it is a link in the chain, not the end of one.
    expect(mergedInto(nodes, merges, 'B')).toEqual([])
  })
})
