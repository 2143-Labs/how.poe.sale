import { describe, expect, it } from 'vitest'
import { DEMO_ITEMS } from '../demo'

describe('DEMO_ITEMS', () => {
  it('lists the six examples in menu order, without buy notes', () => {
    expect(DEMO_ITEMS.map((item) => item.id)).toEqual(['simplex', 'crown', 'bow', 'staff', 'amulet', 'boots'])
    for (const item of DEMO_ITEMS) {
      expect(item.text.trim()).not.toBe('')
      expect(item.text).not.toContain('\nNote:')
    }
  })
})
