import { describe, expect, it } from 'vitest'
import draftJson from '../__fixtures__/simplexDraft.json'
import { draftToTarget, normalizeDraft, targetSignature, targetToDraft } from '../problem'
import { catalystNames, normalizeSolution, summarizeStateKey } from '../solution'

// Requirement order as the real parser emits it for fixtures/simplex_target.txt.
const target = draftToTarget(
  normalizeDraft({
    ...draftJson,
    requirements: [draftJson.requirements[2], draftJson.requirements[1], draftJson.requirements[0]],
  }),
)!

// Real keys from `Bridge.solve` (HarvestInfluenced predecessors of the uncatalysed ideal state).
const locked =
  '{"base":16,"ilvl":84,"rarity":"Rare","influences":33,"catalyst":null,"prefixes":{"wanted":[],"blocked":[],"untracked":0,"untracked_influenced":0},"suffixes":{"wanted":[{"req":0,"idx":0,"value_ok":true},{"req":1,"idx":0,"value_ok":true}],"blocked":[],"untracked":0,"untracked_influenced":0},"lock":2}'
const steered = locked.replace('"catalyst":null', '"catalyst":[4,20]')
const shaperOnly =
  '{"base":16,"ilvl":84,"rarity":"Rare","influences":1,"catalyst":null,"prefixes":{"wanted":[],"blocked":[],"untracked":0,"untracked_influenced":0},"suffixes":{"wanted":[{"req":0,"idx":0,"value_ok":false}],"blocked":[{"group":3,"influence":true}],"untracked":1,"untracked_influenced":1},"lock":0}'

describe('summarizeStateKey', () => {
  const catalysts = new Map([[4, 'Intrinsic Catalyst']])

  it('names requirements, the lock and nothing else for the target influences', () => {
    expect(summarizeStateKey(locked, target, catalysts)).toBe(
      'prefixes: none · suffixes: 19% increased Attributes, 26% increased Strength · Suffixes Cannot Be Changed',
    )
  })

  it('tells a catalyst-steered predecessor apart from its twin', () => {
    expect(summarizeStateKey(steered, target, catalysts)).toBe(
      'prefixes: none · suffixes: 19% increased Attributes, 26% increased Strength · Suffixes Cannot Be Changed · Intrinsic Catalyst 20%',
    )
  })

  it('shows a different influence set, low rolls and untracked mods', () => {
    expect(summarizeStateKey(shaperOnly, target, catalysts)).toBe(
      'Shaper · prefixes: none · suffixes: 19% increased Attributes (low roll), 1 other tier, 1 other influenced',
    )
  })

  it('returns null for start, goal and junk', () => {
    expect(summarizeStateKey('goal', target)).toBeNull()
    expect(summarizeStateKey('start', target)).toBeNull()
    expect(summarizeStateKey('{not json', target)).toBeNull()
  })
})

describe('catalystNames', () => {
  it('learns catalyst names from ApplyCatalyst labels in the solution', async () => {
    const raw = (await import('../__fixtures__/simplexSolution.json')).default
    expect(catalystNames(normalizeSolution(raw)).get(4)).toBe('Intrinsic Catalyst')
  })
})

describe('target round trip', () => {
  it('turns the applied target back into an editable draft', () => {
    expect(targetSignature(draftToTarget(targetToDraft(target)))).toBe(targetSignature(target))
  })

  it('notices an edited minimum', () => {
    const edited = targetToDraft(target)
    edited.requirements[0] = { ...edited.requirements[0], minValue: 20 }
    expect(targetSignature(draftToTarget(edited))).not.toBe(targetSignature(target))
  })

  it('defaults a missing item level to 1, as the Rust side does', () => {
    expect(draftToTarget({ ...normalizeDraft(draftJson), ilvl: null })!.ilvlMin).toBe(1)
  })
})
