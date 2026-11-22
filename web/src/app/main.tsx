import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '../styles.css'
import './app.css'
import { App } from './App'
import { createAppClient } from './client'
import { AppContext, errorText } from './context'
import type { InitMessage } from './protocol'
import type { Scope } from './scope'
import { createAppStore } from './store'
import type { Start } from './types'

function storage(): Storage | null {
  try {
    return window.localStorage
  } catch {
    return null
  }
}

/** Starts a game page: its worker, its state, and the app in `#root`. */
export function mount(scope: Scope): void {
  const element = document.getElementById('root')
  if (!element) throw new Error('the page has no #root')
  // Vite finds the worker only when its options are literals, so the bundle URL goes in a message;
  // the worker handles messages in order, so `init` comes before every request.
  const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
  worker.postMessage({ type: 'init', bundle: scope.bundle } satisfies InitMessage)
  const client = createAppClient(worker)

  void (async () => {
    let start: Start | null = null
    let startError: string | null = null
    try {
      start = await client.start()
    } catch (reason) {
      startError = errorText(reason)
    }
    const saved = storage()
    const store = createAppStore(scope, {
      client,
      start,
      startError,
      hash: window.location.hash,
      storage: saved,
      storageNotice: saved ? null : 'Presets cannot be saved in this browser.',
      dropHash: () =>
        window.history.replaceState(null, '', window.location.pathname + window.location.search),
    })
    store.getState().boot()
    createRoot(element).render(
      <StrictMode>
        <AppContext.Provider value={{ scope, client, store }}>
          <App />
        </AppContext.Provider>
      </StrictMode>,
    )
  })()
}
