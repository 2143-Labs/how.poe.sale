import { useState } from 'react'
import { buildShareUrl } from './share'
import { errorText, useApp, useAppState } from './context'
import { ApplyPanel } from './ApplyPanel'
import { LoopPanel } from './LoopPanel'
import { PricesPanel } from './PricesPanel'
import { SetupPanel } from './SetupPanel'

export function App() {
  const { scope, store } = useApp()
  const start = useAppState((state) => state.start)
  const busy = useAppState((state) => state.busy)
  const error = useAppState((state) => state.error)
  const notice = useAppState((state) => state.notice)
  const [copied, setCopied] = useState(false)

  function share() {
    const url = buildShareUrl(scope.id, store.getState().setup, window.location.origin)
    navigator.clipboard
      .writeText(url)
      .then(() => {
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1500)
      })
      .catch((reason: unknown) => store.getState().setError(errorText(reason)))
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>
          <a href="/">how.poe.sale</a>
        </h1>
        <span className="scope-title">{scope.title}</span>
        {start && <span className="version">{start.version}</span>}
        {busy && <span className="busy">Computing…</span>}
        <span className="spacer" />
        <button type="button" onClick={share}>
          {copied ? 'Copied' : 'Share link'}
        </button>
      </header>

      {notice && (
        <div className="banner banner-notice">
          {notice}
          <button type="button" onClick={() => store.getState().dismissNotice()}>
            dismiss
          </button>
        </div>
      )}
      {error && (
        <div className="banner banner-error">
          {error}
          <button type="button" onClick={() => store.getState().dismissError()}>
            dismiss
          </button>
        </div>
      )}

      {start && (
        <main className="columns">
          <div className="stack">
            <SetupPanel />
            <PricesPanel />
          </div>
          <div className="stack">
            <LoopPanel />
            <ApplyPanel />
          </div>
        </main>
      )}
    </div>
  )
}
