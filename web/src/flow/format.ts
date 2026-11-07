import type { ClassView } from '../types'

/** A chance as a percentage with two decimals. */
export function percent(p: number): string {
  return `${(p * 100).toFixed(2)}%`
}

/** A count to three significant figures, so "about 5.82 tries" and "about 1,204 tries" both fit. */
export function count(n: number): string {
  if (!Number.isFinite(n)) return 'never'
  return String(Number.parseFloat(n.toPrecision(3)))
}

/** A chance's 95% range, when it has one. */
export function range(interval: [number, number] | null): string {
  return interval ? ` (95% ${percent(interval[0])}–${percent(interval[1])})` : ''
}

/** About how many tries a step takes: one over its chance. */
export function tries(p: number): string {
  const n = count(1 / p)
  return `about ${n} ${n === '1' ? 'try' : 'tries'}`
}

export function nodeTitle(view: ClassView): string {
  return `${view.base} · item level ${view.itemLevel}`
}

/** A class on one line: rarity, influences, tracked modifiers and "any" slots. */
export function summary(view: ClassView): string {
  const parts = [view.rarity, ...(view.corrupted ? ['corrupted'] : []), ...view.influences]
  parts.push(...view.prefixes, ...view.suffixes)
  if (view.anyPrefixes > 0) parts.push(`any prefix ×${view.anyPrefixes}`)
  if (view.anySuffixes > 0) parts.push(`any suffix ×${view.anySuffixes}`)
  if (view.prefixes.length + view.suffixes.length + view.anyPrefixes + view.anySuffixes === 0) {
    parts.push('no modifiers')
  }
  return parts.join(' · ')
}
