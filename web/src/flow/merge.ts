// Merging chart cards: a card folded into another is shown as that card from then on.
import { normalizeSpec, type EdgeView, type Merge, type NodeView, type SlotSpec } from '../types'

/** The card `id` is shown as: the end of its `from → into` chain. */
export function resolveId(merges: Merge[], id: string): string {
  const seen = new Set<string>()
  let at = id
  while (!seen.has(at)) {
    seen.add(at)
    const next = merges.find((merge) => merge.from === at)
    if (!next) break
    at = next.into
  }
  return at
}

/** `from` names the slot on the same side: the same id, and both stand for the family or neither does. */
function names(side: SlotSpec[], slot: SlotSpec): boolean {
  return side.some((own) => own.id === slot.id && (own.orBetter === true) === (slot.orBetter === true))
}

/**
 * A crafted metamod ("Prefixes Cannot Be Changed", "Cannot roll Attack modifiers"): it changes
 * what a reforge does, so an item that has one never behaves as one without it.
 */
const metamod = (slot: SlotSpec) => slot.id.includes('MasterItemGeneration')

/**
 * Why `from` cannot be merged into `into`, or `null` when it can: the same item apart from
 * modifiers `from` names and `into` counts as any.
 */
export function mergeBlocker(from: NodeView, into: NodeView): string | null {
  const a = normalizeSpec(from.spec)
  const b = normalizeSpec(into.spec)
  if (!a || !b) return 'an item could not be read'
  if (a.base !== b.base) return 'different base'
  if (a.itemLevel !== b.itemLevel) return 'different item level'
  if (a.rarity !== b.rarity) return 'different rarity'
  if ([...a.influences].sort().join(',') !== [...b.influences].sort().join(',')) return 'different influences'
  if (a.corrupted !== b.corrupted) return 'only one is corrupted'
  if (a.prefixes.length + a.anyPrefixes !== b.prefixes.length + b.anyPrefixes) return 'different number of prefixes'
  if (a.suffixes.length + a.anySuffixes !== b.suffixes.length + b.anySuffixes) return 'different number of suffixes'
  const lostMetamod =
    a.prefixes.some((slot) => metamod(slot) && !names(b.prefixes, slot)) ||
    a.suffixes.some((slot) => metamod(slot) && !names(b.suffixes, slot))
  if (lostMetamod) return 'only one carries a crafted metamod'
  if (!b.prefixes.every((slot) => names(a.prefixes, slot)) || !b.suffixes.every((slot) => names(a.suffixes, slot))) {
    return 'the target names a modifier this item lacks'
  }
  return null
}

/**
 * The most specific card `from` can merge into without dropping a tracked modifier, or `null`.
 * Ties go to the earliest candidate.
 */
export function autoTarget(from: NodeView, candidates: NodeView[], tracked: Set<string>): NodeView | null {
  const own = normalizeSpec(from.spec)
  if (!own) return null
  let best: NodeView | null = null
  let bestNamed = -1
  for (const candidate of candidates) {
    if (candidate.id === from.id || mergeBlocker(from, candidate) !== null) continue
    const spec = normalizeSpec(candidate.spec)
    if (!spec) continue
    const keepsTracked =
      own.prefixes.every((slot) => !tracked.has(slot.id) || spec.prefixes.some((s) => s.id === slot.id)) &&
      own.suffixes.every((slot) => !tracked.has(slot.id) || spec.suffixes.some((s) => s.id === slot.id))
    if (!keepsTracked) continue
    const named = spec.prefixes.length + spec.suffixes.length
    if (named > bestNamed) {
      best = candidate
      bestNamed = named
    }
  }
  return best
}

/**
 * The chart as shown: merged cards dropped, and each edge drawn between the cards its ends are
 * shown as. A shown card's steps with one method come from a single raw card: itself when it
 * has that step pinned, otherwise the first merged card that does. The same step from cards
 * merged together is never added up; its outcomes that land on one shown card are.
 */
export function applyMerges(
  nodes: NodeView[],
  edges: EdgeView[],
  merges: Merge[],
): { nodes: NodeView[]; edges: EdgeView[] } {
  const hidden = new Set(merges.map((merge) => merge.from))
  const source = new Map<string, string>()
  for (const edge of edges) {
    const from = resolveId(merges, edge.from)
    const step = JSON.stringify([from, edge.method])
    if (!source.has(step) || edge.from === from) source.set(step, edge.from)
  }
  const groups = new Map<string, { from: string; to: string; edges: EdgeView[] }>()
  for (const edge of edges) {
    const from = resolveId(merges, edge.from)
    if (source.get(JSON.stringify([from, edge.method])) !== edge.from) continue
    const to = resolveId(merges, edge.to)
    const key = JSON.stringify([from, to, edge.method])
    const group = groups.get(key)
    if (group) group.edges.push(edge)
    else groups.set(key, { from, to, edges: [edge] })
  }
  const shown = [...groups.values()].map(({ from, to, edges: kept }) => {
    const p = kept.reduce((sum, edge) => sum + edge.p, 0)
    // the sums are never negative, so only the top needs clamping
    const interval: [number, number] | null = kept.every((edge) => edge.interval === null)
      ? null
      : [
          Math.min(1, kept.reduce((sum, edge) => sum + (edge.interval?.[0] ?? edge.p), 0)),
          Math.min(1, kept.reduce((sum, edge) => sum + (edge.interval?.[1] ?? edge.p), 0)),
        ]
    return {
      from,
      to,
      method: kept[0].method,
      name: kept[0].name,
      p,
      interval,
      repeatable: kept.every((edge) => edge.repeatable),
    }
  })
  return { nodes: nodes.filter((node) => !hidden.has(node.id)), edges: shown }
}

/** The cards merged into `id`, directly or through others, in chart order. */
export function mergedInto(rawNodes: NodeView[], merges: Merge[], id: string): NodeView[] {
  return rawNodes.filter((node) => node.id !== id && resolveId(merges, node.id) === id)
}

/** The saved merges that read back as merges. */
export function mergesOf(raw: unknown): Merge[] {
  if (!Array.isArray(raw)) return []
  return raw.flatMap((entry: unknown) => {
    if (!entry || typeof entry !== 'object' || !('from' in entry) || !('into' in entry)) return []
    const { from, into } = entry
    return typeof from === 'string' && typeof into === 'string' ? [{ from, into }] : []
  })
}
