import init, { Bridge } from './wasm/pkg/solver.js'
import type { SolverRequest, SolverResponse } from './protocol'

let bridge: Promise<Bridge> | null = null

function load(): Promise<Bridge> {
  bridge ??= (async () => {
    await init()
    const res = await fetch('/data/bundle.json')
    if (!res.ok) throw new Error(`bundle.json: HTTP ${res.status}`)
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
  const message = event.data as SolverRequest
  if (!message || typeof message.type !== 'string') return
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
      case 'preimages':
        response = {
          type: 'preimages',
          id: message.id,
          result: JSON.parse(b.preimages(message.target, message.methods.join(','))),
        }
        break
    }
    self.postMessage(response)
  } catch (error) {
    self.postMessage({ type: message.type, id: message.id, error: errorText(error) })
  }
}
