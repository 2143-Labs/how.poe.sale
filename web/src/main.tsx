import { StrictMode, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import './styles.css'
import { ExamplesMenu } from './components/ExamplesMenu'
import { ItemPanel } from './components/ItemPanel'
import { NowLine } from './components/NowLine'
import { SourcesPanel } from './components/SourcesPanel'
import { StepsTable } from './components/StepsTable'
import { TargetPanel } from './components/TargetPanel'
import { UnmetList } from './components/UnmetList'
import { buildShareUrl } from './share'
import { solver } from './solver'
import { useAppStore } from './store'
import type { Example, PatternSpec } from './types'

function App() {
  const patternText = useAppStore((state) => state.patternText)
  const itemText = useAppStore((state) => state.itemText)
  const result = useAppStore((state) => state.result)
  const busy = useAppStore((state) => state.busy)
  const error = useAppStore((state) => state.error)
  const notice = useAppStore((state) => state.notice)
  const setError = useAppStore((state) => state.setError)
  const setNotice = useAppStore((state) => state.setNotice)

  const [version, setVersion] = useState<string | null>(null)
  const [patterns, setPatterns] = useState<PatternSpec[]>([])
  const [examples, setExamples] = useState<Example[]>([])
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    let cancelled = false
    solver
      .start()
      .then((info) => {
        if (cancelled) return
        setVersion(info.version)
        setPatterns(info.patterns)
        setExamples(info.examples)
        const state = useAppStore.getState()
        if (!state.patternText && !state.itemText && info.patterns[0]) {
          state.setPatternText(JSON.stringify(info.patterns[0], null, 2))
          state.setItemText(info.examples.find((example) => example.id === 'archdemon-crown')?.text ?? '')
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason))
      })
    return () => {
      cancelled = true
    }
  }, [setError])

  useEffect(() => {
    const { setParsed, setParseError } = useAppStore.getState()
    if (!itemText.trim()) {
      setParsed(null)
      setParseError(null)
      return
    }
    let cancelled = false
    const timer = setTimeout(() => {
      solver
        .parse(itemText)
        .then((parsed) => {
          if (cancelled) return
          setParsed(parsed)
          setParseError(null)
        })
        .catch((reason: unknown) => {
          if (cancelled) return
          setParsed(null)
          setParseError(reason instanceof Error ? reason.message : String(reason))
        })
    }, 250)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [itemText])

  useEffect(() => {
    if (!patternText.trim() || !itemText.trim()) return
    const timer = setTimeout(() => solver.solve(patternText, itemText), 250)
    return () => clearTimeout(timer)
  }, [patternText, itemText])

  function share() {
    const url = new URL(buildShareUrl(patternText, itemText), window.location.origin).toString()
    window.history.replaceState(null, '', url)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2000)
    void navigator.clipboard?.writeText(url).catch(() => undefined)
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>how.poe.sale</h1>
        <span className="version">{version ? `v${version}` : 'starting…'}</span>
        {busy && <span className="busy">solving…</span>}
        <span className="spacer" />
        <ExamplesMenu examples={examples} />
        <button type="button" onClick={share} disabled={!patternText.trim() || !itemText.trim()}>
          {copied ? 'Link copied' : 'Share link'}
        </button>
      </header>

      {notice && (
        <div className="banner banner-notice">
          <span>{notice}</span>
          <button type="button" onClick={() => setNotice(null)}>
            dismiss
          </button>
        </div>
      )}
      {error && (
        <div className="banner banner-error">
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)}>
            dismiss
          </button>
        </div>
      )}

      <main className="columns">
        <div className="stack">
          <TargetPanel patterns={patterns} target={solver.target} />
          <ItemPanel />
        </div>
        <div className="stack">
          {result ? (
            <>
              <NowLine now={result.now} />
              <StepsTable title="Exact steps" note="Every outcome is computed." steps={result.exact} />
              <StepsTable
                title="Certain part only"
                note="The random rest of these steps is not modelled yet, so their numbers come from what the step certainly does."
                steps={result.certain}
              />
              <StepsTable title="Not computed" note="The model stops before their result." steps={result.blind} />
              <UnmetList unmet={result.unmet} />
              <SourcesPanel sources={result.sources} />
            </>
          ) : (
            <section className="panel hint">Paste an item and choose a target.</section>
          )}
        </div>
      </main>
    </div>
  )
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
