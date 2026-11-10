import { describe, expect, it } from 'vitest'
import { applyMerges, autoOrder, basicsBlocker, foldShape, mergeBlocker, mergedInto, mergesOf, resolveId } from '../flow/merge'
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
const lock: SlotSpec = { id: 'StrMasterItemGenerationCannotChangePrefixes' }

function edge(from: string, to: string, p: number, interval: [number, number] | null = null): EdgeView {
  return { from, to, method: 'm', name: 'M', p, interval, repeatable: true }
}

const never = () => false
const always = () => true

describe('merging cards', () => {
  it('treats two cards of one base as the same item whatever their modifiers', () => {
    const a = card('a', { prefixes: [life], anyPrefixes: 1, anySuffixes: 2 })
    const b = card('b', { anyPrefixes: 2, anySuffixes: 3 })
    // The shape test only asks whether the engine could ever call them the same item.
    expect(basicsBlocker(a, b)).toBeNull()
    expect(basicsBlocker(card('m', { rarity: 'magic' }), b)).toBe('different rarity')
    expect(basicsBlocker(card('i', { itemLevel: 84 }), b)).toBe('different item level')
    expect(basicsBlocker(card('l', { base: 'Bow' }), b)).toBe('different base')
  })

  it('makes two destinations one card only when they are one item', () => {
    const hit = card('hit', { prefixes: [life], anyPrefixes: 1, anySuffixes: 2 })
    const empty = card('empty', { anyPrefixes: 2, anySuffixes: 2 })
    // A card may fold into one that names less — the store's tracked guard is what stops a goal
    // being dropped — but not into one that names a modifier it does not have.
    expect(mergeBlocker(hit, empty)).toBeNull()
    expect(mergeBlocker(empty, hit)).toBe('the target names a modifier this item lacks')
    expect(mergeBlocker(card('c1', { anyPrefixes: 1, anySuffixes: 3 }), empty)).toBe('different number of prefixes')
    expect(mergeBlocker(card('same', { anyPrefixes: 2, anySuffixes: 2 }), empty)).toBeNull()
  })

  it('reads a spec from the engine with its empty fields left out', () => {
    const wire = card('wire', {})
    wire.spec = { base: 'Helmet', itemLevel: 87, rarity: 'rare', anyPrefixes: 2, anySuffixes: 2 } as AbsSpec
    expect(mergeBlocker(wire, card('b', { anyPrefixes: 2, anySuffixes: 2 }))).toBeNull()
  })

  it('never drops a crafted metamod from two destinations', () => {
    const locked = card('locked', { suffixes: [lock], anyPrefixes: 2, anySuffixes: 2 })
    const plain = card('plain', { anyPrefixes: 2, anySuffixes: 3 })
    expect(mergeBlocker(locked, plain)).toBe('only one carries a crafted metamod')
    expect(mergeBlocker(locked, card('same', { suffixes: [lock], anyPrefixes: 2, anySuffixes: 2 }))).toBeNull()
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

  it('lets a destination take on a plan, and refuses to give one up', () => {
    const dest = card('dest', { anyPrefixes: 1, anySuffixes: 3 })
    const plan = card('plan', { anyPrefixes: 2, anySuffixes: 2 })
    // Different counts are no bar: the plan's odds are the evidence, and the store asks for them.
    expect(foldShape(dest, plan, (id) => id === 'plan')).toBeNull()
    expect(foldShape(plan, dest, (id) => id === 'plan')).toBe('one still has steps pinned from it')
    // With neither planned, the two are destinations and must be one item.
    expect(foldShape(dest, plan, never)).toBe('different number of prefixes')
    expect(foldShape(plan, card('same', { anyPrefixes: 2, anySuffixes: 2 }), never)).toBeNull()
    expect(foldShape(dest, plan, always)).toBeNull()
  })

  it('follows a chain of folds to the card at its end', () => {
    const nodes = [card('a', {}), card('b', {}), card('c', {})]
    const edges = [edge('a', 'b', 1), edge('b', 'c', 1)]
    const merges = [
      { from: 'a', into: 'b' },
      { from: 'b', into: 'c' },
    ]
    const shown = applyMerges(nodes, edges, merges)
    expect(shown.nodes.map((node) => node.id)).toEqual(['c'])
    expect(shown.edges.map((e) => [e.from, e.to])).toEqual([['c', 'c']])
    expect(mergedInto(nodes, merges, 'c').map((node) => node.id)).toEqual(['a', 'b'])
    expect(mergedInto(nodes, merges, 'b')).toEqual([])
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

  it('offers the cards a fold shares most with, and never drops a tracked modifier', () => {
    const tracked = new Set(['SimpleLife4'])
    const from = card('v', { prefixes: [life, veiled], anySuffixes: 2 })
    const hit = card('hit', { prefixes: [life], anyPrefixes: 1, anySuffixes: 2 })
    const sibling = card('sib', { prefixes: [life, veiled], anySuffixes: 2 })
    const miss = card('miss', { anyPrefixes: 3, anySuffixes: 2 })

    expect(autoOrder(from, [miss, hit, sibling], tracked).map((n) => n.id)).toEqual(['sib', 'hit'])
    expect(autoOrder(from, [miss], tracked)).toEqual([])
  })

  it('reads saved merges', () => {
    expect(mergesOf([{ from: 'a', into: 'b' }, { from: 1, into: 'b' }, { from: 'a' }, null])).toEqual([
      { from: 'a', into: 'b' },
    ])
    expect(mergesOf('nope')).toEqual([])
  })
})
