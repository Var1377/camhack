/**
 * GraphGameScene - Network graph visualization (non-grid)
 */

import Phaser from 'phaser';
import type { NetworkGameState, NetworkNode, NetworkEdge } from '../types/graphTypes';
import {
  generateNetworkGameState,
  updateNetworkGameState,
  getCapturableNodes,
  initiateCapture,
  canCaptureNode,
  getCapturableConnections,
  isCapturableConnection,
  initiateCaptureViaEdge
} from '../utils/graphData';
import { COLORS, VISUAL_CONFIG, getPlayerColor, getBandwidthColor } from '../config/visualConstants';
import { fetchGameState, pingBackend } from '../services/backendApi';
import { transformBackendToGraph } from '../adapters/graphBackendAdapter';
import { getBackendConfig, isBackendEnabled, debugLog, errorLog } from '../config/backend';

export class GraphGameScene extends Phaser.Scene {
  private gameState!: NetworkGameState;
  private nodeGraphics!: Phaser.GameObjects.Graphics;
  private edgeGraphics!: Phaser.GameObjects.Graphics;
  private connectionGraphics!: Phaser.GameObjects.Graphics;
  private fogGraphics!: Phaser.GameObjects.Graphics;
  private nodeLabelContainer!: Phaser.GameObjects.Container;
  private particleEmitters: Map<string, Phaser.GameObjects.Particles.ParticleEmitter> = new Map();

  // Game interaction
  private selectedNode: NetworkNode | null = null;
  private capturableNodes: NetworkNode[] = [];
  private capturableConnections: Array<{ sourceNodeId: string; targetNodeId: string }> = [];
  private captureMode: boolean = false;

  // Dev options
  private fogOfWarEnabled = false; // Toggle with F key
  private fogToggleKey!: Phaser.Input.Keyboard.Key;

  // Camera controls
  private cursors!: Phaser.Types.Input.Keyboard.CursorKeys;
  private isDragging = false;
  private dragStartX = 0;
  private dragStartY = 0;
  private pointerDownX = 0;
  private pointerDownY = 0;
  private hasDragged = false;
  private zoomKeys!: { plus: Phaser.Input.Keyboard.Key; minus: Phaser.Input.Keyboard.Key };

  // Backend integration
  private useBackend = false;
  private backendConnected = false;
  private currentPlayerId = 0; // Default to player 0

  constructor() {
    super({ key: 'GraphGameScene' });
  }

  preload() {
    // Create particle texture
    const graphics = this.make.graphics({});
    graphics.fillStyle(0xffffff, 1);
    graphics.fillCircle(16, 16, 16);
    graphics.generateTexture('particle', 32, 32);
    graphics.destroy();
  }

  async create() {
    // Check if backend is enabled and reachable
    if (isBackendEnabled()) {
      debugLog('Checking backend connection...');
      this.backendConnected = await pingBackend();
      this.useBackend = this.backendConnected;

      if (this.backendConnected) {
        debugLog('Backend connected successfully');
      } else {
        errorLog('Backend not reachable, falling back to dummy data');
      }
    } else {
      debugLog('Backend disabled, using dummy data');
    }

    // Initialize game state
    if (this.useBackend) {
      try {
        const backendState = await fetchGameState();
        this.gameState = transformBackendToGraph(backendState, this.currentPlayerId);
        debugLog('Loaded game state from backend:', this.gameState);
      } catch (error) {
        errorLog('Failed to fetch initial game state', error);
        this.gameState = generateNetworkGameState();
        this.useBackend = false;
      }
    } else {
      this.gameState = generateNetworkGameState();
    }

    // Set up graphics layers
    this.connectionGraphics = this.add.graphics();
    this.edgeGraphics = this.add.graphics();
    this.nodeGraphics = this.add.graphics();
    this.fogGraphics = this.add.graphics();
    this.nodeLabelContainer = this.add.container(0, 0);

    // Set up camera
    this.setupCamera();

    // Set up input controls
    this.setupControls();

    // Draw initial state
    this.drawConnections();
    this.drawEdges();
    this.drawNodes();
    this.drawFogOfWar();

    // Launch UI scene
    this.scene.launch('UIScene');

    // Listen for capture mode changes from UI (use scene manager to ensure scene is ready)
    this.scene.get('UIScene').events.on('captureModeChanged', (enabled: boolean) => {
      console.log('📡 GraphGameScene received captureModeChanged:', enabled);
      this.captureMode = enabled;

      // Update capturable connections and redraw
      if (enabled) {
        this.updateCapturableConnections();
      }
      this.drawConnections();
    });
    console.log('✅ GraphGameScene: Listening for captureModeChanged events from UIScene');

    // Start update loop
    const config = getBackendConfig();
    const updateDelay = this.useBackend ? config.pollingInterval : 100;
    this.time.addEvent({
      delay: updateDelay,
      callback: this.updateGameState,
      callbackScope: this,
      loop: true,
    });

    console.log('GraphGameScene initialized with', this.gameState.nodes.size, 'nodes');
    console.log('Backend mode:', this.useBackend ? 'ENABLED' : 'DISABLED (dummy data)');
  }

