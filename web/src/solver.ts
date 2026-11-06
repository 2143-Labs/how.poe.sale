import { createSolverClient } from './solverClient'
import { useAppStore } from './store'

/**
 * The app's single solver worker. Only the components import this module, so tests
 * never start a worker.
 */
export const solver = createSolverClient(
  new Worker(new URL('./solver.worker.ts', import.meta.url), { type: 'module' }),
  {
    onResult: (result) => useAppStore.getState().setResult(result),
    onBusy: (busy) => useAppStore.getState().setBusy(busy),
    onError: (message) => useAppStore.getState().setError(message),
  },
)
