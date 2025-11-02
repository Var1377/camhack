/**
 * Dummy Data Generator for Testing Visualization
 */

import type {
  GameState,
  GameNode,
  BandwidthStream,
  PlayerState,
  HexCoordinate,
} from '../types/gameTypes';
import {
  NodeType,
  NodeState,
  hexCoord,
  getNodeKey,
} from '../types/gameTypes';
import { PLAYER_COLORS } from '../config/visualConstants';

/**
 * Generate a spiral pattern of hex coordinates
 */
function generateHexSpiral(rings: number): HexCoordinate[] {
  const coords: HexCoordinate[] = [hexCoord(0, 0)]; // Center

  for (let ring = 1; ring <= rings; ring++) {
    let q = 0;
    let r = -ring;

    // Directions for hex spiral: SE, S, SW, NW, N, NE
    const directions = [
      { q: 1, r: 0 },   // SE
      { q: 0, r: 1 },   // S
      { q: -1, r: 1 },  // SW
      { q: -1, r: 0 },  // NW
      { q: 0, r: -1 },  // N
      { q: 1, r: -1 },  // NE
    ];

    for (const dir of directions) {
      for (let i = 0; i < ring; i++) {
        coords.push(hexCoord(q, r));
        q += dir.q;
        r += dir.r;
      }
    }
  }

  return coords;
}

/**
 * Create dummy player states
 */
function createDummyPlayers(): PlayerState[] {
  return [
    {
      id: 0,
      name: 'Player 1 (You)',
      color: PLAYER_COLORS[0],
      totalThroughput: 12.5,
      nodesOwned: 5,
      maxNodes: 10,
      baseNodeId: '0,0',
      isAlive: true,
    },
    {
      id: 1,
      name: 'Player 2',
      color: PLAYER_COLORS[1],
      totalThroughput: 10.2,
      nodesOwned: 4,
      maxNodes: 10,
      baseNodeId: '5,-3',
      isAlive: true,
    },
  ];
}

/**
 * Create dummy nodes in a hex grid pattern
 */
function createDummyNodes(_players: PlayerState[]): Map<string, GameNode> {
  const nodes = new Map<string, GameNode>();
  const hexCoords = generateHexSpiral(5); // 5 rings = ~91 hexes

  hexCoords.forEach((coord, index) => {
    const key = getNodeKey(coord);
    let type: (typeof NodeType)[keyof typeof NodeType] = NodeType.NEUTRAL;
    let ownerId: number | null = null;
    let explored = false;

    // Player 1 base at center
    if (key === '0,0') {
      type = NodeType.BASE;
      ownerId = 0;
      explored = true;
    }
    // Player 2 base at offset position
    else if (key === '5,-3') {
      type = NodeType.BASE;
      ownerId = 1;
      explored = false; // Not explored by player 1 yet
    }
    // Player 1 owned nodes around their base
    else if (index >= 1 && index <= 4) {
      type = NodeType.OWNED;
      ownerId = 0;
      explored = true;
    }
    // Player 2 owned nodes (not visible yet)
    else if (key === '6,-3' || key === '5,-2' || key === '4,-2') {
      type = NodeType.OWNED;
      ownerId = 1;
      explored = false;
    }
    // Some explored neutral nodes
    else if (index >= 5 && index <= 12) {
      explored = true;
    }

    const node: GameNode = {
      id: key,
      position: coord,
      type,
      state: NodeState.IDLE,
      ownerId,
      bandwidth: Math.random() * 5 + 2, // 2-7 Gbps
      maxBandwidth: 10,
      captureProgress: 0,
      explored,
    };

    nodes.set(key, node);
  });

  return nodes;
}

/**
 * Create dummy bandwidth streams between nodes
 */
function createDummyStreams(nodes: Map<string, GameNode>): Map<string, BandwidthStream> {
  const streams = new Map<string, BandwidthStream>();

  // Player 1's streams (visible)
  const player1Streams = [
    { from: '0,0', to: '1,-1', bw: 5.5 },    // Base to node
    { from: '0,0', to: '0,-1', bw: 4.2 },
    { from: '1,-1', to: '2,-1', bw: 3.8 },   // Extending frontier
    { from: '0,-1', to: '-1,0', bw: 2.9 },
  ];

  player1Streams.forEach(({ from, to, bw }, index) => {
    const streamId = `stream-p1-${index}`;
    streams.set(streamId, {
      id: streamId,
      sourceNodeId: from,
      targetNodeId: to,
      ownerId: 0,
      bandwidth: bw,
      maxBandwidth: 10,
      packetsSent: Math.floor(Math.random() * 10000 + 5000),
      packetsLost: Math.floor(Math.random() * 100),
      active: true,
    });
  });

  // Add one stream that's attacking a neutral node
  const attackStreamId = 'stream-attack-1';
  streams.set(attackStreamId, {
    id: attackStreamId,
    sourceNodeId: '1,-1',
    targetNodeId: '2,0', // Neutral node being captured
    ownerId: 0,
    bandwidth: 8.5, // High bandwidth = active attack
    maxBandwidth: 10,
    packetsSent: 2500,
    packetsLost: 180,
    active: true,
  });

  // Update the target node to show capture in progress
  const targetNode = nodes.get('2,0');
  if (targetNode) {
    targetNode.state = NodeState.CAPTURING;
    targetNode.captureProgress = 0.65; // 65% captured
    targetNode.explored = true;
  }

  return streams;
}

/**
 * Generate complete dummy game state
 */
export function generateDummyGameState(): GameState {
  const players = createDummyPlayers();
  const nodes = createDummyNodes(players);
  const streams = createDummyStreams(nodes);

  return {
    players,
    nodes,
    streams,
    currentTick: 0,
    currentPlayerId: 0, // We are player 1
  };
}

/**
 * Simulate bandwidth fluctuations for testing
 */
export function updateDummyGameState(state: GameState): void {
  state.currentTick++;

  // Update stream bandwidth with random fluctuations
  state.streams.forEach((stream) => {
    if (stream.active) {
      // Vary bandwidth ±20%
      const fluctuation = (Math.random() - 0.5) * 0.4;
      stream.bandwidth = Math.max(
        1,
        Math.min(stream.maxBandwidth, stream.bandwidth * (1 + fluctuation))
      );

      // Simulate packet transmission
      const packetsThisTick = Math.floor(stream.bandwidth * 10);
      stream.packetsSent += packetsThisTick;

      // Random packet loss (higher when bandwidth is near max)
      const lossRate = (stream.bandwidth / stream.maxBandwidth) * 0.05;
      stream.packetsLost += Math.random() < lossRate ? 1 : 0;
    }
  });

  // Update capture progress
  state.nodes.forEach((node) => {
    if (node.state === NodeState.CAPTURING) {
      node.captureProgress = Math.min(1, node.captureProgress + 0.01);

      if (node.captureProgress >= 1) {
        node.state = NodeState.CAPTURED;
        // In real game, would change ownership here
      }
    }
  });

  // Update player throughput
  state.players.forEach((player) => {
    const playerStreams = Array.from(state.streams.values()).filter(
      (s) => s.ownerId === player.id
    );
    player.totalThroughput = playerStreams.reduce(
      (sum, s) => sum + s.bandwidth,
      0
    );
  });
}