  private setupCamera() {
    const camera = this.cameras.main;
    camera.setBackgroundColor(COLORS.BACKGROUND);
    camera.setZoom(0.8); // Start zoomed out a bit to see more of the map
    camera.setBounds(-2500, -2500, 5000, 5000); // Larger bounds for bigger map

    // Keyboard controls
    this.cursors = this.input.keyboard!.createCursorKeys();

    // Zoom keys (+ and - or = and -)
    this.zoomKeys = {
      plus: this.input.keyboard!.addKey(Phaser.Input.Keyboard.KeyCodes.EQUALS),
      minus: this.input.keyboard!.addKey(Phaser.Input.Keyboard.KeyCodes.MINUS),
    };

    // Fog of war toggle (F key)
    this.fogToggleKey = this.input.keyboard!.addKey(Phaser.Input.Keyboard.KeyCodes.F);

    // Mouse wheel zoom
    this.input.on('wheel', (_pointer: any, _gameObjects: any, _deltaX: number, deltaY: number) => {
      const zoomDelta = deltaY > 0 ? -VISUAL_CONFIG.CAMERA_ZOOM_SPEED : VISUAL_CONFIG.CAMERA_ZOOM_SPEED;
      const newZoom = Phaser.Math.Clamp(
        camera.zoom + zoomDelta,
        VISUAL_CONFIG.CAMERA_ZOOM_MIN,
        VISUAL_CONFIG.CAMERA_ZOOM_MAX
      );
      camera.setZoom(newZoom);
    });
  }

  private setupControls() {
    // Mouse drag to pan (works with left or right mouse button)
    this.input.on('pointerdown', (pointer: Phaser.Input.Pointer) => {
      this.isDragging = true;
      this.hasDragged = false;
      this.dragStartX = pointer.x;
      this.dragStartY = pointer.y;
      this.pointerDownX = pointer.x;
      this.pointerDownY = pointer.y;
    });

    this.input.on('pointermove', (pointer: Phaser.Input.Pointer) => {
      if (this.isDragging) {
        const camera = this.cameras.main;
        const deltaX = (this.dragStartX - pointer.x) / camera.zoom;
        const deltaY = (this.dragStartY - pointer.y) / camera.zoom;

        // Detect if mouse has moved enough to be considered a drag
        const totalDragDistance = Math.sqrt(
          Math.pow(pointer.x - this.pointerDownX, 2) +
          Math.pow(pointer.y - this.pointerDownY, 2)
        );

        if (totalDragDistance > 5) {
          this.hasDragged = true;
          this.input.setDefaultCursor('grab');
        }

        camera.scrollX += deltaX;
        camera.scrollY += deltaY;
        this.dragStartX = pointer.x;
        this.dragStartY = pointer.y;
      } else {
        this.handleNodeHover(pointer);
      }
    });

    this.input.on('pointerup', (pointer: Phaser.Input.Pointer) => {
      this.isDragging = false;

      // Only trigger node click if we didn't drag
      if (!this.hasDragged && pointer.leftButtonReleased()) {
        this.handleNodeClick(pointer);
      }

      this.hasDragged = false;
      this.input.setDefaultCursor('default');
    });
  }

