/**
 * Backend Data Adapter
 * Transforms CamHack backend data format into Packet Royale frontend format
 */

import type {
  GameState,
  GameNode,
  BandwidthStream,
  PlayerState,
  HexCoordinate,
} from '../types/gameTypes';
import { hexCoord, getNodeKey, NodeType, NodeState } from '../types/gameTypes';
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

/**
 * Get all hexagonal neighbors for a coordinate
 */
function getHexNeighbors(coord: HexCoordinate): HexCoordinate[] {
  return [
    hexCoord(coord.q + 1, coord.r), // East
    hexCoord(coord.q - 1, coord.r), // West
    hexCoord(coord.q, coord.r + 1), // Southeast
    hexCoord(coord.q, coord.r - 1), // Northwest
    hexCoord(coord.q + 1, coord.r - 1), // Northeast
    hexCoord(coord.q - 1, coord.r + 1), // Southwest
  ];
}

/**
 * Convert backend coordinate to frontend coordinate
 */
function convertCoord(backendCoord: BackendNodeCoord): HexCoordinate {
  return hexCoord(backendCoord.q, backendCoord.r);
}

/**
 * Determine node type based on ownership and position
 */
function determineNodeType(
  node: BackendNodeInfo,
  players: BackendPlayerInfo[]
): NodeType {
  // Neutral node
  if (node.owner_id === null) {
    return NodeType.NEUTRAL;
  }

  // Check if this is a player's capital (BASE)
  const owner = players.find((p) => p.player_id === node.owner_id);
  if (
    owner &&
    owner.capital_coord.q === node.coord.q &&
    owner.capital_coord.r === node.coord.r
  ) {
    return NodeType.BASE;
  }

  // Regular owned node
  return NodeType.OWNED;
}

/**
 * Calculate fog of war visibility for current player
 * A node is explored if:
 * - It's owned by the player
 * - It's adjacent to a node owned by the player
 * - It's being attacked by the player
 */
function calculateVisibility(
  nodes: BackendNodeInfo[],
  currentPlayerId: number
): Set<string> {
  const visibleNodeKeys = new Set<string>();

  // Get all nodes owned by current player
  const playerNodes = nodes.filter((n) => n.owner_id === currentPlayerId);

  playerNodes.forEach((playerNode) => {
    const coord = convertCoord(playerNode.coord);
    const key = getNodeKey(coord);

    // Mark player's own node as visible
    visibleNodeKeys.add(key);

    // Mark all neighbors as visible
    const neighbors = getHexNeighbors(coord);
    neighbors.forEach((neighborCoord) => {
      visibleNodeKeys.add(getNodeKey(neighborCoord));
    });
  });

  // Also mark nodes being attacked by player as visible
  playerNodes.forEach((playerNode) => {
    if (playerNode.current_target) {
      const targetCoord = convertCoord(playerNode.current_target);
      visibleNodeKeys.add(getNodeKey(targetCoord));
    }
  });

  return visibleNodeKeys;
}

/**
 * Derive bandwidth streams from node attack targets
 */
function deriveStreams(
  nodes: BackendNodeInfo[],
  visibilitySet: Set<string>
): Map<string, BandwidthStream> {
  const streams = new Map<string, BandwidthStream>();

  nodes.forEach((node) => {
    // Skip nodes with no target
    if (!node.current_target || node.owner_id === null) {
      return;
    }

    const sourceCoord = convertCoord(node.coord);
    const targetCoord = convertCoord(node.current_target);
    const sourceKey = getNodeKey(sourceCoord);
    const targetKey = getNodeKey(targetCoord);

    // Only create stream if source or target is visible
    if (!visibilitySet.has(sourceKey) && !visibilitySet.has(targetKey)) {
      return;
    }

    const streamId = `${sourceKey}->${targetKey}`;

    // Simulate bandwidth data (backend doesn't expose metrics yet)
    const bandwidth = 5.0 + Math.random() * 5.0; // 5-10 Gbps
    const maxBandwidth = 10.0;
    const utilization = bandwidth / maxBandwidth;
    const packetsSent = Math.floor(Math.random() * 50000) + 10000;
    const packetsLost = Math.floor(packetsSent * utilization * 0.1); // Simulate 10% loss at high utilization

    streams.set(streamId, {
      id: streamId,
      sourceNodeId: sourceKey,
      targetNodeId: targetKey,
      ownerId: node.owner_id,
      bandwidth,
      maxBandwidth,
      packetsSent,
      packetsLost,
      active: true,
    });
  });

  return streams;
}

