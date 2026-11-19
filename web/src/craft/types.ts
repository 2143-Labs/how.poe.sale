// The JSON the craft engine answers with, mirroring `crates/simple/src/api.rs` field for field,
// and the specs the page sends back, mirroring `crates/simple/src/craft/spec.rs`.

/** One way out of a method, and the rarity an item leaves with. */
export interface ExitSpecView {
  name: string
  rarity: string
}

/** One parameter a block sets, with the values it accepts. */
export interface ParamView {
  name: string
  /** `int`, `bool` or `essence`. */
  type: 'int' | 'bool' | 'essence'
  min?: number
  max?: number
  default?: number | boolean | string
}

/** One method, as the page offers it: what it takes, what it leaves through and what it costs extra. */
export interface MethodView {
  kind: string
  label: string
  takes: string[]
  exits: ExitSpecView[]
  params: ParamView[]
  /** The extra currency's id, or `null` when the method needs none. */
  needs: string | null
}

/** One currency a craft can pay with. */
export interface CurrencyView {
  id: string
  name: string
  /** Whether the price editor shows it: the bench is bought by option, not by orb. */
  priced: boolean
}

/** One essence a method can be told to use. */
export interface EssenceView {
  /** The `essence` parameter's value. */
  id: string
  /** The id the use is costed and priced under. */
  currency: string
  label: string
  /** The export id of the modifier the essence places. */
  modifier: string
  /** The base key the essence's modifier is for. */
  base: string
}

/** The goal a run is for: a base, the grade floors each side has to clear, and the modifiers each grade is made of. */
export interface GoalSpec {
  id: string
  name: string
  base: string
  /** `"none"` or one of the base's offered influences. */
  influence: string
  /** Grade names, best first: the prefix side's floors. */
  prefixes: string[]
  /** Grade names, best first: the suffix side's floors. */
  suffixes: string[]
  /** The modifier ids the goal calls desired. */
  desired: string[]
  /** The modifier ids the goal calls good. */
  good: string[]
}

/** What one orb is worth: the unit the numbers are in, and the weight of every id it names. */
export interface PriceSpec {
  id: string
  name: string
  unit: string
  weights: Record<string, number>
}

/** One block of a craft: a method with its parameters, or a saved craft used as one block. */
export interface BlockSpec {
  id: string
  method: MethodSpec | null
  /** The id of the craft this block inlines, when it is a craft block. */
  craft: string | null
  /** Where the page drew the block, kept for the next visit. */
  position: [number, number] | null
}

/** The method a block names, with only the parameters that method reads. */
export interface MethodSpec {
  kind: string
  /** `exalt_to`: how many modifiers the item holds when it leaves. */
  modifiers: number | null
  /** `alt_to_mod`: whether an augmentation fills an empty side the goal wants a desired modifier on. */
  augment: boolean | null
  /** `essence_spam`: the essence to use, by its short id (`essence-of-horror`). */
  essence: string | null
}

/** One wire: the exit of a block, and where an item leaving through it goes. */
export interface WireSpec {
  from: string
  exit: string
  /** A block id, or `done` for the finished item. */
  to: string
}

/** A craft as the page stores it: blocks wired exit to block, with the goal and prices it runs under. */
export interface CraftSpec {
  id: string
  name: string
  /** The id of the goal the craft is built for. */
  goal: string
  /** The id of the price list the craft is costed at. */
  prices: string
  /** The rarity the craft starts from: `normal`, `magic` or `rare`. */
  start: string
  /** The id of the block the craft starts at. */
  entry: string
  blocks: BlockSpec[]
  wires: WireSpec[]
}

/** How many modifiers a side of a base may hold at each cap. */
export interface CapsView {
  magic: number
  rare: number
}

/** One base, its caps, and the goal its data file states. */
export interface BaseView {
  key: string
  label: string
  itemLevel: number
  itemClass: string
  influences: string[]
  caps: CapsView
  goal: GoalSpec
}

/** One explicit modifier of a base's pool. */
export interface ModView {
  id: string
  /** `prefix` or `suffix`. */
  side: string
  name: string
  text: string
  level: number
  /** The modifier's first group, or empty when it lists none. */
  group: string
  /** Its weight on the base with no influence. */
  weightPlain: number
  /** Its weight under the base's offered influence, or `null` when the base offers none. */
  weightInfluenced: number | null
  /** Whether the modifier rolls nowhere and an essence places it instead. */
  essenceOnly: boolean
}

