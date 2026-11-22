import init, { Bridge } from '../wasm/pkg/solver.js'
import type { AppRequest, AppResponse, InitMessage } from './protocol'

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

/** One request's answer, as JSON. */
async function answer(message: AppRequest): Promise<string> {
  const b = await load()
  switch (message.type) {
    case 'start':
      return b.start()
    case 'mods':
      return b.mods(message.payload.base, message.payload.itemLevel, message.payload.influences.join(','))
    case 'loop':
      return b.loop_report(message.payload)
    case 'apply':
      return b.apply_currency(message.payload)
  }
}

self.onmessage = async (event: MessageEvent) => {
  const message = event.data as AppRequest | InitMessage
  if (!message || typeof message.type !== 'string') return
  if (message.type === 'init') {
    bundleUrl = message.bundle
    return
  }
  try {
    const result = await answer(message)
    self.postMessage({ type: message.type, id: message.id, result } satisfies AppResponse)
  } catch (error) {
    self.postMessage({ type: message.type, id: message.id, error: errorText(error) } satisfies AppResponse)
  }
}