/**
 * Calculate node state based on incoming streams
 */
function calculateNodeState(
  nodeKey: string,
  nodeOwnerId: number | null,
  streams: Map<string, BandwidthStream>
): NodeState {
  // Check if node is being attacked (has incoming streams from different owner)
  const incomingStreams = Array.from(streams.values()).filter(
    (s) => s.targetNodeId === nodeKey && s.ownerId !== nodeOwnerId
  );

  if (incomingStreams.length > 0) {
    // Node is under attack
    return NodeState.UNDER_ATTACK;
  }

  return NodeState.IDLE;
}

/**
 * Calculate fake capture progress based on attack intensity
 */
function calculateCaptureProgress(
  nodeKey: string,
  streams: Map<string, BandwidthStream>
): number {
  const incomingStreams = Array.from(streams.values()).filter(
    (s) => s.targetNodeId === nodeKey
  );

  if (incomingStreams.length === 0) {
    return 0;
  }

  // Calculate total incoming bandwidth
  const totalBandwidth = incomingStreams.reduce((sum, s) => sum + s.bandwidth, 0);

  // Assume max capacity is 10 Gbps, calculate progress
  const maxCapacity = 10.0;
  const progress = Math.min(totalBandwidth / maxCapacity, 0.95); // Cap at 95% until actual capture

  return progress;
}

/**
 * Transform players with color assignment and throughput calculation
 */
function transformPlayers(
  backendPlayers: BackendPlayerInfo[],
  streams: Map<string, BandwidthStream>
): PlayerState[] {
  return backendPlayers.map((p) => {
    // Assign color based on player ID
    const color = PLAYER_COLORS[p.player_id % PLAYER_COLORS.length];

    // Calculate total throughput from outgoing streams
    const playerStreams = Array.from(streams.values()).filter(
      (s) => s.ownerId === p.player_id
    );
    const totalThroughput = playerStreams.reduce(
      (sum, s) => sum + s.bandwidth,
      0
    );

    const capitalKey = getNodeKey(convertCoord(p.capital_coord));

    return {
      id: p.player_id,
      name: p.name,
      color,
      totalThroughput,
      nodesOwned: p.node_count,
      maxNodes: 100, // Arbitrary limit for now
      baseNodeId: capitalKey,
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
  streams: Map<string, BandwidthStream>
): Map<string, GameNode> {
  const nodes = new Map<string, GameNode>();

  backendNodes.forEach((n) => {
    const coord = convertCoord(n.coord);
    const key = getNodeKey(coord);
    const type = determineNodeType(n, backendPlayers);
    const state = calculateNodeState(key, n.owner_id, streams);
    const captureProgress = calculateCaptureProgress(key, streams);
    const explored = visibilitySet.has(key);

    // Simulate bandwidth (backend doesn't expose this yet)
    const bandwidth = 5.0 + Math.random() * 3.0;
    const maxBandwidth = type === NodeType.BASE ? 20.0 : 10.0;

    nodes.set(key, {
      id: key,
      position: coord,
      type,
      state,
      ownerId: n.owner_id,
      bandwidth,
      maxBandwidth,
      captureProgress,
      explored,
    });
  });

  return nodes;
}

/**
 * Main transformation function: Backend format → Frontend format
 */
export function transformBackendToFrontend(
  backend: BackendGameState,
  currentPlayerId: number
): GameState {
  // 1. Calculate visibility (fog of war)
  const visibilitySet = calculateVisibility(backend.nodes, currentPlayerId);

  // 2. Derive streams from attack targets
  const streams = deriveStreams(backend.nodes, visibilitySet);

  // 3. Transform players
  const players = transformPlayers(backend.players, streams);

  // 4. Transform nodes
  const nodes = transformNodes(
    backend.nodes,
    backend.players,
    visibilitySet,
    streams
  );

  // 5. Build final game state
  return {
    players,
    nodes,
    streams,
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
