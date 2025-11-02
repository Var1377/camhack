/**
 * Graph Backend Data Adapter
 * Transforms CamHack backend data format into Graph-based frontend format
 */

import type {
  NetworkGameState,
  NetworkNode,
  NetworkEdge,
  PlayerState,
} from '../types/graphTypes';
import type {
  BackendGameState,
  BackendNodeCoord,
  BackendPlayerInfo,
  BackendNodeInfo,
} from '../services/backendApi';

// Player color palette (matches frontend visual constants)
const PLAYER_COLORS = [
  0x00ffff, // Cyan (Player 1)
  0xff1493, // Deep Pink (Player 2)
  0x00ff00, // Green (Player 3)
  0xff8800, // Orange (Player 4)
  0x9d00ff, // Purple (Player 5)
  0xffff00, // Yellow (Player 6)
];

// Layout configuration for graph visualization
const LAYOUT_CONFIG = {
  CENTER_X: 0,
  CENTER_Y: 0,
  NODE_SPACING: 200, // Distance between connected nodes
  RING_RADIUS: 150, // Radius for each ring from center
};

/**
 * Convert backend hex coordinate to graph pixel position
 * Uses a radial layout based on distance from origin
 */
function coordToPosition(coord: BackendNodeCoord): { x: number; y: number } {
  const { q, r } = coord;

  // Calculate distance from origin (ring number)
  const distance = Math.max(Math.abs(q), Math.abs(r), Math.abs(-q - r));

  // Calculate angle based on position in hex grid
  const angle = Math.atan2(r * 1.5, q * Math.sqrt(3));

  // Convert to pixel position
  const radius = distance * LAYOUT_CONFIG.RING_RADIUS;
  const x = LAYOUT_CONFIG.CENTER_X + radius * Math.cos(angle);
  const y = LAYOUT_CONFIG.CENTER_Y + radius * Math.sin(angle);

  return { x, y };
}

/**
 * Generate node ID from coordinate
 */
function getNodeId(coord: BackendNodeCoord): string {
  return `${coord.q},${coord.r}`;
}

/**
 * Determine node type based on ownership and position
 */
function determineNodeType(
  node: BackendNodeInfo,
  players: BackendPlayerInfo[]
): 'NEUTRAL' | 'OWNED' | 'BASE' {
  // Neutral node
  if (node.owner_id === null) {
    return 'NEUTRAL';
  }

  // Check if this is a player's capital (BASE)
  const owner = players.find((p) => p.player_id === node.owner_id);
  if (
    owner &&
    owner.capital_coord.q === node.coord.q &&
    owner.capital_coord.r === node.coord.r
  ) {
    return 'BASE';
  }

  // Regular owned node
  return 'OWNED';
}

/**
 * Get all neighboring hex coordinates
 */
function getHexNeighbors(coord: BackendNodeCoord): BackendNodeCoord[] {
  return [
    { q: coord.q + 1, r: coord.r },
    { q: coord.q - 1, r: coord.r },
    { q: coord.q, r: coord.r + 1 },
    { q: coord.q, r: coord.r - 1 },
    { q: coord.q + 1, r: coord.r - 1 },
    { q: coord.q - 1, r: coord.r + 1 },
  ];
}

/**
 * Calculate fog of war visibility for current player
 */
function calculateVisibility(
  nodes: BackendNodeInfo[],
  currentPlayerId: number
): Set<string> {
  const visibleNodeIds = new Set<string>();

  // Get all nodes owned by current player
  const playerNodes = nodes.filter((n) => n.owner_id === currentPlayerId);

  playerNodes.forEach((playerNode) => {
    const nodeId = getNodeId(playerNode.coord);

    // Mark player's own node as visible
    visibleNodeIds.add(nodeId);

    // Mark all neighbors as visible
    const neighbors = getHexNeighbors(playerNode.coord);
    neighbors.forEach((neighborCoord) => {
      visibleNodeIds.add(getNodeId(neighborCoord));
    });
  });

  // Also mark nodes being attacked by player as visible
  playerNodes.forEach((playerNode) => {
    if (playerNode.current_target) {
      visibleNodeIds.add(getNodeId(playerNode.current_target));
    }
  });

  return visibleNodeIds;
}

/**
 * Build connection graph (edges between nodes)
 */
function buildConnections(nodes: BackendNodeInfo[]): Map<string, string[]> {
  const connections = new Map<string, string[]>();

  // Initialize connections for all nodes
  nodes.forEach((node) => {
    const nodeId = getNodeId(node.coord);
    connections.set(nodeId, []);
  });

  // Add connections based on adjacency in hex grid
  nodes.forEach((node) => {
    const nodeId = getNodeId(node.coord);
    const neighbors = getHexNeighbors(node.coord);

    neighbors.forEach((neighborCoord) => {
      const neighborId = getNodeId(neighborCoord);
      // Only add connection if neighbor exists
      if (connections.has(neighborId)) {
        connections.get(nodeId)!.push(neighborId);
      }
    });
  });

  return connections;
}

/**
 * Derive active edges (attacks) from node targets
 */
