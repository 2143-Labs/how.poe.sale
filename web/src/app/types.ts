// The JSON the engine answers with, mirroring `crates/poe1/src/api.rs` field for field.

export type Rarity = 'normal' | 'magic' | 'rare'

/** The item a goal starts from, as `poe1::item::ItemSpec` reads it. */
export interface ItemSpec {
  base: string
  itemLevel: number
  rarity: Rarity
  influences: string[]
}

export type Side = 'prefix' | 'suffix'

/** One modifier the goal asks for: its exact tier, or its family at that tier or higher. */
export interface SelectorSpec {
  side: Side
  id: string
  orBetter: boolean
}

export type Accept = 'has-desired' | 'desired-or-good-both-sides' | 'any'

export interface GoalSpec {
  desired: SelectorSpec[]
  good: SelectorSpec[]
  accept: Accept
}

/** What a currency costs: its price per use, and the trade penalty on a bought one. */
export interface Prices {
  prices: Record<string, number>
  tradePenalty: number
}

/** Everything a loop needs, plus whether to pay for the priced graph. */
export interface LoopRequest {
  item: ItemSpec
  goal: GoalSpec
  prices: Prices
  depth: number
  price: boolean
}

export interface ApplyRequest {
  item: ItemSpec
  currency: string
}

/** What the page saves, shares and sends: the loop request without `price`. */
export interface Setup {
  item: ItemSpec
  goal: GoalSpec
  prices: Prices
  depth: number
}

/** A share of the loop's mass: its exact fraction, and the same as a percent string. */
export interface Mass {
  exact: string
  percent: string
}

export interface CostView {
  key: string
  name: string
  amount: number
}

export interface CurrencyView {
  id: string
  name: string
  costs: CostView[]
}

export interface Start {
  version: string
  patch: string
  bases: string[]
  influences: string[]
  /** `Stage::name()` of every stage, in enum order. */
  stages: string[]
  accepts: string[]
  /** The seven modelled currencies, in `currency::MODELLED` order. */
  currencies: CurrencyView[]
  /** Each cost key's chaos price, from the 3.30 table. */
  priceDefaults: Record<string, number>
}

export interface ModOptionView {
  id: string
  text: string
  side: string
  level: number
  influence: string | null
  source: string
}

export interface StageView {
  stage: string
  mass: Mass
}

export interface DepthView {
  depth: number
  classes: number
  live: Mass
  stages: StageView[]
}

export interface SpendView {
  key: string
  name: string
  count: number
}

export interface PricedView {
  expectedChaos: number
  sdChaos: number
  closure: number
  truncated: number
  spend: SpendView[]
}

export interface LoopView {
  start: { text: string; stage: string }
  depths: DepthView[]
  /** The absorbed stages only: desired, reset and stop, summed over depths. */
  totals: StageView[]
  live: Mass
  stopped: string | null
  priced: PricedView | null
  warnings: string[]
}

export interface OutcomeView {
  chance: Mass
  text: string
  added: string | null
}

export interface ApplyView {
  currency: { id: string; name: string }
  /** The weight total as a decimal string: it can pass JavaScript's safe integer. */
  total: string
  outcomes: OutcomeView[]
  /** How many outcomes the 200-row cap hid. */
  more: number
  /** The hidden outcomes' combined chance: `0/1` when `more` is 0, so `outcomes` plus `rest` is 100 %. */
  rest: Mass
}
