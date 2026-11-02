import { createSolverClient } from './solverClient'
import { useAppStore } from './store'

/**
 * The app's single solver worker. Only `main.tsx` imports this module, so tests
 * never start a worker.
 */
export const solver = createSolverClient(
  new Worker(new URL('./solver.worker.ts', import.meta.url), { type: 'module' }),
  {
    onSolution: (solution) => useAppStore.getState().setSolution(solution),
    onBusy: (busy) => useAppStore.getState().setBusy(busy),
    onError: (message) => useAppStore.getState().setError(message),
  },
)
