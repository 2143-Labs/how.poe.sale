import init, { Bridge } from './wasm/pkg/solver.js'
import type { SolverRequest, SolverResponse } from './protocol'

let bridge: Promise<Bridge> | null = null

function load(): Promise<Bridge> {
  bridge ??= (async () => {
    await init()
    const res = await fetch('/data/bundle.json')
    if (!res.ok) throw new Error(`bundle.json: HTTP ${res.status}`)
    return new Bridge(await res.text())
  })()
  return bridge
}

function parseJson<T>(text: string): T {
  return JSON.parse(text) as T
}

self.onmessage = async (event: MessageEvent) => {
  const message = event.data as SolverRequest
  if (!message || typeof message.type !== 'string') return
  try {
    const b = await load()
    let response: SolverResponse
    switch (message.type) {
      case 'version':
        response = { type: 'version', id: message.id, version: b.version() }
        break
      case 'solve':
        response = { type: 'solve', id: message.id, solution: parseJson(b.solve(JSON.stringify(message.problem))) }
        break
      case 'parse':
        response = { type: 'parse', id: message.id, draft: parseJson(b.parse_target(message.text)) }
        break
      case 'influenceMods':
        response = {
          type: 'influenceMods',
          id: message.id,
          mods: parseJson(b.influence_mods(message.base, message.influence)),
        }
        break
    }
    self.postMessage(response)
  } catch (error) {
    self.postMessage({
      type: message.type,
      id: message.id,
      error: error instanceof Error ? error.message : String(error),
    })
  }
}
