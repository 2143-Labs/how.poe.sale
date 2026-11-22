import { describe, expect, it } from 'vitest'
import { chance, chaos, count, tries } from '../app/format'

describe('count', () => {
  it('keeps three significant figures', () => {
    expect(count(5.8203)).toBe('5.82')
    expect(count(1204)).toBe('1200')
    expect(count(1)).toBe('1')
  })

  it('says never for a count with no value', () => {
    expect(count(Number.POSITIVE_INFINITY)).toBe('never')
  })
})

describe('tries', () => {
  it('reads one over the chance', () => {
    expect(tries(0.5)).toBe('about 2 tries')
    expect(tries(1)).toBe('about 1 try')
  })
})

describe('chance', () => {
  it('appends the sign to the engine percent string', () => {
    expect(chance({ exact: '1/108', percent: '0.925926' })).toBe('0.925926%')
    expect(chance({ exact: '1/1', percent: '100.000000' })).toBe('100.000000%')
  })
})

describe('chaos', () => {
  it('shows one decimal', () => {
    expect(chaos(3.14159)).toBe('3.1')
    expect(chaos(8)).toBe('8.0')
  })
})
