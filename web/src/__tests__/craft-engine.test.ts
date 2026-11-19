/// <reference types="node" />
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import type { Catalog, CraftSpec, RunRequest, RunView } from '../craft/types'
import init, { CraftBridge } from '../wasm/pkg/solver.js'

let loaded: Promise<unknown> | null = null

/** The engine over the wasm `npm run stage` put in `src/wasm/pkg`. */
async function engine(): Promise<CraftBridge> {
  loaded ??= init({ module_or_path: readFileSync(new URL('../wasm/pkg/solver_bg.wasm', import.meta.url)) })
  await loaded
  return new CraftBridge()
}

function catalogOf(bridge: CraftBridge): Catalog {
  return JSON.parse(bridge.catalog()) as Catalog
}

/** The request the page sends: the craft under its own goal and prices, with the built-in library. */
function request(catalog: Catalog, craft: CraftSpec): RunRequest {
  const goal = catalog.builtins.goals.find((entry) => entry.id === craft.goal)
  const prices = catalog.builtins.prices.find((entry) => entry.id === craft.prices)
  if (!goal || !prices) throw new Error(`the catalog has no goal or prices for ${craft.id}`)
  return { goal, prices, craft, library: catalog.builtins.crafts }
}

function builtin(catalog: Catalog, id: string): CraftSpec {
  const craft = catalog.builtins.crafts.find((entry) => entry.id === id)
  if (!craft) throw new Error(`the catalog has no ${id}`)
  return craft
}

describe('the craft engine in the browser', () => {
  it('lists the catalog the page builds itself from', async () => {
    const catalog = catalogOf(await engine())
    expect(catalog.bases).toHaveLength(6)
    expect(catalog.methods).toHaveLength(10)
    expect(catalog.builtins.crafts).toHaveLength(8)
    expect(catalog.grades).toEqual(['desired', 'good', 'any'])
  })

  it('costs the flask at the built-in prices', async () => {
    const bridge = await engine()
    const catalog = catalogOf(bridge)
    const view = JSON.parse(bridge.run(JSON.stringify(request(catalog, builtin(catalog, 'builtin:craft:flask'))))) as RunView
    expect(view.total.value).toBeCloseTo(14.7555625, 9)
  })

  it('refuses a craft that contains itself', async () => {
    const bridge = await engine()
    const catalog = catalogOf(bridge)
    const flask = builtin(catalog, 'builtin:craft:flask')
    const self: CraftSpec = {
      ...flask,
      id: 'x',
      name: 'x',
      blocks: [{ id: 'b1', method: null, craft: 'x', position: null }],
      wires: [],
    }
    const body = request(catalog, self)
    body.library = [...body.library, self]
    let message: string | null = null
    try {
      bridge.run(JSON.stringify(body))
    } catch (error) {
      message = String(error)
    }
    expect(message).toContain('contains itself')
  })
})
