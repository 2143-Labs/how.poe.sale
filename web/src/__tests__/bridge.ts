/// <reference types="node" />
// A `Client` that calls the engine in this process, with the arguments the worker passes it.
import { readFileSync } from 'node:fs'
import type { Client } from '../client'
import { SCOPES, type Scope } from '../flow/scope'
import type { AppliedView, MadeByView, MethodOption, NodeView, PinnedView } from '../types'
import init, { Bridge } from '../wasm/pkg/solver.js'

/** The wasm module once loaded: a second client reuses it and starts its own engine. */
let loaded: Promise<unknown> | null = null

/** The engine over one page's bundle; the wasm and both bundles are staged by `npm run build`. */
export async function clientFor(scope: Scope['id']): Promise<Client> {
  loaded ??= init({ module_or_path: readFileSync(new URL('../wasm/pkg/solver_bg.wasm', import.meta.url)) })
  await loaded
  const bundle = new URL(`../../public${SCOPES[scope].bundle}`, import.meta.url)
  const bridge = new Bridge(readFileSync(bundle, 'utf8'))
  return {
    start: async () => ({
      version: bridge.version(),
      examples: JSON.parse(bridge.examples()),
      bases: JSON.parse(bridge.bases()),
    }),
    parse: async (text) => JSON.parse(bridge.parse(text)),
    mods: async (base, itemLevel, influences) => JSON.parse(bridge.mods(base, itemLevel, influences.join(','))),
    chartStart: async (root, watch): Promise<NodeView> =>
      JSON.parse(bridge.chart_start(JSON.stringify(root), JSON.stringify(watch))),
    chartMethods: async (node): Promise<MethodOption[]> => JSON.parse(bridge.chart_methods(node)),
    chartForward: async (node, method): Promise<AppliedView> => JSON.parse(bridge.chart_forward(node, method)),
    chartMadeBy: async (node, methods): Promise<MadeByView> =>
      JSON.parse(bridge.chart_made_by(node, methods.join(','))),
    chartPinForward: async (node, method, key): Promise<PinnedView> =>
      JSON.parse(bridge.chart_pin_forward(node, method, key)),
    chartPinBackward: async (node, method, key): Promise<PinnedView> =>
      JSON.parse(bridge.chart_pin_backward(node, method, key)),
    dispose: () => bridge.free(),
  }
}

/** The engine over the simple game's bundle. */
export function simpleClient(): Promise<Client> {
  return clientFor('simple')
}
