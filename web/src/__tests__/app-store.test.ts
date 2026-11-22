import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AppClient } from '../app/client'
import { SCOPES } from '../app/scope'
import { createAppStore, type AppStore } from '../app/store'
import type { LoopRequest, LoopView, ModOptionView, Start } from '../app/types'

const START: Start = {
  version: '0.1.0',
  patch: '3.30',
  bases: ['Archdemon Crown', 'Supreme Spiked Shield'],
  influences: ['shaper'],
  stages: ['desired', 'good pair', 'good prefix', 'good suffix', 'other', 'spare', 'spent rare', 'reset', 'stop'],
  accepts: ['has-desired', 'desired-or-good-both-sides', 'any'],
  currencies: [
    {
      id: 'currency/exalted',
      name: 'Exalted Orb',
      costs: [{ key: 'currency/exalted', name: 'Exalted Orb', amount: 1 }],
    },
  ],
  priceDefaults: { 'currency/exalted': 0.83, 'currency/transmutation': 0.021 },
}

const LOOP: LoopView = {
  start: { text: 'Normal Archdemon Crown', stage: 'spare' },
  depths: [],
  totals: [],
  live: { exact: '1/1', percent: '100.000000' },
  stopped: null,
  priced: null,
  warnings: [],
}

/** The setup every page opens on, with a fake engine that records each loop request. */
function opened(): { store: AppStore; requests: LoopRequest[] } {
  const requests: LoopRequest[] = []
  const client: AppClient = {
    start: async () => START,
    mods: async (): Promise<ModOptionView[]> => [],
    loopReport: async (request) => {
      requests.push(request)
      if (!('currency/exalted' in request.prices.prices)) throw new Error('no price for Exalted Orb')
      return LOOP
    },
    applyCurrency: async () => ({
      currency: { id: 'currency/exalted', name: 'Exalted Orb' },
      total: '1',
      outcomes: [],
      more: 0,
      rest: { exact: '0/1', percent: '0' },
    }),
    dispose: () => undefined,
  }
  const store = createAppStore(SCOPES.poe1, {
    client,
    start: START,
    hash: '',
    storage: null,
    dropHash: () => undefined,
    debounceMs: 0,
  })
  store.getState().boot()
  return { store, requests }
}

/** Runs the store's zero-millisecond debounce without waiting on the wall clock. */
const settle = () => vi.advanceTimersByTimeAsync(1)

describe('the app store', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('opens on the default setup and asks for the unpriced loop', async () => {
    const { requests } = opened()
    await settle()
    const request = requests[0]
    expect(request.item).toEqual({ base: 'Archdemon Crown', itemLevel: 86, rarity: 'normal', influences: [] })
    expect(request.goal).toEqual({ desired: [], good: [], accept: 'desired-or-good-both-sides' })
    expect(request.depth).toBe(6)
    expect(request.price).toBe(false)
    expect(request.prices.tradePenalty).toBe(0.02)
    expect(request.prices.prices).toEqual(START.priceDefaults)
  })

  it('leaves only good when a desired toggle is followed by a good one', async () => {
    const { store, requests } = opened()
    store.getState().setMod('prefix', 'MaximumTotemUber1', 'desired')
    store.getState().setMod('prefix', 'MaximumTotemUber1', 'good')
    await settle()
    const request = requests.at(-1)!
    expect(request.goal.desired).toEqual([])
    expect(request.goal.good).toEqual([{ side: 'prefix', id: 'MaximumTotemUber1', orBetter: false }])
  })

  it('carries orBetter into the request', async () => {
    const { store, requests } = opened()
    store.getState().setMod('suffix', 'AllResistances6', 'desired')
    store.getState().setOrBetter('AllResistances6', true)
    await settle()
    expect(requests.at(-1)!.goal.desired).toEqual([{ side: 'suffix', id: 'AllResistances6', orBetter: true }])
  })

  it('drops a cleared price from the request and shows the engine error', async () => {
    const { store, requests } = opened()
    await settle()
    store.getState().setPrice('currency/exalted', null)
    await settle()
    expect(requests.at(-1)!.prices.prices).not.toHaveProperty('currency/exalted')
    expect(store.getState().error).toContain('no price for Exalted Orb')
  })
})
