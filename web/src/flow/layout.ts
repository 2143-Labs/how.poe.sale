import ELK from 'elkjs/lib/elk.bundled.js'
import type { ElkNode } from 'elkjs/lib/elk-api'

const elk = new ELK()

export const NODE_WIDTH = 260

export const LAYOUT_OPTIONS: Record<string, string> = {
  'elk.algorithm': 'layered',
  'elk.direction': 'DOWN',
  'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
  'elk.layered.spacing.nodeNodeBetweenLayers': '110',
  'elk.spacing.nodeNode': '64',
}

export type LayoutPositions = Record<string, { x: number; y: number }>

export interface LayoutNodeSize {
  id: string
  height: number
}

export interface LayoutEdgeEnds {
  id: string
  source: string
  target: string
}

/** `elkjs` layered, top-down; runs on the main thread whenever the node set changes. */
export async function layoutGraph(nodes: LayoutNodeSize[], edges: LayoutEdgeEnds[]): Promise<LayoutPositions> {
  const graph: ElkNode = {
    id: 'root',
    layoutOptions: LAYOUT_OPTIONS,
    children: nodes.map((node) => ({ id: node.id, width: NODE_WIDTH, height: node.height })),
    edges: edges.map((edge) => ({ id: edge.id, sources: [edge.source], targets: [edge.target] })),
  }
  const laid = await elk.layout(graph)
  const positions: LayoutPositions = {}
  for (const child of laid.children ?? []) {
    positions[child.id] = { x: child.x ?? 0, y: child.y ?? 0 }
  }
  return positions
}
