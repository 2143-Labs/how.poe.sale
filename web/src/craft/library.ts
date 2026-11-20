// The presets the page keeps: filters, price lists and crafts. Built-ins come from the catalog and
// are merged in on load; only the user's own presets are written to storage.

import type { BaseView, BlockSpec, Builtins, CraftSpec, FilterSpec, PriceSpec } from './types'

export type Kind = 'filter' | 'prices' | 'craft'

/** Every preset the page knows: the built-ins from the catalog, then the user's own. */
export interface Library {
  version: 2
  filters: FilterSpec[]
  prices: PriceSpec[]
  crafts: CraftSpec[]
}

/** Where the user's presets live. */
export const STORAGE_KEY = 'how.poe.sale:craft:library'

/** The preset each list falls back to when its selection goes away. */
export const DEFAULTS: Record<Kind, string> = {
  craft: 'builtin:craft:shield-totem',
  filter: 'builtin:goal:shield',
  prices: 'builtin:prices:330',
}

/** What a storage value that will not read as a library leaves in the banner. */
const UNREADABLE = 'Saved presets could not be read and were left untouched.'

/** A goal as version 1 stored it: the influence lived on the goal, and no block named a filter. */
interface V1Goal {
  id: string
  name: string
  base: string
  influence: string
  prefixes: string[]
  suffixes: string[]
  desired: string[]
  good: string[]
}

/** A craft as version 1 stored it: its goal held the influence. */
interface V1Craft {
  id: string
  name: string
  goal: string
  prices: string
  start: string
  entry: string
  blocks: BlockSpec[]
  wires: CraftSpec['wires']
}

interface V1Library {
  version: 1
  goals: V1Goal[]
  prices: PriceSpec[]
  crafts: V1Craft[]
}

/** Whether a preset is the catalog's, which the page never edits or deletes. */
export function isBuiltin(id: string): boolean {
  return id.startsWith('builtin:')
}

/** The preset type a kind names. */
export type PresetOf<K extends Kind> = K extends 'filter' ? FilterSpec : K extends 'prices' ? PriceSpec : CraftSpec

/** One preset by kind and id, or `undefined` when the library has none. */
export function find<K extends Kind>(library: Library, kind: K, id: string): PresetOf<K> | undefined {
  const list: (FilterSpec | PriceSpec | CraftSpec)[] =
    kind === 'filter' ? library.filters : kind === 'prices' ? library.prices : library.crafts
  return list.find((entry) => entry.id === id) as unknown as PresetOf<K> | undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

const strings = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((entry) => typeof entry === 'string')

function isFilter(value: unknown): value is FilterSpec {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.name === 'string' &&
    typeof value.base === 'string' &&
    strings(value.prefixes) &&
    strings(value.suffixes) &&
    strings(value.desired) &&
    strings(value.good)
  )
}

function isV1Goal(value: unknown): value is V1Goal {
  return isFilter(value) && 'influence' in value && typeof value.influence === 'string'
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
  if (value.filter !== undefined && value.filter !== null && typeof value.filter !== 'string') return false
  if (value.position !== null && !Array.isArray(value.position)) return false
  return true
}

function isWire(value: unknown): boolean {
  return isRecord(value) && typeof value.from === 'string' && typeof value.exit === 'string' && typeof value.to === 'string'
}

/** The fields every craft has had, whatever version stored it. */
function isCraftShape(value: unknown): value is V1Craft {
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
    value.wires.every(isWire)
  )
}

function isCraft(value: unknown): value is CraftSpec {
  return isCraftShape(value) && isRecord(value) && typeof value.influence === 'string'
}

function isLibrary(value: unknown): value is Library {
  return (
    isRecord(value) &&
    value.version === 2 &&
    Array.isArray(value.filters) &&
    value.filters.every(isFilter) &&
    Array.isArray(value.prices) &&
    value.prices.every(isPrices) &&
    Array.isArray(value.crafts) &&
    value.crafts.every(isCraft)
  )
}

function isV1Library(value: unknown): value is V1Library {
  return (
    isRecord(value) &&
    value.version === 1 &&
    Array.isArray(value.goals) &&
    value.goals.every(isV1Goal) &&
    Array.isArray(value.prices) &&
    value.prices.every(isPrices) &&
    Array.isArray(value.crafts) &&
    value.crafts.every(isCraftShape)
  )
}

/** The first influence the catalog offers a base, or `none` when it offers none or is unknown. */
function firstInfluence(bases: BaseView[], key: string): string {
  return bases.find((base) => base.key === key)?.influences[0] ?? 'none'
}

/**
 * A version 1 library as version 2: its goals become filters without an influence, and each craft
 * takes its influence from its goal — the user's own goal from the payload; for a built-in goal,
 * the influence of the built-in craft built on it (the Shaper's orb goal had none), else the first
 * influence the catalog offers the base that filter is for. A block's filter starts as `null`, so
 * it reads the craft's goal.
 */
function migrated(raw: V1Library, known: FilterSpec[], knownCrafts: CraftSpec[], bases: BaseView[]): Library {
  const influenceOfGoal = new Map(raw.goals.map((goal) => [goal.id, goal.influence]))
  const baseOfBuiltin = new Map(known.map((filter) => [filter.id, filter.base]))
  const influenceOf = (goal: string): string => {
    const stored = influenceOfGoal.get(goal)
    if (stored !== undefined) return stored
    const builtin = knownCrafts.find((craft) => isBuiltin(craft.id) && craft.goal === goal)
    if (builtin) return builtin.influence
    const base = baseOfBuiltin.get(goal)
    return base === undefined ? 'none' : firstInfluence(bases, base)
  }
  const filters: FilterSpec[] = raw.goals.map((goal) => ({
    id: goal.id,
    name: goal.name,
    base: goal.base,
    prefixes: goal.prefixes,
    suffixes: goal.suffixes,
    desired: goal.desired,
    good: goal.good,
  }))
  const crafts: CraftSpec[] = raw.crafts.map((craft) => ({
    ...craft,
    influence: influenceOf(craft.goal),
    blocks: craft.blocks.map((block) => ({ ...block, filter: block.filter ?? null })),
  }))
  return { version: 2, filters, prices: raw.prices, crafts }
}

