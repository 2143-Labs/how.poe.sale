import { DEMO_ITEMS } from '../demo'
import { useAppStore } from '../store'

/** A collapsed header menu of example items; picking one pastes and parses it in the Target panel. */
export function DemoMenu() {
  const requestPaste = useAppStore((state) => state.requestPaste)
  return (
    <details className="demo-menu">
      <summary>Examples</summary>
      <div className="demo-menu-list">
        {DEMO_ITEMS.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={(event) => {
              requestPaste(item.text)
              event.currentTarget.closest('details')?.removeAttribute('open')
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
    </details>
  )
}
