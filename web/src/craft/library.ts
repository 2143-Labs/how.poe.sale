// The presets the page keeps: goals, price lists and crafts. Built-ins come from the catalog and
// are merged in on load; only the user's own presets are written to storage.

import type { BlockSpec, Builtins, CraftSpec, GoalSpec, PriceSpec } from './types'

export type Kind = 'goal' | 'prices' | 'craft'

/** Every preset the page knows: the built-ins from the catalog, then the user's own. */
export interface Library {
  version: 1
  goals: GoalSpec[]
  prices: PriceSpec[]
  crafts: CraftSpec[]
}

/** Where the user's presets live. */
export const STORAGE_KEY = 'how.poe.sale:craft:library'

/** The preset each list falls back to when its selection goes away. */
export const DEFAULTS: Record<Kind, string> = {
  craft: 'builtin:craft:shield-totem',
  goal: 'builtin:goal:shield',
  prices: 'builtin:prices:330',
}

/** What a storage value that will not read as a library leaves in the banner. */
const UNREADABLE = 'Saved presets could not be read and were left untouched.'

/** Whether a preset is the catalog's, which the page never edits or deletes. */
export function isBuiltin(id: string): boolean {
  return id.startsWith('builtin:')
}

/** The preset type a kind names. */
export type PresetOf<K extends Kind> = K extends 'goal' ? GoalSpec : K extends 'prices' ? PriceSpec : CraftSpec

/** One preset by kind and id, or `undefined` when the library has none. */
export function find<K extends Kind>(library: Library, kind: K, id: string): PresetOf<K> | undefined {
  const list: (GoalSpec | PriceSpec | CraftSpec)[] =
    kind === 'goal' ? library.goals : kind === 'prices' ? library.prices : library.crafts
  return list.find((entry) => entry.id === id) as unknown as PresetOf<K> | undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

const strings = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((entry) => typeof entry === 'string')

function isGoal(value: unknown): value is GoalSpec {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.name === 'string' &&
    typeof value.base === 'string' &&
    typeof value.influence === 'string' &&
    strings(value.prefixes) &&
    strings(value.suffixes) &&
    strings(value.desired) &&
    strings(value.good)
  )
}

function isPrices(value: unknown): value is PriceSpec {
  if (!isRecord(value)) return false
  if (typeof value.id !== 'string' || typeof value.name !== 'string' || typeof value.unit !== 'string') return false
  const weights = value.weights
  if (!isRecord(weights)) return false
  return Object.values(weights).every((weight) => typeof weight === 'number')
}

function isBlock(value: unknown): value is BlockSpec {
  if (!isRecord(value) || typeof value.id !== 'string') return false
  if (value.method !== null && !isRecord(value.method)) return false
  if (value.craft !== null && typeof value.craft !== 'string') return false
  if (value.position !== null && !Array.isArray(value.position)) return false
  return true
}

function isCraft(value: unknown): value is CraftSpec {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.name === 'string' &&
    typeof value.goal === 'string' &&
    typeof value.prices === 'string' &&
    typeof value.start === 'string' &&
    typeof value.entry === 'string' &&
    Array.isArray(value.blocks) &&
    value.blocks.every(isBlock) &&
    Array.isArray(value.wires) &&
    value.wires.every(
      (wire) => isRecord(wire) && typeof wire.from === 'string' && typeof wire.exit === 'string' && typeof wire.to === 'string',
    )
  )
}

function isLibrary(value: unknown): value is Library {
  return (
    isRecord(value) &&
    value.version === 1 &&
    Array.isArray(value.goals) &&
    value.goals.every(isGoal) &&
    Array.isArray(value.prices) &&
    value.prices.every(isPrices) &&
    Array.isArray(value.crafts) &&
    value.crafts.every(isCraft)
  )
}

/** The built-ins alone, which is what a page opens on when nothing can be read. */
function builtinLibrary(builtins: Builtins): Library {
  return {
    version: 1,
    goals: [...builtins.goals],
    prices: [...builtins.prices],
    crafts: [...builtins.crafts],
  }
}

/** The user's own presets, which are the only ones stored and exported. */
function userPresets(library: Library): Library {
  return {
    version: 1,
    goals: library.goals.filter((goal) => !isBuiltin(goal.id)),
    prices: library.prices.filter((prices) => !isBuiltin(prices.id)),
    crafts: library.crafts.filter((craft) => !isBuiltin(craft.id)),
  }
}

