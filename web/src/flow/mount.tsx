import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@xyflow/react/dist/style.css'
import '../styles.css'
import { createClient } from '../client'
import { App } from './App'
import { FlowContext } from './context'
import type { Scope } from './scope'
import { createFlowStore, type FlowStore } from './store'

function storage(): Storage | null {
  try {
    return window.localStorage
  } catch {
    return null
  }
}

/** Starts a chart page: its worker, its state, and the app in `#root`. */
export function mount(scope: Scope): void {
  let store: FlowStore | null = null
  const client = createClient(new Worker(new URL('../worker.ts', import.meta.url), { type: 'module' }), {
    onBusy: (busy) => store?.getState().setBusy(busy),
  })
  store = createFlowStore(scope, {
    client,
    hash: window.location.hash,
    storage: storage(),
    dropHash: () => window.history.replaceState(null, '', window.location.pathname + window.location.search),
  })
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <FlowContext.Provider value={{ scope, client, store }}>
        <App />
      </FlowContext.Provider>
    </StrictMode>,
  )
}