/** The built-ins alone, which is what a page opens on when nothing can be read. */
function builtinLibrary(builtins: Builtins): Library {
  return {
    version: 2,
    filters: [...builtins.filters],
    prices: [...builtins.prices],
    crafts: [...builtins.crafts],
  }
}

/** The user's own presets, which are the only ones stored and exported. */
function userPresets(library: Library): Library {
  return {
    version: 2,
    filters: library.filters.filter((filter) => !isBuiltin(filter.id)),
    prices: library.prices.filter((prices) => !isBuiltin(prices.id)),
    crafts: library.crafts.filter((craft) => !isBuiltin(craft.id)),
  }
}

/** Built-ins first, then the stored presets whose ids the built-ins do not already use. */
function merged(builtins: Library, stored: Library): Library {
  const seen = new Set<string>([...builtins.filters, ...builtins.prices, ...builtins.crafts].map((entry) => entry.id))
  const fresh = <T extends { id: string }>(entries: T[]): T[] =>
    entries.filter((entry) => {
      if (seen.has(entry.id)) return false
      seen.add(entry.id)
      return true
    })
  return {
    version: 2,
    filters: [...builtins.filters, ...fresh(stored.filters)],
    prices: [...builtins.prices, ...fresh(stored.prices)],
    crafts: [...builtins.crafts, ...fresh(stored.crafts)],
  }
}

/**
 * The library a page opens with: the catalog's built-ins, plus whatever storage holds. A version 1
 * value is migrated to version 2 and written back only on the next edit. A value that will not
 * read as a library is reported and left where it is, so the next edit is the first thing that
 * overwrites it.
 */
export function load(
  storage: Storage | null,
  builtins: Builtins,
  bases: BaseView[],
): { library: Library; notice: string | null } {
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
  if (isLibrary(stored)) return { library: merged(base, stored), notice: null }
  if (isV1Library(stored)) {
    return { library: merged(base, migrated(stored, builtins.filters, builtins.crafts, bases)), notice: null }
  }
  return { library: base, notice: UNREADABLE }
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
  if (kind === 'filter') {
    const source = library.filters.find((filter) => filter.id === id)
    if (!source) return { library, id }
    const copy: FilterSpec = { ...clone(source), id: next, name: `${source.name} (copy)` }
    return { library: { ...library, filters: [...library.filters, copy] }, id: next }
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

/** Removes one preset, refusing a built-in and anything a craft still uses as its goal, a block's filter, or its prices. */
export function remove(library: Library, kind: Kind, id: string): { library: Library } | { error: string } {
  if (isBuiltin(id)) return { error: 'built-in presets cannot be deleted' }
  if (kind === 'craft') {
    const user = library.crafts.find((craft) => craft.id !== id && craft.blocks.some((block) => block.craft === id))
    if (user) return { error: `used by "${user.name}"` }
    return { library: { ...library, crafts: library.crafts.filter((craft) => craft.id !== id) } }
  }
  const user = library.crafts.find((craft) =>
    kind === 'filter'
      ? craft.goal === id || craft.blocks.some((block) => block.filter === id)
      : craft.prices === id,
  )
  if (user) return { error: `used by "${user.name}"` }
  if (kind === 'filter') return { library: { ...library, filters: library.filters.filter((filter) => filter.id !== id) } }
  return { library: { ...library, prices: library.prices.filter((prices) => prices.id !== id) } }
}

/** The user's presets as a pretty-printed library, for the Export button. */
export function exportJson(library: Library): string {
  return JSON.stringify(userPresets(library), null, 2)
}

/**
 * The library with another library's presets added. A version 1 library is migrated first. An id
 * the library already uses is given a new one, and every reference the imported presets make to
 * it — a craft's goal and prices, a block's craft and filter — is rewritten to match.
 */
export function importJson(text: string, library: Library, bases: BaseView[]): Library {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    throw new Error('not a craft planner library')
  }
  let parsed: Library
  if (isLibrary(raw)) parsed = raw
  else if (isV1Library(raw)) parsed = migrated(raw, library.filters, library.crafts, bases)
  else throw new Error('not a craft planner library')

  // The ids the library already holds, then the ids this import has already claimed.
  const taken = new Set<string>([
    ...library.filters.map((filter) => filter.id),
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

  const filters = claim(parsed.filters.map((filter) => filter.id), 'filter')
  const prices = claim(parsed.prices.map((preset) => preset.id), 'prices')
  const crafts = claim(parsed.crafts.map((craft) => craft.id), 'craft')

  return {
    version: 2,
    filters: [
      ...library.filters,
      ...parsed.filters.map((filter) => ({ ...clone(filter), id: filters.get(filter.id) ?? filter.id })),
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
        goal: filters.get(craft.goal) ?? craft.goal,
        prices: prices.get(craft.prices) ?? craft.prices,
        blocks: craft.blocks.map((block) => ({
          ...clone(block),
          craft: block.craft === null ? null : (crafts.get(block.craft) ?? block.craft),
          filter: block.filter === undefined ? undefined : block.filter === null ? null : (filters.get(block.filter) ?? block.filter),
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
