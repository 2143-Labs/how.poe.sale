import { useAppStore } from '../store'

/** The item to craft on, and how the engine read it. */
export function ItemPanel() {
  const itemText = useAppStore((state) => state.itemText)
  const setItemText = useAppStore((state) => state.setItemText)
  const parsed = useAppStore((state) => state.parsed)
  const parseError = useAppStore((state) => state.parseError)
  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Your item</h2>
      </header>
      <textarea
        rows={14}
        value={itemText}
        placeholder="Paste an item copied in game (Ctrl+C)"
        onChange={(event) => setItemText(event.target.value)}
      />
      {parseError && <div className="error-inline">{parseError}</div>}
      {parsed && (
        <>
          {parsed.warnings.length > 0 && (
            <ul className="warnings">
              {parsed.warnings.map((warning, i) => (
                <li key={i}>{warning}</li>
              ))}
            </ul>
          )}
          <details open>
            <summary>As read</summary>
            <pre className="item-text">{parsed.text}</pre>
          </details>
        </>
      )}
    </section>
  )
}
