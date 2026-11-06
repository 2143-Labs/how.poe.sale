import { describe, expect, it } from 'vitest'
import { fixed, groupTies } from '../steps'
import type { StepView } from '../types'

function step(method: string, tied: boolean): StepView {
  return {
    method,
    name: method,
    stands: 'advances',
    cost: '1 × Chaos Orb',
    finish: 0,
    goal: 0.5,
    better: 0.5,
    worse: 0,
    blocked: 0,
    results: 1,
    does: '',
    tied,
  }
}

describe('groupTies', () => {
  it('starts a row at every untied step', () => {
    const rows = groupTies([step('a', false), step('b', true), step('c', false)])
    expect(rows.map((row) => row.first.method)).toEqual(['a', 'c'])
    expect(rows.map((row) => row.others.length)).toEqual([1, 0])
  })

  it('prints three decimals', () => {
    expect(fixed(10 / 12)).toBe('0.833')
  })
})
