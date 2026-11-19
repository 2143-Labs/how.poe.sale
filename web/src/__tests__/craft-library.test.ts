import { describe, expect, it } from 'vitest'
import { exportJson, importJson, outputs, remove, type Library } from '../craft/library'
import type { CraftSpec, GoalSpec, MethodSpec, PriceSpec } from '../craft/types'

const goal: GoalSpec = {
  id: 'user:goal:one',
  name: 'One',
  base: 'shield',
  influence: 'none',
  prefixes: [],
  suffixes: [],
  desired: [],
  good: [],
}

const prices: PriceSpec = { id: 'user:prices:one', name: 'Prices', unit: 'chaos', weights: {} }

/** A method spec with only the kind set, which is how a block that needs no parameters reads. */
function method(kind: string): MethodSpec {
  return { kind, modifiers: null, augment: null, essence: null }
}

const inner: CraftSpec = {
  id: 'user:craft:inner',
  name: 'Inner',
  goal: goal.id,
  prices: prices.id,
  start: 'normal',
  entry: 'b1',
  blocks: [{ id: 'b1', method: method('alt_to_mod'), craft: null, position: null }],
  wires: [{ from: 'b1', exit: 'magic', to: 'done' }],
}

const outer: CraftSpec = {
  id: 'user:craft:outer',
  name: 'Outer',
  goal: goal.id,
  prices: prices.id,
  start: 'normal',
  entry: 'b1',
  blocks: [{ id: 'b1', method: null, craft: inner.id, position: null }],
  wires: [{ from: 'b1', exit: 'magic', to: 'done' }],
}

const library: Library = { version: 1, goals: [goal], prices: [prices], crafts: [inner, outer] }
const empty: Library = { version: 1, goals: [], prices: [], crafts: [] }

/** A craft that leaves through the exit `rare` twice, on two different blocks. */
const twoWays: CraftSpec = {
  id: 'user:craft:two',
  name: 'Two ways',
  goal: goal.id,
  prices: prices.id,
  start: 'normal',
  entry: 'b1',
  blocks: [
    { id: 'b1', method: method('regal_to'), craft: null, position: null },
    { id: 'b2', method: method('desired_pair'), craft: null, position: null },
  ],
  wires: [
    { from: 'b1', exit: 'rare', to: 'done' },
    { from: 'b1', exit: 'normal', to: 'b2' },
    { from: 'b2', exit: 'rare', to: 'done' },
  ],
}

describe('the preset library', () => {
  it('round-trips through export and import', () => {
    expect(importJson(exportJson(library), empty)).toEqual(library)
  })

  it('renames a clashing id and rewrites every reference to it', () => {
    const existing: Library = {
      version: 1,
      goals: [{ ...goal, name: 'Existing goal' }],
      prices: [{ ...prices, name: 'Existing prices' }],
      crafts: [{ ...inner, name: 'Existing inner' }],
    }
    const merged = importJson(exportJson(library), existing)
    expect(merged.goals).toHaveLength(2)
    const importedGoal = merged.goals.find((entry) => entry.name === 'One')
    const importedPrices = merged.prices.find((entry) => entry.name === 'Prices')
    const importedInner = merged.crafts.find((entry) => entry.name === 'Inner')
    const importedOuter = merged.crafts.find((entry) => entry.name === 'Outer')
    expect(importedGoal?.id).toBeDefined()
    expect(importedPrices?.id).toBeDefined()
    expect(importedInner?.id).toBeDefined()
    expect(importedOuter?.id).toBeDefined()
    expect(importedGoal!.id).not.toBe(goal.id)
    expect(importedPrices!.id).not.toBe(prices.id)
    expect(importedInner!.id).not.toBe(inner.id)
    expect(importedInner!.goal).toBe(importedGoal!.id)
    expect(importedInner!.prices).toBe(importedPrices!.id)
    expect(importedOuter!.goal).toBe(importedGoal!.id)
    expect(importedOuter!.prices).toBe(importedPrices!.id)
    expect(importedOuter!.blocks[0].craft).toBe(importedInner!.id)
  })

  it('refuses to remove a craft another craft uses as a block', () => {
    const result = remove(library, 'craft', inner.id)
    expect('error' in result && result.error).toBe('used by "Outer"')
  })

  it('names each way out, suffixing a repeated exit', () => {
    expect(outputs(twoWays, library)).toEqual(['rare', 'rare#2'])
  })

  it('never repeats an output name, even when a nested exit is already suffixed', () => {
    const nested: CraftSpec = {
      ...twoWays,
      id: 'user:craft:nested',
      entry: 'y1',
      blocks: [
        { id: 'y1', method: method('regal_to'), craft: null, position: null },
        { id: 'y2', method: null, craft: twoWays.id, position: null },
      ],
      wires: [
        { from: 'y1', exit: 'rare', to: 'done' },
        { from: 'y1', exit: 'normal', to: 'y2' },
        { from: 'y2', exit: 'rare', to: 'done' },
        { from: 'y2', exit: 'rare#2', to: 'done' },
      ],
    }
    expect(outputs(nested, library)).toEqual(['rare', 'rare#2', 'rare#2#2'])
  })
})
