import { beforeEach, describe, expect, it } from 'vitest'
import draftJson from '../__fixtures__/simplexDraft.json'
import { normalizeDraft } from '../problem'
import { emptyProblemState, persistedState, useAppStore } from '../store'
import type { ProcessProposalJson } from '../types'

const draft = normalizeDraft(draftJson)
const stateKey = '{"base":16,"ilvl":84,"rarity":"Rare","influences":33,"catalyst":null,"lock":0}'

const proposal: ProcessProposalJson = {
  process: {
    id: 'p1',
    output: 'main',
    recipe: 'reforge',
    kind: { craft: { from: null, allowed: ['chaos'] } },
    expanded: null,
    added: [],
    disabled: [],
    buy: [],
    sell: [],
  },
  newGoods: [],
  name: 'Chaos reforge',
  cost: 120,
  pSuccess: 1,
  status: 'Ready',
  reason: null,
}

describe('applyDraft', () => {
  beforeEach(() => useAppStore.setState({ ...emptyProblemState(), notice: null }))

  it('makes the parsed item the root good and prices it from the buy note', () => {
    useAppStore.getState().applyDraft(draft)
    const state = useAppStore.getState()
    expect(state.goods).toHaveLength(1)
    expect(state.goods[0].id).toBe('main')
    expect(state.goods[0].buy).toEqual(800 * 389.5)
    expect(state.root).toBe('main')
    expect(state.goods[0].target?.requirements).toHaveLength(3)
    expect(state.view).toBe('market')
  })

  it('replaces the market when the target changes, keeping the root prices', () => {
    const store = useAppStore.getState()
    store.applyDraft(draft)
    useAppStore.getState().useProcessProposal(proposal)
    useAppStore.getState().setFocus('p1')
    useAppStore.getState().addSeed(stateKey)
    useAppStore.getState().hide(stateKey, '{"kind":"Chaos"}')
    useAppStore.getState().setBuy(stateKey, 389.5)
    useAppStore.getState().setGoodPrice('main', { sell: 5 })
    useAppStore.getState().setPosition('market:main', { x: 1, y: 2 })

    // delete a requirement row and re-apply: the old process keys no longer fit the target
    useAppStore
      .getState()
      .applyDraft({ ...draft, buyNote: null, minHits: 2, requirements: draft.requirements.slice(1) })
    const after = useAppStore.getState()
    expect(after.goods[0].target?.requirements).toHaveLength(2)
    expect(after.processes).toEqual([])
    expect(after.focus).toBeNull()
    expect(after.goods[0].buy).toEqual(800 * 389.5)
    expect(after.goods[0].sell).toBe(5)
    expect(after.positions).toEqual({})
  })

  it('leaves the market alone when the draft has no base', () => {
    useAppStore.getState().applyDraft(draft)
    useAppStore.getState().applyDraft({ ...draft, base: null })
    expect(useAppStore.getState().goods[0].target?.base).toBe('Simplex Amulet')
    expect(useAppStore.getState().notice).toMatch(/No base item/)
  })
})

describe('focus-process node actions', () => {
  beforeEach(() => {
    useAppStore.setState({ ...emptyProblemState(), notice: null })
    useAppStore.getState().applyDraft(draft)
    useAppStore.getState().useProcessProposal(proposal)
  })

  it('writes Use / Expand / Hide into the focus process', () => {
    useAppStore.getState().addSeed(stateKey)
    useAppStore.getState().hide(stateKey, '{"kind":"Chaos"}')
    const process = useAppStore.getState().processes[0]
    expect(process.id).toBe('p1')
    expect(process.added).toEqual([stateKey])
    expect(process.expanded).toEqual(['goal', stateKey])
    expect(process.disabled).toEqual([[stateKey, '{"kind":"Chaos"}']])
  })

  it('writes tree buy/sell prices into the focus process, not the good', () => {
    useAppStore.getState().setBuy('goal', 500)
    useAppStore.getState().setSell('goal', 30)
    const process = useAppStore.getState().processes[0]
    expect(process.buy).toEqual([['goal', 500]])
    expect(process.sell).toEqual([['goal', 30]])
    expect(useAppStore.getState().goods[0].sell).toBeNull()

    useAppStore.getState().setBuy('goal', null)
    expect(useAppStore.getState().processes[0].buy).toEqual([])
  })

  it('selection of a proposal makes it the focus and opens the process view', () => {
    const state = useAppStore.getState()
    expect(state.focus).toBe('p1')
    expect(state.view).toBe('process')
    expect(state.selectedGood).toBe('main')
  })
})

