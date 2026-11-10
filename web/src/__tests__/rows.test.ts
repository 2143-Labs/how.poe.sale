import { describe, expect, it } from 'vitest'
import { rowsDiffer, rowsOf } from '../flow/rows'
import type { AppliedView, ClassView, OutcomeView } from '../types'

function view(prefixes: string[], suffixes: string[], anyPrefixes: number, anySuffixes: number): ClassView {
  return { base: 'Helmet', itemLevel: 87, rarity: 'rare', corrupted: false, influences: [], prefixes, suffixes, anyPrefixes, anySuffixes }
}

function outcome(p: number, interval: [number, number] | null, v: Partial<OutcomeView> = {}): OutcomeView {
  return { key: `k${p}`, kind: 'item', p, interval, view: view([], [], 2, 2), reason: null, ...v }
}

function applied(outcomes: OutcomeView[]): AppliedView {
  return { node: 'n', method: 'm', name: 'M', cost: '', simulated: 200000, repeatable: true, outcomes, watched: [] }
}

describe('reading a row', () => {
  it('adds the outcomes that land on one class and keeps their widest interval', () => {
    const row = rowsOf(
      applied([
        outcome(0.2, [0.18, 0.22]),
        outcome(0.1, [0.08, 0.12]),
        outcome(0.7, null, { view: view(['+40 to maximum Life'], [], 1, 2) }),
      ]),
    )
    expect(row.size).toBe(2)
    const same = [...row.values()].find((cell) => cell.p > 0.25)!
    expect(same.p).toBeCloseTo(0.3)
    expect(same.width).toBeCloseTo(0.02)
    expect(same.at).toContain('any prefix ×2')
  })

  it('keys a non-item end apart from every class', () => {
    const row = rowsOf(applied([outcome(0.4, null, { kind: 'refused', view: null, reason: 'needs a free slot' })]))
    expect(row.size).toBe(1)
    expect([...row.values()][0]).toMatchObject({ p: 0.4, width: 0 })
    expect([...row.values()][0].at).toContain('needs a free slot')
  })
})

describe('comparing two rows', () => {
  it('calls sampling noise a match', () => {
    const a = rowsOf(applied([outcome(0.2326, [0.2308, 0.2344]), outcome(0.7674, [0.7656, 0.7692])]))
    const b = rowsOf(applied([outcome(0.2337, [0.2319, 0.2355]), outcome(0.7663, [0.7645, 0.7681])]))
    expect(rowsDiffer(a, b)).toBeNull()
  })

  it('reports the class a real difference is at', () => {
    const a = rowsOf(applied([outcome(0.48, [0.4777, 0.4823])]))
    const b = rowsOf(applied([outcome(0.19, [0.1877, 0.1923])]))
    const diff = rowsDiffer(a, b)
    expect(diff?.gap).toBeCloseTo(0.29)
    expect(diff?.at).toContain('any prefix ×2')
  })

  it('reports a class only one row has', () => {
    const a = rowsOf(applied([outcome(0.5, null)]))
    const b = rowsOf(applied([outcome(0.5, null, { view: view([], [], 3, 2) })]))
    const diff = rowsDiffer(a, b)
    expect(diff?.gap).toBe(0.5)
    expect(diff?.tol).toBeCloseTo(0.001)
  })

  it('will not call two rows of the same class equal when their split differs', () => {
    const a = rowsOf(applied([outcome(0.5, null, { view: view([], [], 2, 2) }), outcome(0.5, null, { view: view([], [], 1, 3) })]))
    const b = rowsOf(applied([outcome(1, null)]))
    expect(rowsDiffer(a, b)?.gap).toBeCloseTo(0.5)
  })
})
