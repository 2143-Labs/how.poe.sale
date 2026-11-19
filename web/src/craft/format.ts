export { count, percent } from '../flow/format'

/** Chaos, to one decimal: a total, a spread or a price. */
export function chaos(n: number): string {
  return n.toFixed(1)
}
