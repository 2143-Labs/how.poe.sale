import { createContext, useContext } from 'react'
import { useStore } from 'zustand'
import type { Client } from '../client'
import type { Scope } from './scope'
import type { FlowState, FlowStore } from './store'

/** What every component of a flowchart page shares: its page, its worker and its state. */
export interface Flow {
  scope: Scope
  client: Client
  store: FlowStore
}

export const FlowContext = createContext<Flow | null>(null)

export function useFlow(): Flow {
  const flow = useContext(FlowContext)
  if (!flow) throw new Error('useFlow outside a mounted page')
  return flow
}

export function useFlowState<T>(selector: (state: FlowState) => T): T {
  return useStore(useFlow().store, selector)
}

/** An error's text, for the banner. */
export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
