import type { StepView } from './types'

/** One table row: a step, and the steps after it that do exactly the same thing. */
export interface StepRow {
  first: StepView
  others: StepView[]
}

/** Folds tied steps into the row of the step before them; a new row starts at every untied step. */
export function groupTies(steps: StepView[]): StepRow[] {
  const rows: StepRow[] = []
  for (const step of steps) {
    const last = rows[rows.length - 1]
    if (step.tied && last) last.others.push(step)
    else rows.push({ first: step, others: [] })
  }
  return rows
}

/** A number with `d` decimals, as the catalog prints it. */
export function fixed(x: number, d = 3): string {
  return x.toFixed(d)
}
