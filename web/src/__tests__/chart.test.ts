import { describe, expect, it } from 'vitest'
import { SCOPES } from '../flow/scope'
import { createFlowStore, seedSpec } from '../flow/store'
import { nodeId, type AbsSpec, type AbsView, type PreimageView, type PreimagesView } from '../types'

const crown: AbsSpec = {
  base: 'Archdemon Crown',
  itemLevel: 87,
  rarity: 'rare',
  influences: ['shaper', 'elder'],
  prefixes: [{ id: 'IncreasedLife9' }, { id: 'IncreasedMana12' }],
  suffixes: [{ id: 'FireResist8' }, { id: 'ColdResist8' }],
  anyPrefixes: 1,
  anySuffixes: 1,
}

function view(spec: AbsSpec): AbsView {
  return {
    base: spec.base,
    itemLevel: spec.itemLevel,
    rarity: spec.rarity,
    influences: spec.influences,
    prefixes: spec.prefixes.map((m) => m.id),
    suffixes: spec.suffixes.map((m) => m.id),
    anyPrefixes: spec.anyPrefixes,
    anySuffixes: spec.anySuffixes,
  }
}

function store() {
  return createFlowStore(SCOPES.poe1, { hash: '', storage: null, dropHash: () => undefined })
}

describe('node ids', () => {
  it('do not depend on modifier or influence order', () => {
    const shuffled: AbsSpec = {
      ...crown,
      influences: ['elder', 'shaper'],
      prefixes: [{ id: 'IncreasedMana12' }, { id: 'IncreasedLife9', values: [140] }],
      suffixes: [{ id: 'ColdResist8' }, { id: 'FireResist8' }],
    }
    expect(nodeId(shuffled)).toBe(nodeId(crown))
  })

  it('change with every field', () => {
    const changed: AbsSpec[] = [
      { ...crown, base: 'Hubris Circlet' },
      { ...crown, itemLevel: 86 },
      { ...crown, rarity: 'magic' },
      { ...crown, influences: ['shaper'] },
      { ...crown, prefixes: [{ id: 'IncreasedLife9' }] },
      { ...crown, suffixes: [{ id: 'FireResist8' }] },
      { ...crown, anyPrefixes: 2 },
      { ...crown, anySuffixes: 0 },
    ]
    for (const spec of changed) expect(nodeId(spec)).not.toBe(nodeId(crown))
  })
})

describe('the chart', () => {
  it('pins the same input once', () => {
    const flow = store()
    const target = seedSpec(SCOPES.simple)
    const accepted: PreimagesView = { target: view(target), steps: [], skipped: [] }
    flow.getState().accept(target, accepted)
    const root = flow.getState().chart[0].id

    const input: AbsSpec = { ...target, rarity: 'normal', suffixes: [], anyPrefixes: 0, anySuffixes: 0 }
    // The engine leaves empty lists and zero counts out of a spec it writes.
    const step: PreimageView = {
      method: 'essence/essence-of-horror',
      name: 'Essence of Horror',
      p: 11 / 64,
      cost: '1 × Essence of Horror',
      input: view(input),
      inputSpec: { base: input.base, itemLevel: input.itemLevel, rarity: input.rarity },
      note: null,
    }
    flow.getState().pin(root, step)
    flow.getState().select(root)
    flow.getState().pin(root, step)

    const { chart, edges, selected } = flow.getState()
    expect(chart.map((node) => node.id)).toEqual([root, nodeId(input)])
    expect(edges).toEqual([{ from: nodeId(input), to: root, method: step.method, name: step.name, p: step.p }])
    expect(selected).toBe(nodeId(input))

    flow.getState().unpin(nodeId(input))
    expect(flow.getState().chart.map((node) => node.id)).toEqual([root])
    expect(flow.getState().edges).toEqual([])
    expect(flow.getState().selected).toBe(root)
  })
})
