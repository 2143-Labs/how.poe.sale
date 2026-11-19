import init, { CraftBridge } from '../wasm/pkg/solver.js'
import type { CraftRequest, CraftResponse } from './protocol'

let bridge: Promise<CraftBridge> | null = null

function load(): Promise<CraftBridge> {
  bridge ??= (async () => {
    await init()
    return new CraftBridge()
  })().catch((error: unknown) => {
    // A failed load is not cached: the next request tries again.
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
  const message = event.data as CraftRequest | null | undefined
  if (!message || typeof message.type !== 'string') return
  try {
    const b = await load()
    let response: CraftResponse
    switch (message.type) {
      case 'catalog':
        response = { type: 'catalog', id: message.id, catalog: JSON.parse(b.catalog()) }
        break
      case 'mods':
        response = { type: 'mods', id: message.id, mods: JSON.parse(b.mods(message.base)) }
        break
      case 'run':
        response = { type: 'run', id: message.id, result: JSON.parse(b.run(message.request)) }
        break
    }
    self.postMessage(response)
  } catch (error) {
    self.postMessage({ type: message.type, id: message.id, error: errorText(error) })
  }
}
