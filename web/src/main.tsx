import { StrictMode, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'

const worker = new Worker(new URL('./solver.worker.ts', import.meta.url), { type: 'module' })

function App() {
  const [version, setVersion] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      if (e.data.type === 'version') setVersion(e.data.version)
      else if (e.data.type === 'error') setError(e.data.message)
    }
    worker.addEventListener('message', onMessage)
    worker.postMessage({ type: 'version' })
    return () => worker.removeEventListener('message', onMessage)
  }, [])
  return <h1>how.poe.sale {version ? `v${version}` : (error ?? '…')}</h1>
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
