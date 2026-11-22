// The craft planner still formats through this module (`src/craft/format.ts` re-exports it), so the
// two functions it needs stay here; the app's own formatting lives in `src/app/format.ts`.

export { count } from '../app/format'

/** A chance as a percentage with two decimals; below 0.01% to three significant figures, so a tiny chance is not 0.00%. */
export function percent(p: number): string {
  if (p > 0 && p < 0.0001) return `${(p * 100).toLocaleString('en-US', { maximumSignificantDigits: 3 })}%`
  return `${(p * 100).toFixed(2)}%`
}
