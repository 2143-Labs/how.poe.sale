// What a method's outcomes are, as a row to compare two cards on.
import type { AppliedView } from '../types'
import { summary } from './format'

/** One class the row can land in: its chance, the half width of its interval, and how it reads. */
export interface Cell {
  p: number
  width: number
  at: string
}

/** A method's row: every class it can land in, keyed so two cards' rows line up. */
export type Row = Map<string, Cell>

/** The key two outcomes share when they are the same class, or the same non-item end. */
function cellKey(outcome: AppliedView['outcomes'][number]): string {
  const { view, kind } = outcome
  return view ? JSON.stringify([view.prefixes, view.suffixes, view.anyPrefixes, view.anySuffixes]) : kind
}

function cellLabel(outcome: AppliedView['outcomes'][number]): string {
  const { view, kind, reason } = outcome
  if (view) return summary(view)
  return reason ? `${kind} (${reason})` : kind
}

/** The row a method leaves from a card. */
export function rowsOf(applied: AppliedView): Row {
  const row: Row = new Map()
  for (const outcome of applied.outcomes) {
    const key = cellKey(outcome)
    const width = outcome.interval ? (outcome.interval[1] - outcome.interval[0]) / 2 : 0
    const seen = row.get(key)
    row.set(key, {
      p: (seen?.p ?? 0) + outcome.p,
      width: Math.max(seen?.width ?? 0, width),
      at: seen?.at ?? cellLabel(outcome),
    })
  }
  return row
}

/**
 * How two rows disagree, when they do: the largest gap past its tolerance, the class it is at, and
 * that tolerance. Two rows of the same item differ by sampling noise, so a gap only counts when it
 * is wider than the two intervals put together; rows with no interval must agree exactly.
 */
export function rowsDiffer(a: Row, b: Row): { gap: number; at: string; tol: number } | null {
  let excess = 0
  let worst = { gap: 0, at: '', tol: 0 }
  for (const key of new Set([...a.keys(), ...b.keys()])) {
    const left = a.get(key)
    const right = b.get(key)
    const gap = Math.abs((left?.p ?? 0) - (right?.p ?? 0))
    const tol = 1.6 * Math.hypot(left?.width ?? 0, right?.width ?? 0) + 0.001
    if (gap - tol > excess) {
      excess = gap - tol
      worst = { gap, at: left?.at ?? right?.at ?? key, tol }
    }
  }
  return excess > 0 ? worst : null
}
