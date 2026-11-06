// The JSON the worker answers with, mirroring `crates/poe1/src/api.rs` field for field.

/** A pattern document. The page edits it as JSON text and only reads its name. */
export type PatternSpec = { name: string } & Record<string, unknown>

/** A pasted item as the engine read it. */
export interface ItemView {
  /** The item in the engine's own form (`ItemSpec`). */
  item: Record<string, unknown>
  /** The item printed back as game text. */
  text: string
  /** Every line the reader could not explain. */
  warnings: string[]
}

/** The item as it stands against the pattern. */
export interface NowView {
  progress: number
  matches: number
  blocked: number
  /** Whether the item can pass the pattern's item filter at all. */
  fits: boolean
}

/** One step the item allows. */
export interface StepView {
  method: string
  name: string
  /** finishes, advances, unblocks, holds or sets back. */
  stands: string
  cost: string
  finish: number
  goal: number
  better: number
  worse: number
  blocked: number
  results: number
  /** What the method does, one sentence per op. */
  does: string
  /** Whether the step does exactly what the one before it does. */
  tied: boolean
}

/** A method that does not apply, or cannot be walked backwards, and why. */
export interface UnmetView {
  method: string
  name: string
  reason: string
}

/** One input that could have produced the item. */
export interface SourceView {
  method: string
  name: string
  p: number
  note: string | null
}

/** The inputs that read as one pattern state. */
export interface GroupView {
  state: string
  best: number
  /** The most likely source's input, as game text. */
  example: string
  sources: SourceView[]
}

export interface SourcesView {
  found: number
  groups: GroupView[]
  skipped: UnmetView[]
}

/** The one-step report. */
export interface SolveView {
  now: NowView
  exact: StepView[]
  certain: StepView[]
  blind: StepView[]
  unmet: UnmetView[]
  sources: SourcesView
}

/** An example paste. */
export interface Example {
  id: string
  label: string
  text: string
}

/** What the worker answers `start` with. */
export interface StartInfo {
  version: string
  patterns: PatternSpec[]
  examples: Example[]
}
