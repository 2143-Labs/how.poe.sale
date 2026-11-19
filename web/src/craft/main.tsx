import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@xyflow/react/dist/style.css'
import '../styles.css'
import './craft.css'
import { App } from './App'
import { createCraftClient } from './client'
import { CraftContext } from './context'
import { createCraftStore, type CraftStore } from './store'

/** Local storage, or `null` when the browser refuses it: presets then live only in memory. */
function storage(): Storage | null {
  try {
    return window.localStorage
  } catch {
    return null
  }
}

function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}

const root = document.getElementById('root')
if (!root) throw new Error('the page has no #root')
const element = createRoot(root)
const kept = storage()

let store: CraftStore | null = null
const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
const client = createCraftClient(worker, {
  onBusy: (busy) => store?.getState().setBusy(busy),
})

client
  .catalog()
  .then((catalog) => {
    const created = createCraftStore({
      catalog,
      client,
      storage: kept,
      notice: kept ? null : 'Presets cannot be saved in this browser.',
    })
    store = created
    element.render(
      <StrictMode>
        <CraftContext.Provider value={created}>
          <App />
        </CraftContext.Provider>
      </StrictMode>,
    )
  })
  .catch((reason: unknown) => {
    element.render(
      <div className="app">
        <div className="banner banner-error">{errorText(reason)}</div>
      </div>,
    )
  })
