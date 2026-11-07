import { useEffect, useState } from 'react'
import { buildShareUrl } from '../share'
import { nodeId, type Example } from '../types'
import { Chart } from './Chart'
import { errorText, useFlow, useFlowState } from './context'
import { Inspector } from './Inspector'
import { TargetPanel } from './TargetPanel'

export function App() {
  const { scope, client, store } = useFlow()
  const spec = useFlowState((state) => state.spec)
  const chart = useFlowState((state) => state.chart)
  const selected = useFlowState((state) => state.selected)
  const preimagesFor = useFlowState((state) => state.preimagesFor)
  const busy = useFlowState((state) => state.busy)
  const error = useFlowState((state) => state.error)
  const notice = useFlowState((state) => state.notice)

  const [version, setVersion] = useState<string | null>(null)
  const [bases, setBases] = useState<string[]>([])
  const [examples, setExamples] = useState<Example[]>([])
  const [copied, setCopied] = useState(false)
  // A new target is being computed; the chart is about to start over from it.
  const [rootPending, setRootPending] = useState(false)

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

  // A target the chart is not built from yet: ask what could have produced it, and start the
  // chart over from it once the engine accepts it. A refused target leaves the chart as it is.
  useEffect(() => {
    if (store.getState().chart[0]?.id === nodeId(spec)) {
      setRootPending(false)
      store.getState().setError(null)
      return
    }
    setRootPending(true)
    let cancelled = false
    const timer = setTimeout(() => {
      client
        .preimages(spec, scope.methods)
        .then((result) => {
          if (cancelled) return
          store.getState().accept(spec, result)
          setRootPending(false)
        })
        .catch((reason: unknown) => {
          if (cancelled) return
          store.getState().setError(errorText(reason))
          setRootPending(false)
        })
    }, 250)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [client, scope, store, spec])

  // The selected node's own steps.
  useEffect(() => {
    if (!selected || selected === preimagesFor || rootPending) return
    const node = chart.find((candidate) => candidate.id === selected)
    if (!node) return
    let cancelled = false
    client
      .preimages(node.spec, scope.methods)
      .then((result) => {
        if (!cancelled) store.getState().setPreimages(selected, result)
      })
      .catch((reason: unknown) => {
        if (!cancelled) store.getState().setError(errorText(reason))
      })
    return () => {
      cancelled = true
    }
  }, [client, scope, store, chart, selected, preimagesFor, rootPending])

  function share() {
    const url = buildShareUrl(scope.id, spec, window.location.origin)
    const shown = () => {
      setCopied(true)
      window.setTimeout(() => setCopied(false), 2000)
    }
    if (navigator.clipboard) {
      navigator.clipboard.writeText(url).then(shown, () => store.getState().setNotice(`Share link: ${url}`))
    } else {
      store.getState().setNotice(`Share link: ${url}`)
    }
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>
          <a href="/">how.poe.sale</a>
        </h1>
        <span className="scope-title">{scope.title}</span>
        <span className="version">{version ? `v${version}` : 'starting…'}</span>
        {busy && <span className="busy">computing…</span>}
        <span className="spacer" />
        <button type="button" onClick={share}>
          {copied ? 'Link copied' : 'Share link'}
        </button>
        <button type="button" onClick={() => store.getState().clearChart()} disabled={chart.length < 2}>
          Clear chart
        </button>
      </header>

      {notice && (
        <div className="banner banner-notice">
          <span>{notice}</span>
          <button type="button" onClick={() => store.getState().setNotice(null)}>
            dismiss
          </button>
        </div>
      )}
      {error && (
        <div className="banner banner-error">
          <span>{error}</span>
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
