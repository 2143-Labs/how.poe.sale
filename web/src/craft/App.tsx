import { useCraft } from './context'
import { CraftEditor } from './CraftEditor'
import { GoalEditor } from './GoalEditor'
import { LibraryPanel } from './Library'
import { PriceEditor } from './PriceEditor'
import { ResultPanel } from './ResultPanel'
import type { Tab } from './store'

const TABS: { id: Tab; label: string }[] = [
  { id: 'craft', label: 'Craft' },
  { id: 'goal', label: 'Goal' },
  { id: 'prices', label: 'Prices' },
]

/** The craft planner: the library, the editor of the current tab, and the last run's result. */
export function App() {
  const tab = useCraft((state) => state.tab)
  const busy = useCraft((state) => state.busy)
  const notice = useCraft((state) => state.notice)
  const setTab = useCraft((state) => state.setTab)

  return (
    <div className="app">
      <header className="app-header">
        <h1>
          <a href="/">how.poe.sale</a>
        </h1>
        <span className="scope-title">Craft planner</span>
        {busy && <span className="busy">Computing…</span>}
      </header>

      {notice && <div className="banner banner-notice">{notice}</div>}

      <main className="craft-columns">
        <LibraryPanel />

        <section className="panel">
          <div className="tabs">
            {TABS.map((item) => (
              <button
                key={item.id}
                type="button"
                className={item.id === tab ? 'tab is-current' : 'tab'}
                onClick={() => setTab(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>
          {tab === 'craft' && <CraftEditor />}
          {tab === 'goal' && <GoalEditor />}
          {tab === 'prices' && <PriceEditor />}
        </section>

        <ResultPanel />
      </main>
    </div>
  )
}
