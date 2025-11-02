/**
 * GameScene - Main game visualization with TRON-style hex grid
 */

import Phaser from 'phaser';
import type { GameState, GameNode, BandwidthStream } from '../types/gameTypes';
import { generateDummyGameState, updateDummyGameState } from '../utils/dummyData';
import { hexToPixel, getHexVertices } from '../utils/hexUtils';
import { COLORS, VISUAL_CONFIG, getPlayerColor, getBandwidthColor } from '../config/visualConstants';
import { fetchGameState, pingBackend } from '../services/backendApi';
import { transformBackendToFrontend } from '../adapters/backendAdapter';
import { getBackendConfig, isBackendEnabled, debugLog, errorLog } from '../config/backend';

export class GameScene extends Phaser.Scene {
  private gameState!: GameState;
  private hexGraphics!: Phaser.GameObjects.Graphics;
  private nodeGraphics!: Phaser.GameObjects.Graphics;
  private streamGraphics!: Phaser.GameObjects.Graphics;
  private fogGraphics!: Phaser.GameObjects.Graphics;
  private particleEmitters: Map<string, Phaser.GameObjects.Particles.ParticleEmitter> = new Map();

  // Camera controls
  private cursors!: Phaser.Types.Input.Keyboard.CursorKeys;
  private isDragging = false;
  private dragStartX = 0;
  private dragStartY = 0;

  // Backend integration
  private useBackend = false;
  private backendConnected = false;
  private currentPlayerId = 0; // Default to player 0, can be set from URL params

  constructor() {
    super({ key: 'GameScene' });
  }