/** Built-ins first, then the stored presets whose ids the built-ins do not already use. */
function merged(builtins: Library, stored: Library): Library {
  const seen = new Set<string>([...builtins.goals, ...builtins.prices, ...builtins.crafts].map((entry) => entry.id))
  const fresh = <T extends { id: string }>(entries: T[]): T[] =>
    entries.filter((entry) => {
      if (seen.has(entry.id)) return false
      seen.add(entry.id)
      return true
    })
  return {
    version: 1,
    goals: [...builtins.goals, ...fresh(stored.goals)],
    prices: [...builtins.prices, ...fresh(stored.prices)],
    crafts: [...builtins.crafts, ...fresh(stored.crafts)],
  }
}

/**
 * The library a page opens with: the catalog's built-ins, plus whatever storage holds. A value
 * that will not read as a library is reported and left where it is, so the next edit is the first
 * thing that overwrites it.
 */
export function load(storage: Storage | null, builtins: Builtins): { library: Library; notice: string | null } {
  const base = builtinLibrary(builtins)
  if (!storage) return { library: base, notice: null }
  let text: string | null
  try {
    text = storage.getItem(STORAGE_KEY)
  } catch {
    return { library: base, notice: UNREADABLE }
  }
  if (text === null) return { library: base, notice: null }
  let stored: unknown
  try {
    stored = JSON.parse(text)
  } catch {
    return { library: base, notice: UNREADABLE }
  }
  if (!isLibrary(stored)) return { library: base, notice: UNREADABLE }
  return { library: merged(base, stored), notice: null }
}

/** Writes the user's presets, leaving storage untouched when it refuses: a full quota is not fatal. */
export function save(storage: Storage | null, library: Library): void {
  if (!storage) return
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(userPresets(library)))
  } catch {
    // The page keeps working from memory.
  }
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

/** A free `user:{kind}:{8 hex}` id. */
function freshId(kind: Kind, library: Library): string {
  for (;;) {
    const hex = Math.floor(Math.random() * 0x100000000)
      .toString(16)
      .padStart(8, '0')
    const id = `user:${kind}:${hex}`
    if (!find(library, kind, id)) return id
  }
}

/** Copies one preset under a new id, named "<name> (copy)", and returns the copy's id. */
export function duplicate(library: Library, kind: Kind, id: string): { library: Library; id: string } {
  const next = freshId(kind, library)
  if (kind === 'goal') {
    const source = library.goals.find((goal) => goal.id === id)
    if (!source) return { library, id }
    const copy: GoalSpec = { ...clone(source), id: next, name: `${source.name} (copy)` }
    return { library: { ...library, goals: [...library.goals, copy] }, id: next }
  }
  if (kind === 'prices') {
    const source = library.prices.find((prices) => prices.id === id)
    if (!source) return { library, id }
    const copy: PriceSpec = { ...clone(source), id: next, name: `${source.name} (copy)` }
    return { library: { ...library, prices: [...library.prices, copy] }, id: next }
  }
  const source = library.crafts.find((craft) => craft.id === id)
  if (!source) return { library, id }
  const copy: CraftSpec = { ...clone(source), id: next, name: `${source.name} (copy)` }
  return { library: { ...library, crafts: [...library.crafts, copy] }, id: next }
}

/** Removes one preset, refusing a built-in and anything a craft still uses. */
export function remove(library: Library, kind: Kind, id: string): { library: Library } | { error: string } {
  if (isBuiltin(id)) return { error: 'built-in presets cannot be deleted' }
  if (kind === 'craft') {
    const user = library.crafts.find((craft) => craft.id !== id && craft.blocks.some((block) => block.craft === id))
    if (user) return { error: `used by "${user.name}"` }
    return { library: { ...library, crafts: library.crafts.filter((craft) => craft.id !== id) } }
  }
  const user = library.crafts.find((craft) => (kind === 'goal' ? craft.goal === id : craft.prices === id))
  if (user) return { error: `used by "${user.name}"` }
  if (kind === 'goal') return { library: { ...library, goals: library.goals.filter((goal) => goal.id !== id) } }
  return { library: { ...library, prices: library.prices.filter((prices) => prices.id !== id) } }
}

