/// <reference types="node" />
// The app's requests against the real engine, in this process: `npm run build` stages the wasm
// and both bundles, so this reads them the way the deleted `bridge.ts` did.
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import type { ApplyView, LoopRequest, LoopView, ModOptionView, SelectorSpec, Start } from '../app/types'
import init, { Bridge } from '../wasm/pkg/solver.js'

/** The wasm module once loaded: a second bridge reuses it and starts its own engine. */
let loaded: Promise<unknown> | null = null

async function bridge(): Promise<Bridge> {
  loaded ??= init({ module_or_path: readFileSync(new URL('../wasm/pkg/solver_bg.wasm', import.meta.url)) })
  await loaded
  return new Bridge(readFileSync(new URL('../../public/data/bundle.json', import.meta.url), 'utf8'))
}

const SHIELD = { base: 'Supreme Spiked Shield', itemLevel: 87, rarity: 'normal' as const, influences: ['shaper'] }

/** The goal the fixture's desired and good classes name, as the page builds it from its mod rows. */
function totemGoal(): LoopRequest['goal'] {
  const pattern = JSON.parse(
    readFileSync(new URL('../../../fixtures/patterns/totem-shield.json', import.meta.url), 'utf8'),
  ) as {
    classes: { side: 'prefix' | 'suffix'; grade: string; matchers: { mod?: string }[] }[]
  }
  const selectors = (grade: string): SelectorSpec[] =>
    pattern.classes
      .filter((entry) => entry.grade === grade)
      .flatMap((entry) =>
        entry.matchers.flatMap((matcher) =>
          matcher.mod ? [{ side: entry.side, id: matcher.mod, orBetter: false } satisfies SelectorSpec] : [],
        ),
      )
  return { desired: selectors('desired'), good: selectors('good'), accept: 'desired-or-good-both-sides' }
}

describe('the app engine', () => {
  it('lists bases, seven currencies and default prices', async () => {
    const b = await bridge()
    const start = JSON.parse(b.start()) as Start
    expect(start.bases.length).toBeGreaterThanOrEqual(3)
    expect(start.currencies).toHaveLength(7)
    expect(Object.keys(start.priceDefaults).length).toBeGreaterThan(0)
    expect(start.stages).toContain('desired')
    const mods = JSON.parse(b.mods('Supreme Spiked Shield', 87, 'shaper')) as ModOptionView[]
    expect(mods.length).toBeGreaterThan(0)
  })

  it('puts the totem shield loop at 1/108 for the fixture goal', { timeout: 60_000 }, async () => {
    const b = await bridge()
    const start = JSON.parse(b.start()) as Start
    const request: LoopRequest = {
      item: SHIELD,
      goal: totemGoal(),
      prices: { prices: start.priceDefaults, tradePenalty: 0.02 },
      depth: 1,
      price: false,
    }
    const loop = JSON.parse(b.loop_report(JSON.stringify(request))) as LoopView
    expect(loop.depths).toHaveLength(1)
    const desired = loop.depths[0].stages.find((stage) => stage.stage === 'desired')
    expect(desired?.mass.exact).toBe('1/108')
    expect(desired?.mass.percent).toBe('0.925926')
  })

  it('applies transmutation to the white shield', async () => {
    const b = await bridge()
    const applied = JSON.parse(
      b.apply_currency(JSON.stringify({ item: SHIELD, currency: 'currency/transmutation' })),
    ) as ApplyView
    expect(applied.total).not.toBe('0')
    expect(applied.outcomes.length).toBeGreaterThan(0)
    // The view caps its rows at 200 and folds the rest into `rest`: shown plus hidden is exactly 100 %.
    const sum = applied.outcomes.reduce((total, outcome) => total + Number.parseFloat(outcome.chance.percent), 0)
    expect(Math.abs(sum + Number.parseFloat(applied.rest.percent) - 100)).toBeLessThan(1e-4)
  })

  it('rejects an unknown currency', async () => {
    const b = await bridge()
    let caught: unknown = null
    try {
      b.apply_currency(JSON.stringify({ item: SHIELD, currency: 'currency/nope' }))
    } catch (error) {
      caught = error
    }
    expect(caught).not.toBeNull()
    expect(String(caught instanceof Error ? caught.message : caught)).toMatch(/unknown currency/)
  })
})