  preload() {
    // Create particle texture programmatically
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
        this.gameState = transformBackendToFrontend(backendState, this.currentPlayerId);
        debugLog('Loaded game state from backend:', this.gameState);
      } catch (error) {
        errorLog('Failed to fetch initial game state', error);
        this.gameState = generateDummyGameState();
        this.useBackend = false;
      }
    } else {
      this.gameState = generateDummyGameState();
    }

    // Set up graphics layers
    this.hexGraphics = this.add.graphics();
    this.streamGraphics = this.add.graphics();
    this.nodeGraphics = this.add.graphics();
    this.fogGraphics = this.add.graphics();

    // Set up camera
    this.setupCamera();

    // Set up input controls
    this.setupControls();

    // Draw initial state
    this.drawHexGrid();
    this.drawStreams();
    this.drawNodes();
    this.drawFogOfWar();

    // Note: UIScene is not used in hex grid mode (only in GraphGameScene)
    // Hex grid has all UI elements built-in

    // Start update loop for animations
    const config = getBackendConfig();
    const updateDelay = this.useBackend ? config.pollingInterval : 100;
    this.time.addEvent({
      delay: updateDelay,
      callback: this.updateGameState,
      callbackScope: this,
      loop: true,
    });

    console.log('GameScene initialized with', this.gameState.nodes.size, 'nodes');
    console.log('Backend mode:', this.useBackend ? 'ENABLED' : 'DISABLED (dummy data)');
  }

  private setupCamera() {
    const camera = this.cameras.main;
    camera.setBackgroundColor(COLORS.BACKGROUND);
    camera.setZoom(1);

    // Enable camera bounds for infinite scrolling feel
    // We'll set it large enough for our hex grid
    camera.setBounds(-2000, -2000, 4000, 4000);

    // Set up keyboard controls
    this.cursors = this.input.keyboard!.createCursorKeys();

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
    // Mouse drag to pan
    this.input.on('pointerdown', (pointer: Phaser.Input.Pointer) => {
      if (pointer.rightButtonDown()) {
        this.isDragging = true;
        this.dragStartX = pointer.x;
        this.dragStartY = pointer.y;
      } else if (pointer.leftButtonDown()) {
        // Handle node selection
        this.handleNodeClick(pointer);
      }
    });

    this.input.on('pointermove', (pointer: Phaser.Input.Pointer) => {
      if (this.isDragging) {
        const camera = this.cameras.main;
        const deltaX = (this.dragStartX - pointer.x) / camera.zoom;
        const deltaY = (this.dragStartY - pointer.y) / camera.zoom;
        camera.scrollX += deltaX;
        camera.scrollY += deltaY;
        this.dragStartX = pointer.x;
        this.dragStartY = pointer.y;
      } else {
        // Handle node hover
        this.handleNodeHover(pointer);
      }
    });

    this.input.on('pointerup', (pointer: Phaser.Input.Pointer) => {
      if (pointer.rightButtonReleased()) {
        this.isDragging = false;
      }
    });
  }

  private handleNodeHover(pointer: Phaser.Input.Pointer) {
    const camera = this.cameras.main;
    const worldX = camera.scrollX + (pointer.x - camera.width / 2) / camera.zoom;
    const worldY = camera.scrollY + (pointer.y - camera.height / 2) / camera.zoom;

    // Find node under pointer
    let hoveredNode: GameNode | null = null;
    this.gameState.nodes.forEach((node) => {
      if (!node.explored) return;

      const pixelPos = hexToPixel(node.position);
      const radius = node.type === 'BASE' ? VISUAL_CONFIG.NODE_BASE_RADIUS * 1.5 : VISUAL_CONFIG.NODE_OWNED_RADIUS * 1.5;
      const distance = Math.sqrt(
        Math.pow(worldX - pixelPos.x, 2) + Math.pow(worldY - pixelPos.y, 2)
      );

      if (distance < radius) {
        hoveredNode = node;
      }
    });

    // Update cursor
    if (hoveredNode) {
      this.input.setDefaultCursor('pointer');
    } else {
      this.input.setDefaultCursor('default');
    }
  }

  private handleNodeClick(pointer: Phaser.Input.Pointer) {
    const camera = this.cameras.main;
    const worldX = camera.scrollX + (pointer.x - camera.width / 2) / camera.zoom;
    const worldY = camera.scrollY + (pointer.y - camera.height / 2) / camera.zoom;

    // Find clicked node
    this.gameState.nodes.forEach((node) => {
      if (!node.explored) return;

      const pixelPos = hexToPixel(node.position);
      const radius = node.type === 'BASE' ? VISUAL_CONFIG.NODE_BASE_RADIUS * 1.5 : VISUAL_CONFIG.NODE_OWNED_RADIUS * 1.5;
      const distance = Math.sqrt(
        Math.pow(worldX - pixelPos.x, 2) + Math.pow(worldY - pixelPos.y, 2)
      );

      if (distance < radius) {
        console.log('Node clicked:', node);
        this.events.emit('nodeSelected', node);

        // Visual feedback - pulse effect
        const pulseGraphics = this.add.graphics();
        pulseGraphics.lineStyle(3, COLORS.UI_PRIMARY, 1);
        pulseGraphics.strokeCircle(pixelPos.x, pixelPos.y, radius);

        this.tweens.add({
          targets: pulseGraphics,
          alpha: 0,
          scaleX: 2,
          scaleY: 2,
          duration: 500,
          onComplete: () => {
            pulseGraphics.destroy();
          },
        });
      }
    });
  }

  private drawHexGrid() {
    this.hexGraphics.clear();

    // Draw hex grid for all nodes
    this.gameState.nodes.forEach((node) => {
      if (!node.explored) return; // Don't draw unexplored hexes

      const pixelPos = hexToPixel(node.position);
      const vertices = getHexVertices(pixelPos);

      // Draw hex outline with glow effect
      // Outer glow
      this.hexGraphics.lineStyle(
        VISUAL_CONFIG.HEX_GLOW_WIDTH,
        COLORS.GRID_GLOW,
        VISUAL_CONFIG.GRID_GLOW_ALPHA
      );
      this.hexGraphics.beginPath();
      this.hexGraphics.moveTo(vertices[0].x, vertices[0].y);
      for (let i = 1; i < vertices.length; i++) {
        this.hexGraphics.lineTo(vertices[i].x, vertices[i].y);
      }
      this.hexGraphics.closePath();
      this.hexGraphics.strokePath();

      // Inner line
      this.hexGraphics.lineStyle(
        VISUAL_CONFIG.HEX_LINE_WIDTH,
        COLORS.GRID_PRIMARY,
        VISUAL_CONFIG.GRID_ALPHA
      );
      this.hexGraphics.beginPath();
      this.hexGraphics.moveTo(vertices[0].x, vertices[0].y);
      for (let i = 1; i < vertices.length; i++) {
        this.hexGraphics.lineTo(vertices[i].x, vertices[i].y);
      }
      this.hexGraphics.closePath();
      this.hexGraphics.strokePath();
    });
  }

  private drawNodes() {
    this.nodeGraphics.clear();

    this.gameState.nodes.forEach((node) => {
      if (!node.explored) return;

      const pixelPos = hexToPixel(node.position);
      const time = this.time.now;
      const pulse = Math.sin(time / VISUAL_CONFIG.PULSE_SPEED * Math.PI * 2) * 0.3 + 0.7;

      if (node.ownerId !== null) {
        // Player-owned or base node
        const color = getPlayerColor(node.ownerId);
        const radius = node.type === 'BASE' ? VISUAL_CONFIG.NODE_BASE_RADIUS : VISUAL_CONFIG.NODE_OWNED_RADIUS;

        // Outer glow
        this.nodeGraphics.fillStyle(color, 0.3 * pulse);
        this.nodeGraphics.fillCircle(pixelPos.x, pixelPos.y, radius * 1.5);

        // Main node
        this.nodeGraphics.fillStyle(color, 0.8);
        this.nodeGraphics.fillCircle(pixelPos.x, pixelPos.y, radius);

        // Inner core
        this.nodeGraphics.fillStyle(0xffffff, 0.6 * pulse);
        this.nodeGraphics.fillCircle(pixelPos.x, pixelPos.y, radius * 0.5);

        // For base nodes, add rotating ring
        if (node.type === 'BASE') {
          const ringRadius = radius * 1.2;
          const rotation = (time / 3000) * Math.PI * 2;
          this.nodeGraphics.lineStyle(2, color, 0.8);

          // Draw partial ring (arc)
          this.nodeGraphics.beginPath();
          this.nodeGraphics.arc(pixelPos.x, pixelPos.y, ringRadius, rotation, rotation + Math.PI * 1.5);
          this.nodeGraphics.strokePath();
        }

        // Capture progress indicator
        if (node.state === 'CAPTURING' && node.captureProgress > 0) {
          const progressRadius = radius * 1.8;
          const progressAngle = node.captureProgress * Math.PI * 2;

          this.nodeGraphics.lineStyle(3, COLORS.UI_WARNING, 0.9);
          this.nodeGraphics.beginPath();
          this.nodeGraphics.arc(pixelPos.x, pixelPos.y, progressRadius, -Math.PI / 2, -Math.PI / 2 + progressAngle);
          this.nodeGraphics.strokePath();
        }
      } else {
        // Neutral node
        const radius = VISUAL_CONFIG.NODE_NEUTRAL_RADIUS;

        // Outer glow
        this.nodeGraphics.fillStyle(COLORS.NEUTRAL_GLOW, 0.2 * pulse);
        this.nodeGraphics.fillCircle(pixelPos.x, pixelPos.y, radius * 1.5);

        // Main node
        this.nodeGraphics.fillStyle(COLORS.NEUTRAL, 0.6);
        this.nodeGraphics.fillCircle(pixelPos.x, pixelPos.y, radius);

        // Inner dot
        this.nodeGraphics.fillStyle(COLORS.NEUTRAL_GLOW, 0.4);
        this.nodeGraphics.fillCircle(pixelPos.x, pixelPos.y, radius * 0.4);
      }
    });
  }

  private drawStreams() {
    this.streamGraphics.clear();

    this.gameState.streams.forEach((stream) => {
      const sourceNode = this.gameState.nodes.get(stream.sourceNodeId);
      const targetNode = this.gameState.nodes.get(stream.targetNodeId);

      if (!sourceNode || !targetNode || !sourceNode.explored || !targetNode.explored) {
        return;
      }

      const sourcePos = hexToPixel(sourceNode.position);
      const targetPos = hexToPixel(targetNode.position);

      // Calculate stream width based on bandwidth
      const utilization = stream.bandwidth / stream.maxBandwidth;
      const width = VISUAL_CONFIG.STREAM_MIN_WIDTH +
        (VISUAL_CONFIG.STREAM_MAX_WIDTH - VISUAL_CONFIG.STREAM_MIN_WIDTH) * utilization;

      const color = getBandwidthColor(utilization);

      // Draw outer glow
      this.streamGraphics.lineStyle(width * 2, color, 0.3);
      this.streamGraphics.lineBetween(sourcePos.x, sourcePos.y, targetPos.x, targetPos.y);

      // Draw main stream
      this.streamGraphics.lineStyle(width, color, 0.8);
      this.streamGraphics.lineBetween(sourcePos.x, sourcePos.y, targetPos.x, targetPos.y);

      // Create or update particle emitter for this stream
      if (!this.particleEmitters.has(stream.id)) {
        this.createStreamParticles(stream, sourcePos, targetPos, color);
      } else {
        this.updateStreamParticles(stream, sourcePos, targetPos, color);
      }
    });

    // Clean up particle emitters for removed streams
    this.particleEmitters.forEach((emitter, streamId) => {
      if (!this.gameState.streams.has(streamId)) {
        emitter.stop();
        emitter.destroy();
        this.particleEmitters.delete(streamId);
      }
    });
  }

  private createStreamParticles(
    stream: BandwidthStream,
    sourcePos: { x: number; y: number },
    targetPos: { x: number; y: number },
    color: number
  ) {
    const distance = Math.sqrt(
      Math.pow(targetPos.x - sourcePos.x, 2) + Math.pow(targetPos.y - sourcePos.y, 2)
    );

    const utilization = stream.bandwidth / stream.maxBandwidth;
    const particleSpeed = VISUAL_CONFIG.PARTICLE_SPEED * (0.5 + utilization * 0.5);

    // Create particle emitter
    const emitter = this.add.particles(sourcePos.x, sourcePos.y, 'particle', {
      lifespan: (distance / particleSpeed) * 1000,
      speed: { min: particleSpeed * 0.9, max: particleSpeed * 1.1 },
      scale: { start: VISUAL_CONFIG.PARTICLE_SIZE / 32, end: 0 },
      alpha: { start: 0.8, end: 0 },
      frequency: 1000 / VISUAL_CONFIG.PARTICLES_PER_SECOND,
      blendMode: 'ADD',
      tint: color,
      emitCallback: (particle: Phaser.GameObjects.Particles.Particle) => {
        // Calculate direction to target
        const angle = Math.atan2(targetPos.y - sourcePos.y, targetPos.x - sourcePos.x);
        particle.velocityX = Math.cos(angle) * particleSpeed;
        particle.velocityY = Math.sin(angle) * particleSpeed;
      },
    });

    this.particleEmitters.set(stream.id, emitter);
  }

  private updateStreamParticles(
    stream: BandwidthStream,
    sourcePos: { x: number; y: number },
    _targetPos: { x: number; y: number },
    _color: number
  ) {
    const emitter = this.particleEmitters.get(stream.id);
    if (!emitter) return;

    // Update emitter position
    emitter.setPosition(sourcePos.x, sourcePos.y);

    // Note: Color updates would require recreating the emitter in Phaser
    // Keeping emitters with their original color for performance

    // Update emission rate based on utilization
    const utilization = stream.bandwidth / stream.maxBandwidth;
    const frequency = 1000 / (VISUAL_CONFIG.PARTICLES_PER_SECOND * (0.5 + utilization * 0.5));
    emitter.setFrequency(frequency);
  }

  private drawFogOfWar() {
    this.fogGraphics.clear();

    // Draw fog over unexplored hexes
    this.gameState.nodes.forEach((node) => {
      if (!node.explored) {
        const pixelPos = hexToPixel(node.position);
        const vertices = getHexVertices(pixelPos);

        this.fogGraphics.fillStyle(COLORS.BACKGROUND, VISUAL_CONFIG.FOG_ALPHA);
        this.fogGraphics.beginPath();
        this.fogGraphics.moveTo(vertices[0].x, vertices[0].y);
        for (let i = 1; i < vertices.length; i++) {
          this.fogGraphics.lineTo(vertices[i].x, vertices[i].y);
        }
        this.fogGraphics.closePath();
        this.fogGraphics.fillPath();

        // Add subtle grid pattern to fog
        this.fogGraphics.lineStyle(1, COLORS.GRID_DARK, 0.3);
        this.fogGraphics.beginPath();
        this.fogGraphics.moveTo(vertices[0].x, vertices[0].y);
        for (let i = 1; i < vertices.length; i++) {
          this.fogGraphics.lineTo(vertices[i].x, vertices[i].y);
        }
        this.fogGraphics.closePath();
        this.fogGraphics.strokePath();
      }
    });
  }

  private async updateGameState() {
    if (this.useBackend) {
      // Fetch from backend
      try {
        const backendState = await fetchGameState();
        this.gameState = transformBackendToFrontend(backendState, this.currentPlayerId);
      } catch (error) {
        errorLog('Failed to update game state from backend', error);
        // Fall back to dummy data on error
        this.useBackend = false;
        this.gameState = generateDummyGameState();
      }
    } else {
      // Update dummy data
      updateDummyGameState(this.gameState);
    }

    // Redraw dynamic elements
    this.drawNodes();
    this.drawStreams();
    this.drawFogOfWar();

    // Emit event for UI to update
    this.events.emit('gameStateUpdated', this.gameState);
  }

  update() {
    // Camera keyboard controls
    if (!this.cursors) return; // Wait for initialization to complete

    const camera = this.cameras.main;
    const speed = VISUAL_CONFIG.CAMERA_PAN_SPEED / 60; // Per frame

    if (this.cursors.left.isDown) {
      camera.scrollX -= speed / camera.zoom;
    }
    if (this.cursors.right.isDown) {
      camera.scrollX += speed / camera.zoom;
    }
    if (this.cursors.up.isDown) {
      camera.scrollY -= speed / camera.zoom;
    }
    if (this.cursors.down.isDown) {
      camera.scrollY += speed / camera.zoom;
    }
  }

  // Public method to access game state (for UI scene)
  getGameState(): GameState {
    return this.gameState;
  }

  // Public method to check if backend is being used
  isUsingBackend(): boolean {
    return this.useBackend;
  }

  // Public method to get current player ID
  getCurrentPlayerId(): number {
    return this.currentPlayerId;
  }
}