  private handleNodeHover(pointer: Phaser.Input.Pointer) {
    // In capture mode, only show crosshair cursor - don't detect node hovers
    if (this.captureMode) {
      this.input.setDefaultCursor('crosshair');
      return;
    }

    const camera = this.cameras.main;

    // Use Phaser's built-in method to convert screen to world coordinates
    const worldPoint = camera.getWorldPoint(pointer.x, pointer.y);
    const worldX = worldPoint.x;
    const worldY = worldPoint.y;

    let hoveredNode: NetworkNode | null = null;
    this.gameState.nodes.forEach((node) => {
      if (this.fogOfWarEnabled && !node.explored) return;

      const radius = node.type === 'BASE' ? VISUAL_CONFIG.NODE_BASE_RADIUS * 1.5 : VISUAL_CONFIG.NODE_OWNED_RADIUS * 1.5;
      const distance = Math.sqrt(
        Math.pow(worldX - node.position.x, 2) + Math.pow(worldY - node.position.y, 2)
      );

      if (distance < radius) {
        hoveredNode = node;
      }
    });

    // Show pointer cursor when hovering over nodes (only when NOT in capture mode)
    if (hoveredNode) {
      this.input.setDefaultCursor('pointer');
    } else {
      this.input.setDefaultCursor('default');
    }
  }

  private handleNodeClick(pointer: Phaser.Input.Pointer) {
    const camera = this.cameras.main;

    // Use Phaser's built-in method to convert screen to world coordinates
    // This properly handles camera scroll, zoom, and rotation
    const worldPoint = camera.getWorldPoint(pointer.x, pointer.y);
    const worldX = worldPoint.x;
    const worldY = worldPoint.y;

    console.log('🖱️ Click at screen:', pointer.x.toFixed(1), pointer.y.toFixed(1),
                '→ world:', worldX.toFixed(1), worldY.toFixed(1),
                'Capture mode:', this.captureMode,
                'Zoom:', camera.zoom.toFixed(2),
                'Camera scroll:', `(${camera.scrollX.toFixed(1)}, ${camera.scrollY.toFixed(1)})`);

    // In capture mode, ONLY handle edge clicks - skip all node selection logic
    if (this.captureMode) {
      this.handleEdgeClick(worldX, worldY);
      return;
    }

    // Node selection logic (only runs when NOT in capture mode)
    let clickedNode: NetworkNode | null = null;
    let closestNode: NetworkNode | null = null;
    let closestDistance = Infinity;

    this.gameState.nodes.forEach((node) => {
      if (this.fogOfWarEnabled && !node.explored) return;

      // Use larger click radius for better UX (2.5x instead of 1.5x)
      const radius = node.type === 'BASE' ? VISUAL_CONFIG.NODE_BASE_RADIUS * 2.5 : VISUAL_CONFIG.NODE_OWNED_RADIUS * 2.5;
      const distance = Math.sqrt(
        Math.pow(worldX - node.position.x, 2) + Math.pow(worldY - node.position.y, 2)
      );

      // Track closest node for debugging
      if (distance < closestDistance) {
        closestDistance = distance;
        closestNode = node;
      }

      // Check if click is within node radius
      if (distance < radius && !clickedNode) {
        clickedNode = node;
      }
    });

    // Debug logging
    if (!clickedNode) {
      if (closestNode) {
        const closest: NetworkNode = closestNode;
        console.log('⚠️ No direct hit. Closest node:', closest.id,
                    'at position:', `(${closest.position.x.toFixed(1)}, ${closest.position.y.toFixed(1)})`,
                    'distance:', closestDistance.toFixed(1));
      } else {
        console.log('⚠️ No node found at click location');
      }
      return;
    }

    // Type narrowing: clickedNode is NetworkNode from this point
    const node: NetworkNode = clickedNode;
    console.log('🎯 Node clicked:', node.id,
                'at position:', `(${node.position.x.toFixed(1)}, ${node.position.y.toFixed(1)})`,
                'Type:', node.type, 'Owner:', node.ownerId);

    // Select the node
    this.selectedNode = node;
    this.events.emit('nodeSelected', node);

    // Visual feedback
    const radius = node.type === 'BASE' ? VISUAL_CONFIG.NODE_BASE_RADIUS * 1.5 : VISUAL_CONFIG.NODE_OWNED_RADIUS * 1.5;
    const pulseGraphics = this.add.graphics();
    pulseGraphics.lineStyle(3, COLORS.UI_PRIMARY, 1);
    pulseGraphics.strokeCircle(node.position.x, node.position.y, radius);

    this.tweens.add({
      targets: pulseGraphics,
      alpha: 0,
      scaleX: 2,
      scaleY: 2,
      duration: 500,
      onComplete: () => pulseGraphics.destroy(),
    });

    // Update capturable connections list
    this.updateCapturableConnections();
  }

