import { describe, expect, it } from 'vitest'
import type { ClassView, EdgeView, NodeView } from '../types'
import { itemsFrom, percent } from '../flow/format'

const view: ClassView = {
  base: 'Helmet',
  itemLevel: 87,
  rarity: 'magic',
  corrupted: false,
  influences: [],
  prefixes: [],
  suffixes: [],
  anyPrefixes: 1,
  anySuffixes: 1,
}

function node(id: string, source: NodeView['source'] = null): NodeView {
  return {
    id,
    spec: {
      base: 'Helmet',
      itemLevel: 87,
      rarity: 'magic',
      influences: [],
      prefixes: [],
      suffixes: [],
      anyPrefixes: 1,
      anySuffixes: 1,
      corrupted: false,
    },
    view,
    members: 1,
    approx: false,
    source,
  }
}

function edge(from: string, to: string, method: string, name: string): EdgeView {
  return { from, to, method, name, p: 0.5, interval: null, repeatable: true }
}

describe('percent', () => {
  it('keeps three figures of a chance below 0.01%', () => {
    expect(percent(3.8416 / 200003.8416)).toBe('0.00192%')
  })

  it('shows two decimals otherwise', () => {
    expect(percent(29 / 60)).toBe('48.33%')
    expect(percent(0)).toBe('0.00%')
  })
})

describe('itemsFrom', () => {
  const nodes = [node('a'), node('b'), node('x', { from: 'a', method: 'currency/alteration' })]

  it('says nothing for a node one step reached', () => {
    const edges = [edge('a', 'x', 'currency/alteration', 'Orb of Alteration')]
    expect(itemsFrom(nodes[2], nodes, edges)).toBeNull()
  })

  it('names the step that gave a merged node its items', () => {
    const edges = [
      edge('a', 'x', 'currency/alteration', 'Orb of Alteration'),
      edge('b', 'x', 'currency/augmentation', 'Orb of Augmentation'),
    ]
    expect(itemsFrom(nodes[2], nodes, edges)).toBe('items from: Orb of Alteration on magic · any prefix ×1 · any suffix ×1')
  })

  it('says an earlier step when the source node is gone', () => {
    const edges = [
      edge('b', 'x', 'currency/augmentation', 'Orb of Augmentation'),
      edge('x', 'x', 'currency/alteration', 'Orb of Alteration'),
    ]
    const left = [nodes[1], nodes[2]]
    expect(itemsFrom(nodes[2], left, edges)).toBe('items from: an earlier currency/alteration step')
  })
})
