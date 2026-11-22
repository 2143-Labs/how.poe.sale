import { describe, expect, it } from 'vitest'
import { exportJson, importJson, load, outputs, remove, STORAGE_KEY, type Library } from '../craft/library'
import type { BaseView, Builtins, CraftSpec, FilterSpec, MethodSpec, PriceSpec } from '../craft/types'

const filter: FilterSpec = {
  id: 'user:filter:one',
  name: 'One',
  base: 'shield',
  prefixes: [],
  suffixes: [],
  desired: [],
  good: [],
}

const builtinFilter: FilterSpec = {
  id: 'builtin:goal:shield',
  name: 'Shield: the file goal',
  base: 'shield',
  prefixes: [],
  suffixes: [],
  desired: [],
  good: [],
}

const shield: BaseView = {
  key: 'shield',
  label: 'Supreme Spiked Shield',
  itemLevel: 87,
  itemClass: 'Shield',
  influences: ['Shaper'],
  caps: { magic: 1, rare: 3 },
  goal: builtinFilter,
}

const bases: BaseView[] = [shield]
const builtins: Builtins = { filters: [builtinFilter], prices: [], crafts: [] }

const prices: PriceSpec = { id: 'user:prices:one', name: 'Prices', unit: 'chaos', weights: {} }

/** A method spec with only the kind set, which is how a block that needs no parameters reads. */
function method(kind: string): MethodSpec {
  return { kind, modifiers: null, augment: null, essence: null }
}

const inner: CraftSpec = {
  id: 'user:craft:inner',
  name: 'Inner',
  goal: filter.id,
  influence: 'none',
  prices: prices.id,
  start: 'normal',
  entry: 'b1',
  blocks: [{ id: 'b1', method: method('alt_to_mod'), craft: null, filter: null, position: null }],
  wires: [{ from: 'b1', exit: 'magic', to: 'done' }],
}

const outer: CraftSpec = {
  id: 'user:craft:outer',
  name: 'Outer',
  goal: filter.id,
  influence: 'none',
  prices: prices.id,
  start: 'normal',
  entry: 'b1',
  blocks: [{ id: 'b1', method: null, craft: inner.id, filter: null, position: null }],
  wires: [{ from: 'b1', exit: 'magic', to: 'done' }],
}

const library: Library = { version: 2, filters: [filter], prices: [prices], crafts: [inner, outer] }
const empty: Library = { version: 2, filters: [], prices: [], crafts: [] }

/** A craft that leaves through the exit `rare` twice, on two different blocks. */
const twoWays: CraftSpec = {
  id: 'user:craft:two',
  name: 'Two ways',
  goal: filter.id,
  influence: 'none',
  prices: prices.id,
  start: 'normal',
  entry: 'b1',
  blocks: [
    { id: 'b1', method: method('regal_to'), craft: null, filter: null, position: null },
    { id: 'b2', method: method('desired_pair'), craft: null, filter: null, position: null },
  ],
  wires: [
    { from: 'b1', exit: 'rare', to: 'done' },
    { from: 'b1', exit: 'normal', to: 'b2' },
    { from: 'b2', exit: 'rare', to: 'done' },
  ],
}

/** A storage holding one JSON value under the library's key. */
function storageOf(value: unknown): Storage {
  const text = JSON.stringify(value)
  return { getItem: (key: string) => (key === STORAGE_KEY ? text : null) } as unknown as Storage
}

