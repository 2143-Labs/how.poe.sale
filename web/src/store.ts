import { create } from 'zustand'
import { parseShareHash } from './share'
import type { ItemView, SolveView } from './types'

/** Where the three texts are kept between visits. */
export const STORAGE_KEY = 'how.poe.sale:one-step'

/** The previous version's key: nothing reads it any more. */
const LEGACY_STORAGE_KEY = 'how.poe.sale:v2'

interface Texts {
  patternText: string
  targetText: string
  itemText: string
}

export interface AppState extends Texts {
  /** The pasted item as the engine read it. */
  parsed: ItemView | null
  parseError: string | null
  result: SolveView | null
  busy: boolean
  error: string | null
  /** A share-link problem, shown as a dismissible banner. */
  notice: string | null
  setPatternText: (text: string) => void
  setTargetText: (text: string) => void
  setItemText: (text: string) => void
  setParsed: (parsed: ItemView | null) => void
  setParseError: (message: string | null) => void
  setResult: (result: SolveView | null) => void
  setBusy: (busy: boolean) => void
  setError: (message: string | null) => void
  setNotice: (message: string | null) => void
}

const empty: Texts = { patternText: '', targetText: '', itemText: '' }

function saved(): Texts {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return empty
    const value = JSON.parse(raw) as Partial<Texts> | null
    if (!value || typeof value !== 'object') return empty
    const text = (x: unknown) => (typeof x === 'string' ? x : '')
    return { patternText: text(value.patternText), targetText: text(value.targetText), itemText: text(value.itemText) }
  } catch {
    return empty
  }
}

/** The texts the page opens with, and a notice when a share link could not be used. */
function initial(): Texts & { notice: string | null } {
  const hash = typeof window === 'undefined' ? '' : window.location.hash
  const share = parseShareHash(hash)
  switch (share.kind) {
    case 'ok':
      window.history.replaceState(null, '', window.location.pathname + window.location.search)
      return { patternText: share.pattern, targetText: '', itemText: share.item, notice: null }
    case 'legacy':
      return {
        ...saved(),
        notice: 'This link was made by the previous version of how.poe.sale and cannot be opened.',
      }
    case 'invalid':
      return { ...saved(), notice: 'This share link could not be read.' }
    case 'none':
      return { ...saved(), notice: null }
  }
}

try {
  localStorage.removeItem(LEGACY_STORAGE_KEY)
} catch {
  // storage unavailable: nothing to clean up
}

export const useAppStore = create<AppState>((set) => ({
  ...initial(),
  parsed: null,
  parseError: null,
  result: null,
  busy: false,
  error: null,
  setPatternText: (patternText) => set({ patternText }),
  setTargetText: (targetText) => set({ targetText }),
  setItemText: (itemText) => set({ itemText }),
  setParsed: (parsed) => set({ parsed }),
  setParseError: (parseError) => set({ parseError }),
  setResult: (result) => set({ result }),
  setBusy: (busy) => set({ busy }),
  setError: (error) => set({ error }),
  setNotice: (notice) => set({ notice }),
}))

useAppStore.subscribe((state, previous) => {
  if (
    state.patternText === previous.patternText &&
    state.targetText === previous.targetText &&
    state.itemText === previous.itemText
  ) {
    return
  }
  try {
    const { patternText, targetText, itemText } = state
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ patternText, targetText, itemText }))
  } catch {
    // storage full or unavailable: the texts still live for this visit
  }
})
