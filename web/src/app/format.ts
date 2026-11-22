import type { Mass } from './types'

/** A count to three significant figures, so "about 5.82 tries" and "about 1,200 tries" both fit. */
export function count(n: number): string {
  if (!Number.isFinite(n)) return 'never'
  return String(Number.parseFloat(n.toPrecision(3)))
}

/** About how many tries a step takes: one over its chance. */
export function tries(p: number): string {
  const n = count(1 / p)
  return `about ${n} ${n === '1' ? 'try' : 'tries'}`
}

/** A mass the engine sent as a percent string, with its sign. */
export function chance(mass: Mass): string {
  return `${mass.percent}%`
}

/** Chaos, to one decimal: a total, a spread or a price. */
export function chaos(n: number): string {
  return n.toFixed(1)
}