/** The user's presets as a pretty-printed library, for the Export button. */
export function exportJson(library: Library): string {
  return JSON.stringify(userPresets(library), null, 2)
}

/**
 * The library with another library's presets added. An id the library already uses is given a new
 * one, and every reference the imported presets make to it — a craft's goal, its prices, a block's
 * craft — is rewritten to match.
 */
export function importJson(text: string, library: Library): Library {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    throw new Error('not a craft planner library')
  }
  if (!isLibrary(raw)) throw new Error('not a craft planner library')
  const parsed = raw

  // The ids the library already holds, then the ids this import has already claimed.
  const taken = new Set<string>([
    ...library.goals.map((goal) => goal.id),
    ...library.prices.map((prices) => prices.id),
    ...library.crafts.map((craft) => craft.id),
  ])
  const claim = (ids: string[], kind: Kind): Map<string, string> => {
    const renamed = new Map<string, string>()
    for (const id of ids) {
      if (!taken.has(id)) {
        taken.add(id)
        continue
      }
      let next: string
      do {
        const hex = Math.floor(Math.random() * 0x100000000)
          .toString(16)
          .padStart(8, '0')
        next = `user:${kind}:${hex}`
      } while (taken.has(next))
      taken.add(next)
      renamed.set(id, next)
    }
    return renamed
  }

  const goals = claim(parsed.goals.map((goal) => goal.id), 'goal')
  const prices = claim(parsed.prices.map((preset) => preset.id), 'prices')
  const crafts = claim(parsed.crafts.map((craft) => craft.id), 'craft')

  return {
    version: 1,
    goals: [
      ...library.goals,
      ...parsed.goals.map((goal) => ({ ...clone(goal), id: goals.get(goal.id) ?? goal.id })),
    ],
    prices: [
      ...library.prices,
      ...parsed.prices.map((preset) => ({ ...clone(preset), id: prices.get(preset.id) ?? preset.id })),
    ],
    crafts: [
      ...library.crafts,
      ...parsed.crafts.map((craft) => ({
        ...clone(craft),
        id: crafts.get(craft.id) ?? craft.id,
        goal: goals.get(craft.goal) ?? craft.goal,
        prices: prices.get(craft.prices) ?? craft.prices,
        blocks: craft.blocks.map((block) => ({
          ...clone(block),
          craft: block.craft === null ? null : (crafts.get(block.craft) ?? block.craft),
        })),
      })),
    ],
  }
}

/** Every block an entry can reach along the wires, following only ids that name blocks. */
function reachable(spec: CraftSpec): Set<string> {
  const seen = new Set<string>()
  const queue = [spec.entry]
  while (queue.length > 0) {
    const id = queue.pop()
    if (id === undefined || seen.has(id) || !spec.blocks.some((block) => block.id === id)) continue
    seen.add(id)
    for (const wire of spec.wires) if (wire.from === id) queue.push(wire.to)
  }
  return seen
}

/**
 * The names of a craft's outputs: one per wire to `done`, in the craft's wire order, with the
 * second and later uses of a name suffixed `#2`, `#3` and so on, skipping any name already taken
 * (a nested craft's exit can itself be suffixed). Wires from blocks the entry
 * cannot reach are ignored, and a nested craft block's exits are already output names.
 *
 * The library is accepted so callers can pass the one they compile against; naming needs only the
 * spec.
 */
export function outputs(spec: CraftSpec, _library: Library): string[] {
  const reach = reachable(spec)
  const used = new Set<string>()
  const names: string[] = []
  for (const wire of spec.wires) {
    if (wire.to !== 'done' || !reach.has(wire.from)) continue
    let name = wire.exit
    for (let k = 2; used.has(name); k++) name = `${wire.exit}#${k}`
    used.add(name)
    names.push(name)
  }
  return names
}

/** Whether inlining `childId` in `parentId` would make a craft contain itself. */
export function wouldCycle(parentId: string, childId: string, library: Library): boolean {
  const seen = new Set<string>()
  const walk = (id: string): boolean => {
    if (id === parentId) return true
    if (seen.has(id)) return false
    seen.add(id)
    const craft = library.crafts.find((entry) => entry.id === id)
    if (!craft) return false
    return craft.blocks.some((block) => block.craft !== null && walk(block.craft))
  }
  return walk(childId)
}