/** The presets a page starts from: one goal per base and one more, two price lists, eight crafts. */
export interface Builtins {
  goals: GoalSpec[]
  prices: PriceSpec[]
  crafts: CraftSpec[]
}

/** Everything the page needs to draw itself. */
export interface Catalog {
  bases: BaseView[]
  methods: MethodView[]
  currencies: CurrencyView[]
  essences: EssenceView[]
  grades: string[]
  builtins: Builtins
}

/** One run: the goal it is for, the prices its orbs are weighed at, the craft to cost, and the library it nests. */
export interface RunRequest {
  goal: GoalSpec
  prices: PriceSpec
  craft: CraftSpec
  library: CraftSpec[]
}

/** One orb's mean count and spread, and its weight when the price list names it. */
export interface OrbView {
  id: string
  name: string
  mean: number
  sd: number
  price: number | null
}

/** One thing paid for that has no weight. */
export interface UnweightedView {
  id: string
  name: string
  count: number
}

/** A spend as one weighted number and the orb counts behind it. */
export interface SpendView {
  value: number
  sd: number
  orbs: OrbView[]
  unweighted: UnweightedView[]
}

/** One exit of a step, and where it goes. */
export interface ExitView {
  exit: string
  /** `"step N"` or `"done"`. */
  to: string
  share: number
}

/** One step of a craft, costed. */
export interface StepView {
  step: number
  /** The block's path in the spec, e.g. `b2/b1` for a step inside a nested craft. */
  block: string
  name: string
  takes: string[]
  /** The share of this step's runs that leave through a wire. */
  chance: number
  /** `1 / chance`, or `null` when nothing leaves. */
  tries: number | null
  exits: ExitView[]
  /** What one run of the step spends. */
  run: SpendView
  /** What it takes to get past the step, failed attempts included. */
  reach: SpendView
}

/** One way out of the craft. */
export interface OutputView {
  step: number
  exit: string
  share: number
}

/** One other move a keeps box allows, and what it costs to finish from there. */
export interface OptionView {
  id: string
  name: string
  cost: number
}

/** One box of a method's diagram. */
export interface BoxView {
  id: number
  /** `forgets`, `keeps` or `exit`. */
  kind: 'forgets' | 'keeps' | 'exit'
  /** The box's rarity, or `null` for an exit. */
  rarity: string | null
  /** The prefixes held, best first. `null` means any grades; a normal item has `[]`. */
  prefixes: string[] | null
  /** The suffixes held, best first. `null` means any grades; a normal item has `[]`. */
  suffixes: string[] | null
  /** The currency the strategy uses here. */
  move: string | null
  /** What the move costs to finish from here, when it is priced. */
  cost: number | null
  /** Every other move the item allows, cheapest first. */
  options: OptionView[]
  /** The exit's name, for an exit box. */
  exit: string | null
  /** Where the exit goes: `"step N"` or `"done"`. */
  to: string | null
  visits: number
}

/** One edge of a method's diagram: the share of `from`'s visits that go to `to`. */
export interface EdgeView {
  from: number
  to: number
  share: number
}

/** One step's method as a graph. */
export interface DiagramView {
  step: number
  boxes: BoxView[]
  edges: EdgeView[]
}

/** One other move the craft could make at a place it passes through. */
export interface OtherView {
  step: number
  /** The place, as a node's label. */
  item: string
  visits: number
  /** The move the craft makes there. */
  planned: string
  plannedCost: number
  /** The other move. */
  other: string
  otherCost: number
  /** Whether the other move is cheaper by the weights. */
  better: boolean
}

/** What the other-moves pass checked, and what it had to leave out. */
export interface CheckedView {
  moves: number
  places: number
  skipped: number
}

/** One run's report: what the craft costs, step by step, with every way out and every other move. */
export interface RunView {
  /** The craft's name. */
  craft: string
  /** The unit the weighted numbers are in. */
  unit: string
  /** The law's warning, then the compile's. */
  warnings: string[]
  total: SpendView
  steps: StepView[]
  /** Every exit wired out of the craft. */
  outputs: OutputView[]
  /** One diagram per step. */
  diagrams: DiagramView[]
  /** Every other move the craft could make at a place it passes through. */
  others: OtherView[]
  checked: CheckedView
}