describe('market edits', () => {
  beforeEach(() => {
    useAppStore.setState({ ...emptyProblemState(), notice: null })
    useAppStore.getState().applyDraft(draft)
  })

  it('numbers brick goods and leaves them unpriced', () => {
    const first = useAppStore.getState().addBrickGood('main', '+1 all Skill Gems')
    const second = useAppStore.getState().addBrickGood('main', '+1 all Skill Gems')
    expect([first, second]).toEqual(['brick-1', 'brick-2'])
    const state = useAppStore.getState()
    expect(state.goods[1].sell).toBeNull()
    expect(state.goods[1].target.rarity).toBe('Rare')
    expect(state.selectedGood).toBe('brick-2')
  })

  it('adds a pasted item as another good without replacing the market', () => {
    const id = useAppStore.getState().addGoodFromDraft(draft)
    const state = useAppStore.getState()
    expect(id).toBe('good-1')
    expect(state.root).toBe('main')
    expect(state.goods.map((g) => g.id)).toEqual(['main', 'good-1'])
    expect(state.goods[1].buy).toBeNull()
    expect(state.goods[1].sell).toBeNull()
    expect(state.selectedGood).toBe('good-1')
  })

  it('drops the stale expansion when a good target is edited', () => {
    useAppStore.getState().useProcessProposal({ ...proposal, process: { ...proposal.process, expanded: ['goal'] } })
    const target = useAppStore.getState().goods[0].target
    useAppStore.getState().updateGoodTarget('main', { ...target, minHits: 1 })
    expect(useAppStore.getState().processes[0].expanded).toBeNull()
    expect(useAppStore.getState().goods[0].target.minHits).toBe(1)
  })
})

describe('addCustomCraft', () => {
  beforeEach(() => useAppStore.setState({ ...emptyProblemState(), notice: null }))

  it('adds the input good once and a process per craft', () => {
    useAppStore.getState().applyDraft(draft)
    const spec = {
      output: 'main',
      name: 'Veiled chaos and unveil',
      input: { kind: 'without' as const, remove: [0] },
      chance: 0.25,
      cost: 10,
      keepsInput: false,
    }
    expect(useAppStore.getState().addCustomCraft(spec)).toBe('custom-1')
    let state = useAppStore.getState()
    const input = state.goods.find((good) => good.id === 'custom-input-1')
    expect(input?.target.requirements).toHaveLength(2)
    expect(input?.target.minHits).toBe(2)
    const process = state.processes.find((p) => p.id === 'custom-1')
    expect(process?.kind).toEqual({
      custom: { from: 'custom-input-1', name: 'Veiled chaos and unveil', chance: 0.25, cost: 10, keepsInput: false },
    })
    expect(state.selectedGood).toBe('main')
    expect(state.view).toBe('market')

    expect(useAppStore.getState().addCustomCraft(spec)).toBe('custom-2')
    state = useAppStore.getState()
    expect(state.goods.filter((good) => good.id.startsWith('custom-input-'))).toHaveLength(1)
    expect(state.processes.find((p) => p.id === 'custom-2')?.kind).toMatchObject({ custom: { from: 'custom-input-1' } })
  })

  it('rejects a free craft from nothing and a zero chance', () => {
    useAppStore.getState().applyDraft(draft)
    const base = { output: 'main', name: 'x', chance: 0.5, cost: 0, keepsInput: false }
    expect(useAppStore.getState().addCustomCraft({ ...base, input: { kind: 'none' } })).toBeNull()
    expect(
      useAppStore.getState().addCustomCraft({ ...base, cost: 1, chance: 0, input: { kind: 'none' } }),
    ).toBeNull()
    expect(useAppStore.getState().processes).toHaveLength(0)
  })
})

describe('requestPaste', () => {
  it('bumps the nonce on every request and is never saved', () => {
    useAppStore.setState({ pasteRequest: null })
    useAppStore.getState().requestPaste('a')
    expect(useAppStore.getState().pasteRequest).toEqual({ text: 'a', nonce: 1 })
    useAppStore.getState().requestPaste('a')
    expect(useAppStore.getState().pasteRequest).toEqual({ text: 'a', nonce: 2 })
    expect('pasteRequest' in persistedState(useAppStore.getState())).toBe(false)
  })
})
