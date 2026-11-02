import { beforeEach, describe, expect, it } from 'vitest'
import draftJson from '../__fixtures__/simplexDraft.json'
import { normalizeDraft } from '../problem'
import { emptyProblemState, useAppStore } from '../store'

const draft = normalizeDraft(draftJson)
const stateKey = '{"base":16,"ilvl":84,"rarity":"Rare","influences":33,"catalyst":null,"lock":0}'

describe('applyDraft', () => {
  beforeEach(() => useAppStore.setState({ ...emptyProblemState(), notice: null }))

  it('sets the goal buy price from the buy note', () => {
    useAppStore.getState().applyDraft(draft)
    expect(useAppStore.getState().buy).toEqual({ goal: 800 * 389.5 })
    expect(useAppStore.getState().target?.requirements).toHaveLength(3)
  })

  it('drops every state-keyed entry when the target changes, keeping the goal prices', () => {
    const store = useAppStore.getState()
    store.applyDraft(draft)
    store.addSeed(stateKey)
    store.hide(stateKey, '{"kind":"Chaos"}')
    store.setBuy(stateKey, 389.5)
    store.setSell(stateKey, 100)
    store.setSell('goal', 5)
    store.setPosition(stateKey, { x: 1, y: 2 })

    // delete a requirement row and re-apply: the old keys no longer fit the target
    useAppStore.getState().applyDraft({ ...draft, buyNote: null, minHits: 2, requirements: draft.requirements.slice(1) })
    const after = useAppStore.getState()
    expect(after.target?.requirements).toHaveLength(2)
    expect(after.added).toEqual([])
    expect(after.expanded).toEqual(['goal'])
    expect(after.disabled).toEqual([])
    expect(after.buy).toEqual({ goal: 800 * 389.5 })
    expect(after.sell).toEqual({ goal: 5 })
    expect(after.positions).toEqual({})
  })

  it('leaves the target alone when the draft has no base', () => {
    useAppStore.getState().applyDraft(draft)
    useAppStore.getState().applyDraft({ ...draft, base: null })
    expect(useAppStore.getState().target?.base).toBe('Simplex Amulet')
    expect(useAppStore.getState().notice).toMatch(/No base item/)
  })
})
