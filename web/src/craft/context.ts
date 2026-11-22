import { createContext, useContext } from 'react'
import { useStore } from 'zustand'
import type { CraftState, CraftStore } from './store'

export const CraftContext = createContext<CraftStore | null>(null)

/** Reads the craft planner's state, and throws when there is no page around the call. */
export function useCraft<T>(selector: (state: CraftState) => T): T {
  const store = useContext(CraftContext)
  if (!store) throw new Error('useCraft outside a craft planner page')
  return useStore(store, selector)
}
