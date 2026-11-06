import { useAppStore } from '../store'
import type { Example } from '../types'

/** A collapsed header menu of example pastes; picking one puts it in Your item. */
export function ExamplesMenu({ examples }: { examples: Example[] }) {
  const setItemText = useAppStore((state) => state.setItemText)
  return (
    <details className="demo-menu">
      <summary>Examples</summary>
      <div className="demo-menu-list">
        {examples.map((example) => (
          <button
            key={example.id}
            type="button"
            onClick={(event) => {
              setItemText(example.text)
              event.currentTarget.closest('details')?.removeAttribute('open')
            }}
          >
            {example.label}
          </button>
        ))}
      </div>
    </details>
  )
}
