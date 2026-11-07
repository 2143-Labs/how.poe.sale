import type { AbsView } from '../types'

/** A chance as a percentage with two decimals. */
export function percent(p: number): string {
  return `${(p * 100).toFixed(2)}%`
}

export function nodeTitle(view: AbsView): string {
  return `${view.base} · item level ${view.itemLevel}`
}

/** An abstract item on one line: rarity, influences, tracked modifiers and "any" slots. */
export function summary(view: AbsView): string {
  const parts = [view.rarity, ...view.influences, ...view.prefixes, ...view.suffixes]
  if (view.anyPrefixes > 0) parts.push(`any prefix ×${view.anyPrefixes}`)
  if (view.anySuffixes > 0) parts.push(`any suffix ×${view.anySuffixes}`)
  if (view.prefixes.length + view.suffixes.length + view.anyPrefixes + view.anySuffixes === 0) {
    parts.push('no modifiers')
  }
  return parts.join(' · ')
}
