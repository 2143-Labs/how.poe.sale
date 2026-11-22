import { createContext, useContext } from 'react'
import { useStore } from 'zustand'
import type { AppClient } from './client'
import type { Scope } from './scope'
import type { AppState, AppStore } from './store'

/** What every component of a game page shares: its page, its worker and its state. */
export interface App {
  scope: Scope
  client: AppClient
  store: AppStore
}

export const AppContext = createContext<App | null>(null)

export function useApp(): App {
  const app = useContext(AppContext)
  if (!app) throw new Error('useApp outside a mounted page')
  return app
}

export function useAppState<T>(selector: (state: AppState) => T): T {
  return useStore(useApp().store, selector)
}

/** An error's text, for the banner. */
export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
