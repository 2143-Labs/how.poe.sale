import type { ClassView, EdgeView, NodeView } from '../types'

/** A chance as a percentage with two decimals; below 0.01% to three significant figures, so a tiny chance is not 0.00%. */
export function percent(p: number): string {
  if (p > 0 && p < 0.0001) return `${(p * 100).toLocaleString('en-US', { maximumSignificantDigits: 3 })}%`
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

/**
 * Where a node reached by more than one pinned step got its items: the step that gave them. Its
 * odds onward come from those items, so a merged node says which they are.
 */
export function itemsFrom(node: NodeView, nodes: NodeView[], edges: EdgeView[]): string | null {
  const source = node.source
  if (!source || edges.filter((edge) => edge.to === node.id).length < 2) return null
  const step = edges.find((edge) => edge.from === source.from && edge.to === node.id && edge.method === source.method)
  const name = step?.name ?? source.method
  const from = nodes.find((n) => n.id === source.from)
  return from ? `items from: ${name} on ${summary(from.view)}` : `items from: an earlier ${name} step`
}
