import { StrictMode, useEffect, useMemo, useState } from 'react'
import { createRoot } from 'react-dom/client'
import '@xyflow/react/dist/style.css'
import './styles.css'
import { Donors } from './components/Donors'
import { NodePanel } from './components/NodePanel'
import { Prices } from './components/Prices'
import { Stats } from './components/Stats'
import { TargetEditor } from './components/TargetEditor'
import { Tree } from './components/Tree'
import { buildProblem } from './problem'
import { buildShareUrl } from './share'
import { solver } from './solver'
import { useAppStore } from './store'

function App() {
  const target = useAppStore((state) => state.target)
  const prices = useAppStore((state) => state.prices)
  const donors = useAppStore((state) => state.donors)
  const added = useAppStore((state) => state.added)
  const expanded = useAppStore((state) => state.expanded)
  const disabled = useAppStore((state) => state.disabled)
  const buy = useAppStore((state) => state.buy)
  const sell = useAppStore((state) => state.sell)
  const solution = useAppStore((state) => state.solution)
  const busy = useAppStore((state) => state.busy)
  const error = useAppStore((state) => state.error)
  const notice = useAppStore((state) => state.notice)
  const setError = useAppStore((state) => state.setError)
  const clearNotice = useAppStore((state) => state.clearNotice)

  const [version, setVersion] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [selected, setSelected] = useState('goal')

  useEffect(() => {
    let cancelled = false
    solver
      .version()
      .then((value) => {
        if (!cancelled) setVersion(value)
      })
      .catch(() => {
        // the worker reports load failures through the solve error banner
      })
    return () => {
      cancelled = true
    }
  }, [])

  const problem = useMemo(
    () => buildProblem({ target, prices, donors, added, expanded, disabled, buy, sell }),
    [target, prices, donors, added, expanded, disabled, buy, sell],
  )

  useEffect(() => {
    if (!problem) return
    const timer = setTimeout(() => solver.solve(problem), 250)
    return () => clearTimeout(timer)
  }, [problem])

  // Fall back to the goal only when a fresh solution lacks the selected node; a key the user just
  // seeded with Use is kept even if an older solution (without it) is still on screen.
  useEffect(() => {
    if (!solution) return
    setSelected((current) =>
      current === 'goal' ||
      solution.nodes.some((node) => node.key === current) ||
      useAppStore.getState().added.includes(current)
        ? current
        : 'goal',
    )
  }, [solution])

  // A new target invalidates every state key.
  useEffect(() => {
    setSelected('goal')
  }, [target])

  function share() {
    if (!problem) return
    const url = new URL(buildShareUrl(problem), window.location.origin).toString()
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
        <button type="button" onClick={share} disabled={!problem}>
          {copied ? 'Link copied' : 'Share link'}
        </button>
      </header>

      {notice && (
        <div className="banner banner-notice">
          <span>{notice}</span>
          <button type="button" onClick={clearNotice}>
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

      <main>
        <TargetEditor parse={solver.parse} />

        <div className="workspace">
          <section className="panel graph-panel">
            <header className="panel-header">
              <h2>Tree</h2>
              <span className="hint">craft edges are solid on the solved policy, dashed when they close a cycle</span>
              {solution?.error && <span className="error-inline">{solution.error.message}</span>}
            </header>
            {solution ? (
              <Tree solution={solution} selected={selected} onSelect={setSelected} />
            ) : (
              <div className="graph-empty">
                {problem ? 'Solving…' : 'Paste a target item to start. The tree grows from the goal backwards.'}
              </div>
            )}
          </section>

          <aside className="side">
            <NodePanel selected={selected} onSelect={setSelected} />
            <Stats />
          </aside>
        </div>

        <div className="lower">
          <Prices />
          <Donors influenceMods={solver.influenceMods} />
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
