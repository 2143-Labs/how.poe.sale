import { StrictMode, useEffect, useMemo, useState } from 'react'
import { createRoot } from 'react-dom/client'
import '@xyflow/react/dist/style.css'
import './styles.css'
import { CraftList } from './components/CraftList'
import { DemoMenu } from './components/DemoMenu'
import { Donors } from './components/Donors'
import { GoodPanel } from './components/GoodPanel'
import { Market } from './components/Market'
import { NodePanel } from './components/NodePanel'
import { Prices } from './components/Prices'
import { Stats } from './components/Stats'
import { TargetEditor } from './components/TargetEditor'
import { Tree } from './components/Tree'
import { buildProblem, targetSignature } from './problem'
import { buildShareUrl } from './share'
import { solver } from './solver'
import { useAppStore } from './store'
import type { ContractJson } from './types'

function App() {
  const goods = useAppStore((state) => state.goods)
  const processes = useAppStore((state) => state.processes)
  const root = useAppStore((state) => state.root)
  const focus = useAppStore((state) => state.focus)
  const prices = useAppStore((state) => state.prices)
  const donors = useAppStore((state) => state.donors)
  const authoredRecipes = useAppStore((state) => state.authoredRecipes)
  const view = useAppStore((state) => state.view)
  const setView = useAppStore((state) => state.setView)
  const solution = useAppStore((state) => state.solution)
  const busy = useAppStore((state) => state.busy)
  const error = useAppStore((state) => state.error)
  const notice = useAppStore((state) => state.notice)
  const setError = useAppStore((state) => state.setError)
  const clearNotice = useAppStore((state) => state.clearNotice)

  const [version, setVersion] = useState<string | null>(null)
  const [contracts, setContracts] = useState<ContractJson[]>([])
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
    solver
      .contracts()
      .then((list) => {
        if (!cancelled) setContracts(list)
      })
      .catch(() => {
        // the craft list stays empty until the bridge answers
      })
    return () => {
      cancelled = true
    }
  }, [])

  const problem = useMemo(
    () => buildProblem({ goods, processes, root, focus, prices, donors, authoredRecipes }),
    [goods, processes, root, focus, prices, donors, authoredRecipes],
  )

  useEffect(() => {
    if (!problem) return
    const timer = setTimeout(() => solver.solve(problem), 250)
    return () => clearTimeout(timer)
  }, [problem])

  // Fall back to the goal only when a fresh tree lacks the selected node; a key the user just
  // seeded with Use is kept even if an older tree (without it) is still on screen.
  const tree = solution?.focus ?? null
  useEffect(() => {
    if (!tree) return
    const added = processes.find((process) => process.id === focus)?.added ?? []
    setSelected((current) =>
      current === 'goal' || tree.nodes.some((node) => node.key === current) || added.includes(current)
        ? current
        : 'goal',
    )
  }, [tree, processes, focus])

  // A new root target invalidates every state key of the focus tree.
  const rootSignature = targetSignature(problem?.goods.find((good) => good.id === problem.root)?.target ?? null)
  useEffect(() => {
    setSelected('goal')
  }, [rootSignature])

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
        <DemoMenu />
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
        <TargetEditor parse={solver.parse} modTiers={solver.modTiers} />

        <div className="workspace">
          <section className="panel graph-panel">
            <header className="panel-header">
              <h2>{view === 'market' ? 'Market' : 'Tree'}</h2>
              <div className="view-tabs">
                <button type="button" className={view === 'market' ? 'is-active' : ''} onClick={() => setView('market')}>
                  Market
                </button>
                <button
                  type="button"
                  className={view === 'process' ? 'is-active' : ''}
                  onClick={() => setView('process')}
                  disabled={!solution?.focus}
                >
                  Process tree
                </button>
              </div>
              <span className="hint">
                {view === 'market'
                  ? 'one node per good, one edge per input → output; the chosen process is solid'
                  : 'craft edges are solid on the solved policy, dashed when they close a cycle'}
              </span>
              {solution?.error && <span className="error-inline">{solution.error.message}</span>}
            </header>
            {view === 'market' ? (
              <Market />
            ) : tree ? (
              <Tree solution={tree} selected={selected} onSelect={setSelected} />
            ) : (
              <div className="graph-empty">
                {problem ? 'Solving…' : 'Paste a target item to start. The tree grows from the goal backwards.'}
              </div>
            )}
          </section>

          <aside className="side">
            {view === 'market' ? (
              <GoodPanel proposeProcesses={solver.proposeProcesses} contracts={contracts} modTiers={solver.modTiers} />
            ) : (
              <NodePanel selected={selected} onSelect={setSelected} />
            )}
            <Stats />
          </aside>
        </div>

        <div className="lower">
          <Prices />
          <CraftList contracts={contracts} />
        </div>

        <Donors influenceMods={solver.influenceMods} />
      </main>
    </div>
  )
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