  /**
   * Calculate distance from point to line segment
   */
  private distanceToLineSegment(
    px: number, py: number,
    x1: number, y1: number,
    x2: number, y2: number
  ): number {
    const dx = x2 - x1;
    const dy = y2 - y1;
    const lengthSquared = dx * dx + dy * dy;

    if (lengthSquared === 0) {
      // Line segment is a point
      return Math.sqrt((px - x1) * (px - x1) + (py - y1) * (py - y1));
    }

    // Calculate projection of point onto line segment
    const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / lengthSquared));
    const projX = x1 + t * dx;
    const projY = y1 + t * dy;

    // Return distance from point to projection
    return Math.sqrt((px - projX) * (px - projX) + (py - projY) * (py - projY));
  }

  /**
   * Handle clicking on edges in capture mode
   */
  private handleEdgeClick(worldX: number, worldY: number) {
    const clickThreshold = 15; // pixels

    // Find the closest capturable connection
    let closestConnection: { sourceNodeId: string; targetNodeId: string } | null = null;
    let closestDistance = Infinity;

    this.capturableConnections.forEach((conn) => {
      const sourceNode = this.gameState.nodes.get(conn.sourceNodeId);
      const targetNode = this.gameState.nodes.get(conn.targetNodeId);

      if (!sourceNode || !targetNode) return;

      // Calculate distance from click to this edge
      const distance = this.distanceToLineSegment(
        worldX, worldY,
        sourceNode.position.x, sourceNode.position.y,
        targetNode.position.x, targetNode.position.y
      );

      if (distance < closestDistance) {
        closestDistance = distance;
        closestConnection = conn;
      }
    });

    // Debug logging and validation
    if (closestDistance > clickThreshold) {
      console.log('⚠️ No capturable edge clicked. Closest distance:', closestDistance.toFixed(1));
      return;
    }

    if (!closestConnection) {
      console.log('⚠️ No capturable connection found');
      return;
    }

    // Extract connection details
    const { sourceNodeId, targetNodeId } = closestConnection;
    const sourceNode = this.gameState.nodes.get(sourceNodeId);
    const targetNode = this.gameState.nodes.get(targetNodeId);

    console.log('🎯 Edge clicked:', sourceNodeId, '→', targetNodeId,
                'Distance:', closestDistance.toFixed(1));
    console.log('   Source:', sourceNode?.id, 'at', `(${sourceNode?.position.x.toFixed(1)}, ${sourceNode?.position.y.toFixed(1)})`);
    console.log('   Target:', targetNode?.id, 'at', `(${targetNode?.position.x.toFixed(1)}, ${targetNode?.position.y.toFixed(1)})`);

    // Attempt to capture via this edge
    this.attemptCaptureViaEdge(sourceNodeId, targetNodeId);
  }

  /**
   * Attempt to capture a node via an edge connection
   */
  private async attemptCaptureViaEdge(sourceNodeId: string, targetNodeId: string) {
    const playerId = this.gameState.currentPlayerId;
    const sourceNode = this.gameState.nodes.get(sourceNodeId);
    const targetNode = this.gameState.nodes.get(targetNodeId);

    if (!sourceNode || !targetNode) {
      this.events.emit('captureAttempted', {
        success: false,
        message: 'Node not found',
      });
      return;
    }

    // Check if connection is capturable
    if (!isCapturableConnection(this.gameState, sourceNodeId, targetNodeId, playerId)) {
      this.events.emit('captureAttempted', {
        success: false,
        message: 'Cannot capture via this connection',
      });
      return;
    }

    // If backend is enabled and nodes have hex coordinates, submit attack to backend
    if (this.backendConnected && sourceNode.hexCoord && targetNode.hexCoord) {
      try {
        const { setAttackTarget } = await import('../services/backendApi');
        await setAttackTarget(sourceNode.hexCoord, targetNode.hexCoord);

        debugLog('Attack command sent to backend:', sourceNode.hexCoord, '→', targetNode.hexCoord);

        this.events.emit('captureAttempted', {
          success: true,
          message: `Attacking ${targetNodeId} from ${sourceNodeId}`,
        });

        // Don't update local state - let backend update come through
        return;
      } catch (error) {
        errorLog('Failed to send attack command:', error);
        // Fall through to local update
      }
    }

    // Fallback: local simulation (for offline mode or if backend call failed)
    const success = initiateCaptureViaEdge(this.gameState, sourceNodeId, targetNodeId, playerId);

    if (success) {
      this.events.emit('captureAttempted', {
        success: true,
        message: `Building connection to capture ${targetNode.id}`,
      });

      // Update visuals immediately
      this.updateCapturableConnections();
      this.drawNodes();
      this.drawEdges();
      this.drawConnections();
    } else {
      this.events.emit('captureAttempted', {
        success: false,
        message: 'Failed to initiate capture',
      });
    }
  }

  private updateCapturableNodes() {
    this.capturableNodes = getCapturableNodes(this.gameState, this.gameState.currentPlayerId);
  }

  private updateCapturableConnections() {
    this.capturableConnections = getCapturableConnections(this.gameState, this.gameState.currentPlayerId);
  }

  private attemptCapture(node: NetworkNode) {
    const playerId = this.gameState.currentPlayerId;

    // Check if node can be captured
    if (!canCaptureNode(this.gameState, node.id, playerId)) {
      // Determine why it can't be captured
      let reason = 'Cannot capture this node';

      if (!node.explored) {
        reason = 'Cannot capture this node - not explored yet';
      } else if (node.ownerId === playerId) {
        reason = 'Cannot capture this node - you already own it';
      } else if (node.state === 'CAPTURING') {
        reason = 'Cannot capture this node - already being captured';
      } else {
        reason = 'Cannot capture this node - must be adjacent to your territory';
      }

      this.events.emit('captureAttempted', {
        success: false,
        message: reason,
      });
      return;
    }

    // Attempt to initiate capture
    const success = initiateCapture(this.gameState, node.id, playerId);

    if (success) {
      this.events.emit('captureAttempted', {
        success: true,
        message: `Initiating capture of node ${node.id}`,
      });

      // Update visuals immediately
      this.updateCapturableNodes();
      this.drawNodes();
      this.drawEdges();
    } else {
      this.events.emit('captureAttempted', {
        success: false,
        message: 'Failed to initiate capture',
      });
    }
  }

  private drawConnections() {
    this.connectionGraphics.clear();

    // Draw all potential connections as dim lines
    this.gameState.nodes.forEach((node) => {
      if (this.fogOfWarEnabled && !node.explored) return;

      node.connections.forEach((connectedId) => {
        const connectedNode = this.gameState.nodes.get(connectedId);
        if (!connectedNode) return;
        if (this.fogOfWarEnabled && !connectedNode.explored) return;

        // Only draw if not an active edge
        const hasActiveEdge = Array.from(this.gameState.edges.values()).some(
          (edge) =>
            (edge.sourceNodeId === node.id && edge.targetNodeId === connectedId) ||
            (edge.sourceNodeId === connectedId && edge.targetNodeId === node.id)
        );

        if (!hasActiveEdge) {
          // Check if this connection is capturable (in capture mode)
          const isCapturable = this.captureMode && this.capturableConnections.some(
            (conn) =>
              (conn.sourceNodeId === node.id && conn.targetNodeId === connectedId) ||
              (conn.sourceNodeId === connectedId && conn.targetNodeId === node.id)
          );

          if (isCapturable) {
            // Highlight capturable connections
            const time = this.time.now;
            const pulse = Math.sin((time / 500) * Math.PI * 2) * 0.3 + 0.7;
            this.connectionGraphics.lineStyle(3, COLORS.UI_WARNING, 0.6 * pulse);
            this.connectionGraphics.lineBetween(
              node.position.x,
              node.position.y,
              connectedNode.position.x,
              connectedNode.position.y
            );
          } else {
            // Normal dim connection
            this.connectionGraphics.lineStyle(1, COLORS.GRID_SECONDARY, 0.2);
            this.connectionGraphics.lineBetween(
              node.position.x,
              node.position.y,
              connectedNode.position.x,
              connectedNode.position.y
            );
          }
        }
      });
    });
  }

  private drawEdges() {
    this.edgeGraphics.clear();

    this.gameState.edges.forEach((edge) => {
      const sourceNode = this.gameState.nodes.get(edge.sourceNodeId);
      const targetNode = this.gameState.nodes.get(edge.targetNodeId);

      if (!sourceNode || !targetNode) return;
      if (this.fogOfWarEnabled && (!sourceNode.explored || !targetNode.explored)) {
        return;
      }

      const utilization = edge.bandwidth / edge.maxBandwidth;
      const width =
        VISUAL_CONFIG.STREAM_MIN_WIDTH +
        (VISUAL_CONFIG.STREAM_MAX_WIDTH - VISUAL_CONFIG.STREAM_MIN_WIDTH) * utilization;

      const color = getBandwidthColor(utilization);

      // Draw glow
      this.edgeGraphics.lineStyle(width * 2, color, 0.3);
      this.edgeGraphics.lineBetween(
        sourceNode.position.x,
        sourceNode.position.y,
        targetNode.position.x,
        targetNode.position.y
      );

      // Draw main line
      this.edgeGraphics.lineStyle(width, color, 0.8);
      this.edgeGraphics.lineBetween(
        sourceNode.position.x,
        sourceNode.position.y,
        targetNode.position.x,
        targetNode.position.y
      );

      // Create or update particles
      if (!this.particleEmitters.has(edge.id)) {
        this.createEdgeParticles(edge, sourceNode.position, targetNode.position, color);
      } else {
        this.updateEdgeParticles(edge, sourceNode.position, targetNode.position);
      }
    });

    // Clean up removed edges
    this.particleEmitters.forEach((emitter, edgeId) => {
      if (!this.gameState.edges.has(edgeId)) {
        emitter.stop();
        emitter.destroy();
        this.particleEmitters.delete(edgeId);
      }
    });
  }

  private createEdgeParticles(
    edge: NetworkEdge,
    sourcePos: { x: number; y: number },
    targetPos: { x: number; y: number },
    color: number
  ) {
    const distance = Math.sqrt(
      Math.pow(targetPos.x - sourcePos.x, 2) + Math.pow(targetPos.y - sourcePos.y, 2)
    );

    const utilization = edge.bandwidth / edge.maxBandwidth;
    const particleSpeed = VISUAL_CONFIG.PARTICLE_SPEED * (0.5 + utilization * 0.5);

    const emitter = this.add.particles(sourcePos.x, sourcePos.y, 'particle', {
      lifespan: (distance / particleSpeed) * 1000,
      speed: { min: particleSpeed * 0.9, max: particleSpeed * 1.1 },
      scale: { start: VISUAL_CONFIG.PARTICLE_SIZE / 32, end: 0 },
      alpha: { start: 0.8, end: 0 },
      frequency: 1000 / VISUAL_CONFIG.PARTICLES_PER_SECOND,
      blendMode: 'ADD',
      tint: color,
      emitCallback: (particle: Phaser.GameObjects.Particles.Particle) => {
        const angle = Math.atan2(targetPos.y - sourcePos.y, targetPos.x - sourcePos.x);
        particle.velocityX = Math.cos(angle) * particleSpeed;
        particle.velocityY = Math.sin(angle) * particleSpeed;
      },
    });

    this.particleEmitters.set(edge.id, emitter);
  }

  private updateEdgeParticles(
    edge: NetworkEdge,
    sourcePos: { x: number; y: number },
    _targetPos: { x: number; y: number }
  ) {
    const emitter = this.particleEmitters.get(edge.id);
    if (!emitter) return;

    emitter.setPosition(sourcePos.x, sourcePos.y);

    const utilization = edge.bandwidth / edge.maxBandwidth;
    const frequency = 1000 / (VISUAL_CONFIG.PARTICLES_PER_SECOND * (0.5 + utilization * 0.5));
    emitter.setFrequency(frequency);
  }

  private drawNodes() {
    this.nodeGraphics.clear();

    this.gameState.nodes.forEach((node) => {
      if (this.fogOfWarEnabled && !node.explored) return;

      const time = this.time.now;
      const pulse = Math.sin((time / VISUAL_CONFIG.PULSE_SPEED) * Math.PI * 2) * 0.3 + 0.7;

      // Check if this node is capturable
      const isCapturable = this.capturableNodes.some((n) => n.id === node.id);

      if (node.ownerId !== null) {
        // Player-owned node
        const color = getPlayerColor(node.ownerId);
        const radius = node.type === 'BASE' ? VISUAL_CONFIG.NODE_BASE_RADIUS : VISUAL_CONFIG.NODE_OWNED_RADIUS;

        // Outer glow
        this.nodeGraphics.fillStyle(color, 0.3 * pulse);
        this.nodeGraphics.fillCircle(node.position.x, node.position.y, radius * 1.5);

        // Main node
        this.nodeGraphics.fillStyle(color, 0.8);
        this.nodeGraphics.fillCircle(node.position.x, node.position.y, radius);

        // Inner core
        this.nodeGraphics.fillStyle(0xffffff, 0.6 * pulse);
        this.nodeGraphics.fillCircle(node.position.x, node.position.y, radius * 0.5);

        // Base node special effects
        if (node.type === 'BASE') {
          const ringRadius = radius * 1.3;
          const rotation = (time / 3000) * Math.PI * 2;
          this.nodeGraphics.lineStyle(3, color, 0.8);
          this.nodeGraphics.beginPath();
          this.nodeGraphics.arc(node.position.x, node.position.y, ringRadius, rotation, rotation + Math.PI * 1.5);
          this.nodeGraphics.strokePath();
        }

        // Capture progress
        if (node.state === 'CAPTURING' && node.captureProgress > 0) {
          const progressRadius = radius * 1.8;
          const progressAngle = node.captureProgress * Math.PI * 2;
          this.nodeGraphics.lineStyle(3, COLORS.UI_WARNING, 0.9);
          this.nodeGraphics.beginPath();
          this.nodeGraphics.arc(
            node.position.x,
            node.position.y,
            progressRadius,
            -Math.PI / 2,
            -Math.PI / 2 + progressAngle
          );
          this.nodeGraphics.strokePath();
        }
      } else {
        // Neutral node
        const radius = VISUAL_CONFIG.NODE_NEUTRAL_RADIUS;

        // Highlight capturable nodes
        if (isCapturable) {
          this.nodeGraphics.fillStyle(COLORS.UI_WARNING, 0.3 * pulse);
          this.nodeGraphics.fillCircle(node.position.x, node.position.y, radius * 2.5);

          this.nodeGraphics.lineStyle(2, COLORS.UI_WARNING, 0.8 * pulse);
          this.nodeGraphics.strokeCircle(node.position.x, node.position.y, radius * 2);
        }

        this.nodeGraphics.fillStyle(COLORS.NEUTRAL_GLOW, 0.2 * pulse);
        this.nodeGraphics.fillCircle(node.position.x, node.position.y, radius * 1.5);

        this.nodeGraphics.fillStyle(COLORS.NEUTRAL, 0.6);
        this.nodeGraphics.fillCircle(node.position.x, node.position.y, radius);

        this.nodeGraphics.fillStyle(COLORS.NEUTRAL_GLOW, 0.4);
        this.nodeGraphics.fillCircle(node.position.x, node.position.y, radius * 0.4);
      }
    });

    // Draw node labels (for debugging)
    this.drawNodeLabels();
  }

  private drawNodeLabels() {
    // Clear existing labels
    this.nodeLabelContainer.removeAll(true);

    this.gameState.nodes.forEach((node) => {
      if (this.fogOfWarEnabled && !node.explored) return;

      // Create text label for node ID
      const label = this.add.text(node.position.x, node.position.y, node.id, {
        fontSize: '10px',
        fontFamily: 'monospace',
        color: '#ffffff',
        backgroundColor: '#000000',
        padding: { x: 2, y: 1 },
      });
      label.setOrigin(0.5, 0.5);
      label.setAlpha(0.7);

      this.nodeLabelContainer.add(label);
    });
  }

  private drawFogOfWar() {
    this.fogGraphics.clear();

    // Only draw fog if fog of war is enabled
    if (!this.fogOfWarEnabled) return;

    // Draw fog as large circles over unexplored nodes
    this.gameState.nodes.forEach((node) => {
      if (!node.explored) {
        this.fogGraphics.fillStyle(COLORS.BACKGROUND, VISUAL_CONFIG.FOG_ALPHA);
        this.fogGraphics.fillCircle(node.position.x, node.position.y, 100);
      }
    });
  }

  private async updateGameState() {
    if (this.useBackend) {
      // Fetch from backend
      try {
        const backendState = await fetchGameState();
        this.gameState = transformBackendToGraph(backendState, this.currentPlayerId);
      } catch (error) {
        errorLog('Failed to update game state from backend', error);
        // Fall back to dummy data on error
        this.useBackend = false;
        this.gameState = generateNetworkGameState();
      }
    } else {
      // Update dummy data
      updateNetworkGameState(this.gameState);
    }

    this.updateCapturableNodes();
    this.updateCapturableConnections();

    // Redraw connections if in capture mode (for pulsing effect)
    if (this.captureMode) {
      this.drawConnections();
    }

    this.drawNodes();
    this.drawEdges();
    this.drawFogOfWar();

    this.events.emit('gameStateUpdated', this.gameState);
  }

  update() {
    if (!this.cursors) return; // Wait for initialization to complete

    const camera = this.cameras.main;
    const speed = VISUAL_CONFIG.CAMERA_PAN_SPEED / 60;

    // Arrow key panning
    if (this.cursors.left.isDown) camera.scrollX -= speed / camera.zoom;
    if (this.cursors.right.isDown) camera.scrollX += speed / camera.zoom;
    if (this.cursors.up.isDown) camera.scrollY -= speed / camera.zoom;
    if (this.cursors.down.isDown) camera.scrollY += speed / camera.zoom;

    // Keyboard zoom (+/- keys)
    if (Phaser.Input.Keyboard.JustDown(this.zoomKeys.plus)) {
      const newZoom = Phaser.Math.Clamp(
        camera.zoom + VISUAL_CONFIG.CAMERA_ZOOM_SPEED,
        VISUAL_CONFIG.CAMERA_ZOOM_MIN,
        VISUAL_CONFIG.CAMERA_ZOOM_MAX
      );
      camera.setZoom(newZoom);
    }
    if (Phaser.Input.Keyboard.JustDown(this.zoomKeys.minus)) {
      const newZoom = Phaser.Math.Clamp(
        camera.zoom - VISUAL_CONFIG.CAMERA_ZOOM_SPEED,
        VISUAL_CONFIG.CAMERA_ZOOM_MIN,
        VISUAL_CONFIG.CAMERA_ZOOM_MAX
      );
      camera.setZoom(newZoom);
    }

    // Toggle fog of war (F key)
    if (Phaser.Input.Keyboard.JustDown(this.fogToggleKey)) {
      this.fogOfWarEnabled = !this.fogOfWarEnabled;
      console.log(`🌫️ Fog of War: ${this.fogOfWarEnabled ? 'ENABLED' : 'DISABLED'}`);

      // Immediately redraw the scene
      this.drawConnections();
      this.drawEdges();
      this.drawNodes();
      this.drawFogOfWar();
    }
  }

  getGameState(): NetworkGameState {
    return this.gameState;
  }
}
