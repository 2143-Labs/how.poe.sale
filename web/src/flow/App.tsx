import { useEffect, useState } from 'react'
import { buildShareUrl } from '../share'
import type { Example } from '../types'
import { Assumptions } from './Assumptions'
import { Chart } from './Chart'
import { errorText, useFlow, useFlowState } from './context'
import { Inspector } from './Inspector'
import { TargetPanel } from './TargetPanel'

export function App() {
  const { scope, client, store } = useFlow()
  const nodes = useFlowState((state) => state.nodes)
  const busy = useFlowState((state) => state.busy)
  const error = useFlowState((state) => state.error)
  const notice = useFlowState((state) => state.notice)

  const [version, setVersion] = useState<string | null>(null)
  const [bases, setBases] = useState<string[]>([])
  const [examples, setExamples] = useState<Example[]>([])
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    let cancelled = false
    client
      .start()
      .then((info) => {
        if (cancelled) return
        setVersion(info.version)
        setBases(info.bases)
        setExamples(info.examples)
      })
      .catch((reason: unknown) => {
        if (!cancelled) store.getState().setError(errorText(reason))
      })
    return () => {
      cancelled = true
    }
  }, [client, store])

  function share() {
    const state = store.getState()
    const url = buildShareUrl(
      scope.id,
      { root: state.root, watch: state.watch, pins: state.pins, merges: state.merges },
      window.location.origin,
    )
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
        {version && <span className="version">{version}</span>}
        {busy && <span className="busy">Computing…</span>}
        <span className="spacer" />
        <button type="button" onClick={share}>
          {copied ? 'Copied' : 'Share link'}
        </button>
        <button type="button" onClick={() => store.getState().clearChart()} disabled={nodes.length < 2}>
          Clear chart
        </button>
      </header>

      <Assumptions />

      {notice && (
        <div className="banner banner-notice">
          {notice}
          <button type="button" onClick={() => store.getState().setNotice(null)}>
            dismiss
          </button>
        </div>
      )}
      {error && (
        <div className="banner banner-error">
          {error}
          <button type="button" onClick={() => store.getState().setError(null)}>
            dismiss
          </button>
        </div>
      )}

      <main className="columns">
        <TargetPanel bases={bases} examples={examples} />
        <div className="stack">
          <Chart />
          <Inspector />
        </div>
      </main>
    </div>
  )
}
