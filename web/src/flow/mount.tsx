import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@xyflow/react/dist/style.css'
import '../styles.css'
import { createClient } from '../client'
import { App } from './App'
import { FlowContext } from './context'
import type { Scope } from './scope'
import { createFlowStore } from './store'

function storage(): Storage | null {
  try {
    return window.localStorage
  } catch {
    return null
  }
}

/** Starts a flowchart page: its state, its worker, and the app in `#root`. */
export function mount(scope: Scope): void {
  const store = createFlowStore(scope, {
    hash: window.location.hash,
    storage: storage(),
    dropHash: () => window.history.replaceState(null, '', window.location.pathname + window.location.search),
  })
  const client = createClient(new Worker(new URL('../worker.ts', import.meta.url), { type: 'module' }), {
    onBusy: (busy) => store.getState().setBusy(busy),
  })
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <FlowContext.Provider value={{ scope, client, store }}>
        <App />
      </FlowContext.Provider>
    </StrictMode>,
  )
}
