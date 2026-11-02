import init, { Bridge } from './wasm/pkg/solver.js'

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

self.onmessage = async (e: MessageEvent) => {
  try {
    const b = await load()
    if (e.data.type === 'version') self.postMessage({ type: 'version', version: b.version() })
  } catch (err) {
    self.postMessage({ type: 'error', message: String(err) })
  }
}
