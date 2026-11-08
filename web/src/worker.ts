import init, { Bridge } from './wasm/pkg/solver.js'
import type { InitMessage, SolverRequest, SolverResponse } from './protocol'

let bridge: Promise<Bridge> | null = null
/** The page's data bundle, named by its `init` message. */
let bundleUrl: string | null = null

function load(): Promise<Bridge> {
  bridge ??= (async () => {
    const url = bundleUrl
    if (url === null) throw new Error('the page named no data bundle')
    await init()
    const res = await fetch(url)
    if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`)
    return new Bridge(await res.text())
  })().catch((error: unknown) => {
    // A failed load is not cached: the next request fetches again.
    bridge = null
    throw error
  })
  return bridge
}

function errorText(error: unknown): string {
  // A Rust panic traps the instance; nothing it answers afterwards can be trusted.
  if (error instanceof WebAssembly.RuntimeError) return 'The solver crashed on this input; reload the page.'
  return error instanceof Error ? error.message : String(error)
}

self.onmessage = async (event: MessageEvent) => {
  const message = event.data as SolverRequest | InitMessage
  if (!message || typeof message.type !== 'string') return
  if (message.type === 'init') {
    bundleUrl = message.bundle
    return
  }
  try {
    const b = await load()
    let response: SolverResponse
    switch (message.type) {
      case 'start':
        response = {
          type: 'start',
          id: message.id,
          version: b.version(),
          examples: JSON.parse(b.examples()),
          bases: JSON.parse(b.bases()),
        }
        break
      case 'parse':
        response = { type: 'parse', id: message.id, item: JSON.parse(b.parse(message.text)) }
        break
      case 'mods':
        response = {
          type: 'mods',
          id: message.id,
          mods: JSON.parse(b.mods(message.base, message.itemLevel, message.influences.join(','))),
        }
        break
      case 'chartStart':
        response = {
          type: 'chartStart',
          id: message.id,
          node: JSON.parse(b.chart_start(message.root, message.watch)),
        }
        break
      case 'chartMethods':
        response = {
          type: 'chartMethods',
          id: message.id,
          methods: JSON.parse(b.chart_methods(message.node)),
        }
        break
      case 'chartForward':
        response = {
          type: 'chartForward',
          id: message.id,
          applied: JSON.parse(b.chart_forward(message.node, message.method)),
        }
        break
      case 'chartMadeBy':
        response = {
          type: 'chartMadeBy',
          id: message.id,
          madeBy: JSON.parse(b.chart_made_by(message.node, message.methods.join(','))),
        }
        break
      case 'chartPinForward':
        response = {
          type: 'chartPinForward',
          id: message.id,
          pinned: JSON.parse(b.chart_pin_forward(message.node, message.method, message.key)),
        }
        break
      case 'chartPinBackward':
        response = {
          type: 'chartPinBackward',
          id: message.id,
          pinned: JSON.parse(b.chart_pin_backward(message.node, message.method, message.key)),
        }
        break
    }
    self.postMessage(response)
  } catch (error) {
    self.postMessage({ type: message.type, id: message.id, error: errorText(error) })
  }
}