describe('the preset library', () => {
  it('round-trips through export and import', () => {
    expect(importJson(exportJson(library), empty, bases)).toEqual(library)
  })

  it('renames a clashing id and rewrites every reference to it', () => {
    const existing: Library = {
      version: 2,
      filters: [{ ...filter, name: 'Existing filter' }],
      prices: [{ ...prices, name: 'Existing prices' }],
      crafts: [{ ...inner, name: 'Existing inner' }],
    }
    const merged = importJson(exportJson(library), existing, bases)
    expect(merged.filters).toHaveLength(2)
    const importedFilter = merged.filters.find((entry) => entry.name === 'One')
    const importedPrices = merged.prices.find((entry) => entry.name === 'Prices')
    const importedInner = merged.crafts.find((entry) => entry.name === 'Inner')
    const importedOuter = merged.crafts.find((entry) => entry.name === 'Outer')
    expect(importedFilter?.id).toBeDefined()
    expect(importedPrices?.id).toBeDefined()
    expect(importedInner?.id).toBeDefined()
    expect(importedOuter?.id).toBeDefined()
    expect(importedFilter!.id).not.toBe(filter.id)
    expect(importedPrices!.id).not.toBe(prices.id)
    expect(importedInner!.id).not.toBe(inner.id)
    expect(importedInner!.goal).toBe(importedFilter!.id)
    expect(importedInner!.prices).toBe(importedPrices!.id)
    expect(importedOuter!.goal).toBe(importedFilter!.id)
    expect(importedOuter!.prices).toBe(importedPrices!.id)
    expect(importedOuter!.blocks[0].craft).toBe(importedInner!.id)
  })

  it('refuses to remove a craft another craft uses as a block', () => {
    const result = remove(library, 'craft', inner.id)
    expect('error' in result && result.error).toBe('used by "Outer"')
  })

  it('refuses to remove a filter a block names', () => {
    const blockFilter: FilterSpec = { ...filter, id: 'user:filter:block', name: 'Block filter' }
    const craft: CraftSpec = {
      ...inner,
      id: 'user:craft:block-filter',
      name: 'Block filter craft',
      blocks: [{ id: 'b1', method: method('alt_to_mod'), craft: null, filter: blockFilter.id, position: null }],
    }
    const held: Library = { version: 2, filters: [filter, blockFilter], prices: [prices], crafts: [craft] }
    expect(remove(held, 'filter', blockFilter.id)).toEqual({ error: 'used by "Block filter craft"' })
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
        { id: 'y1', method: method('regal_to'), craft: null, filter: null, position: null },
        { id: 'y2', method: null, craft: twoWays.id, filter: null, position: null },
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

describe('a version 1 library', () => {
  const v1 = {
    version: 1,
    goals: [
      {
        id: filter.id,
        name: filter.name,
        base: filter.base,
        influence: 'none',
        prefixes: filter.prefixes,
        suffixes: filter.suffixes,
        desired: filter.desired,
        good: filter.good,
      },
    ],
    prices: [prices],
    crafts: [
      {
        id: inner.id,
        name: inner.name,
        goal: filter.id,
        prices: prices.id,
        start: 'normal',
        entry: 'b1',
        blocks: [{ id: 'b1', method: method('alt_to_mod'), craft: null, position: null }],
        wires: [{ from: 'b1', exit: 'magic', to: 'done' }],
      },
      {
        id: 'user:craft:builtin-goal',
        name: 'Built-in goal craft',
        goal: builtinFilter.id,
        prices: prices.id,
        start: 'normal',
        entry: 'b1',
        blocks: [{ id: 'b1', method: method('alt_to_mod'), craft: null, position: null }],
        wires: [{ from: 'b1', exit: 'magic', to: 'done' }],
      },
    ],
  }

  it('migrates its goals to filters and gives each craft an influence', () => {
    const { library: migrated, notice } = load(storageOf(v1), builtins, bases)
    expect(notice).toBeNull()
    expect(migrated.version).toBe(2)
    expect(migrated.filters.map((entry) => entry.id)).toEqual([builtinFilter.id, filter.id])
    expect(migrated.filters.find((entry) => entry.id === filter.id)).toEqual(filter)
    // A user goal's own influence is kept; a built-in goal takes the first its base offers.
    expect(migrated.crafts.find((entry) => entry.id === inner.id)!.influence).toBe('none')
    expect(migrated.crafts.find((entry) => entry.id === 'user:craft:builtin-goal')!.influence).toBe('Shaper')
    expect(migrated.crafts.every((craft) => craft.blocks.every((block) => block.filter === null))).toBe(true)
  })

  it("takes a built-in goal's influence from the built-in craft built on it", () => {
    // The Shaper's orb goal is on a shield, whose first influence is Shaper, but its craft has none.
    const slam: CraftSpec = { ...inner, id: 'builtin:craft:slam', goal: builtinFilter.id, influence: 'none' }
    const { library: migrated } = load(storageOf(v1), { ...builtins, crafts: [slam] }, bases)
    expect(migrated.crafts.find((entry) => entry.id === 'user:craft:builtin-goal')!.influence).toBe('none')
  })
})

describe('importing presets', () => {
  it('rewrites a block filter and a craft goal when the filter id clashes', () => {
    const blockFilter: FilterSpec = { ...filter, id: 'user:filter:block', name: 'Block filter' }
    const payload: Library = {
      version: 2,
      filters: [blockFilter],
      prices: [prices],
      crafts: [
        {
          ...inner,
          goal: blockFilter.id,
          blocks: [{ id: 'b1', method: method('alt_to_mod'), craft: null, filter: blockFilter.id, position: null }],
        },
      ],
    }
    const existing: Library = { version: 2, filters: [blockFilter], prices: [], crafts: [] }
    const merged = importJson(JSON.stringify(payload), existing, bases)

    const importedFilter = merged.filters.find((entry) => entry.id !== blockFilter.id)
    const importedCraft = merged.crafts.find((entry) => entry.name === inner.name)
    expect(importedFilter).toBeDefined()
    expect(importedFilter!.id).not.toBe(blockFilter.id)
    expect(importedCraft).toBeDefined()
    expect(importedCraft!.goal).toBe(importedFilter!.id)
    expect(importedCraft!.blocks[0].filter).toBe(importedFilter!.id)
  })
})
