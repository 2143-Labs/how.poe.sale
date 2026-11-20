import { useRef, useState } from 'react'
import { useCraft } from './context'
import { isBuiltin, type Kind } from './library'

interface Entry {
  id: string
  name: string
}

const TITLES: Record<Kind, string> = { craft: 'Crafts', filter: 'Filters', prices: 'Price lists' }

/** One list of presets: pick one, copy it, or delete it when nothing uses it. */
function Presets({ kind, entries }: { kind: Kind; entries: Entry[] }) {
  const selected = useCraft((state) => state.selected[kind])
  const select = useCraft((state) => state.select)
  const duplicate = useCraft((state) => state.duplicate)
  const remove = useCraft((state) => state.remove)
  const [error, setError] = useState<string | null>(null)

  return (
    <section>
      <h3>{TITLES[kind]}</h3>
      <ul className="picker-list">
        {entries.map((entry) => (
          <li key={entry.id} className={entry.id === selected ? 'picker-row is-current' : 'picker-row'}>
            <button type="button" onClick={() => select(kind, entry.id)}>
              {entry.name}
            </button>
            {isBuiltin(entry.id) && <span className="badge">built-in</span>}
          </li>
        ))}
      </ul>
      <div className="row-actions">
        <button type="button" onClick={() => duplicate(kind, selected)}>
          Duplicate
        </button>
        <button type="button" onClick={() => setError(remove(kind, selected))}>
          Delete
        </button>
      </div>
      {error && <p className="error-inline">{error}</p>}
    </section>
  )
}

/** The saved filters, price lists and crafts, and the import and export of the user's own. */
export function LibraryPanel() {
  const library = useCraft((state) => state.library)
  const exportLibrary = useCraft((state) => state.exportLibrary)
  const importLibrary = useCraft((state) => state.importLibrary)
  const [error, setError] = useState<string | null>(null)
  const input = useRef<HTMLInputElement>(null)

  function download() {
    const blob = new Blob([exportLibrary()], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = url
    link.download = 'craft-library.json'
    link.click()
    URL.revokeObjectURL(url)
  }

  async function upload(file: File | undefined) {
    if (!file) return
    setError(importLibrary(await file.text()))
  }

  return (
    <aside className="panel">
      <div className="panel-header">
        <h2>Library</h2>
      </div>
      <Presets kind="craft" entries={library.crafts} />
      <Presets kind="filter" entries={library.filters} />
      <Presets kind="prices" entries={library.prices} />
      <div className="row-actions">
        <button type="button" onClick={download}>
          Export
        </button>
        <button type="button" onClick={() => input.current?.click()}>
          Import
        </button>
        <input
          ref={input}
          type="file"
          accept="application/json,.json"
          hidden
          onChange={(event) => {
            const file = event.target.files?.[0]
            event.target.value = ''
            void upload(file)
          }}
        />
      </div>
      {error && <p className="error-inline">{error}</p>}
    </aside>
  )
}