function deriveEdges(
  nodes: BackendNodeInfo[],
  visibilitySet: Set<string>
): Map<string, NetworkEdge> {
  const edges = new Map<string, NetworkEdge>();

  nodes.forEach((node) => {
    // Skip nodes with no target
    if (!node.current_target || node.owner_id === null) {
      return;
    }

    const sourceId = getNodeId(node.coord);
    const targetId = getNodeId(node.current_target);

    // Only create edge if source or target is visible
    if (!visibilitySet.has(sourceId) && !visibilitySet.has(targetId)) {
      return;
    }

    const edgeId = `${sourceId}->${targetId}`;

    // Use real metrics from backend if available, otherwise simulate
    const bandwidth = node.bandwidth_in ? node.bandwidth_in / 1_000_000 : 5.0 + Math.random() * 5.0; // Convert bytes to Gbps
    const maxBandwidth = 10.0;
    const packetLossRatio = node.packet_loss ?? Math.random() * 0.1; // Use real packet loss or simulate
    const packetsSent = Math.floor(Math.random() * 50000) + 10000;
    const packetsLost = Math.floor(packetsSent * packetLossRatio);

    edges.set(edgeId, {
      id: edgeId,
      sourceNodeId: sourceId,
      targetNodeId: targetId,
      ownerId: node.owner_id,
      bandwidth,
      maxBandwidth,
      packetsSent,
      packetsLost,
      active: true,
    });
  });

  return edges;
}

/**
 * Calculate node state based on incoming edges
 */
function calculateNodeState(
  nodeId: string,
  nodeOwnerId: number | null,
  edges: Map<string, NetworkEdge>
): 'IDLE' | 'UNDER_ATTACK' | 'CAPTURING' | 'CAPTURED' {
  // Check if node is being attacked
  const incomingEdges = Array.from(edges.values()).filter(
    (e) => e.targetNodeId === nodeId && e.ownerId !== nodeOwnerId
  );

  if (incomingEdges.length > 0) {
    return 'UNDER_ATTACK';
  }

  return 'IDLE';
}

/**
 * Calculate capture progress based on attack intensity
 */
function calculateCaptureProgress(
  nodeId: string,
  edges: Map<string, NetworkEdge>
): number {
  const incomingEdges = Array.from(edges.values()).filter(
    (e) => e.targetNodeId === nodeId
  );

  if (incomingEdges.length === 0) {
    return 0;
  }

  const totalBandwidth = incomingEdges.reduce((sum, e) => sum + e.bandwidth, 0);
  const maxCapacity = 10.0;
  const progress = Math.min(totalBandwidth / maxCapacity, 0.95);

  return progress;
}

/**
 * Transform players with color assignment and throughput calculation
 */
function transformPlayers(
  backendPlayers: BackendPlayerInfo[],
  edges: Map<string, NetworkEdge>
): PlayerState[] {
  return backendPlayers.map((p) => {
    const color = PLAYER_COLORS[p.player_id % PLAYER_COLORS.length];

    const playerEdges = Array.from(edges.values()).filter(
      (e) => e.ownerId === p.player_id
    );
    const totalThroughput = playerEdges.reduce((sum, e) => sum + e.bandwidth, 0);

    const capitalId = getNodeId(p.capital_coord);

    return {
      id: p.player_id,
      name: p.name,
      color,
      totalThroughput,
      nodesOwned: p.node_count,
      maxNodes: 100,
      baseNodeId: capitalId,
      isAlive: p.alive,
    };
  });
}

/**
 * Transform nodes with type, state, and visibility
 */
function transformNodes(
  backendNodes: BackendNodeInfo[],
  backendPlayers: BackendPlayerInfo[],
  visibilitySet: Set<string>,
  edges: Map<string, NetworkEdge>,
  connections: Map<string, string[]>
): Map<string, NetworkNode> {
  const nodes = new Map<string, NetworkNode>();

  backendNodes.forEach((n) => {
    const nodeId = getNodeId(n.coord);
    const position = coordToPosition(n.coord);
    const type = determineNodeType(n, backendPlayers);
    const state = calculateNodeState(nodeId, n.owner_id, edges);
    const captureProgress = calculateCaptureProgress(nodeId, edges);
    const explored = visibilitySet.has(nodeId);

    // Use real bandwidth from backend if available
    const bandwidth = n.bandwidth_in ? n.bandwidth_in / 1_000_000 : 5.0 + Math.random() * 3.0; // Convert bytes to Gbps
    const maxBandwidth = type === 'BASE' ? 20.0 : 10.0;

    nodes.set(nodeId, {
      id: nodeId,
      position,
      type,
      state,
      ownerId: n.owner_id,
      bandwidth,
      maxBandwidth,
      captureProgress,
      explored,
      connections: connections.get(nodeId) || [],
      hexCoord: { q: n.coord.q, r: n.coord.r }, // Preserve hex coordinates from backend
    });
  });

  return nodes;
}

/**
 * Main transformation function: Backend format → Graph frontend format
 */
export function transformBackendToGraph(
  backend: BackendGameState,
  currentPlayerId: number
): NetworkGameState {
  // 1. Calculate visibility (fog of war)
  const visibilitySet = calculateVisibility(backend.nodes, currentPlayerId);

  // 2. Build connection graph
  const connections = buildConnections(backend.nodes);

  // 3. Derive edges from attack targets
  const edges = deriveEdges(backend.nodes, visibilitySet);

  // 4. Transform players
  const players = transformPlayers(backend.players, edges);

  // 5. Transform nodes
  const nodes = transformNodes(
    backend.nodes,
    backend.players,
    visibilitySet,
    edges,
    connections
  );

  return {
    players,
    nodes,
    edges,
    currentTick: backend.total_events,
    currentPlayerId,
  };
}

/**
 * Validate backend data structure
 */
export function validateBackendData(data: unknown): data is BackendGameState {
  if (typeof data !== 'object' || data === null) {
    return false;
  }

  const state = data as Partial<BackendGameState>;

  return (
    Array.isArray(state.players) &&
    Array.isArray(state.nodes) &&
    typeof state.total_events === 'number'
  );
}
