/**
 * TRON-inspired Visual Constants
 * Neon grid aesthetic for network warfare visualization
 */

export const COLORS = {
  // Background - Deep digital void
  BACKGROUND: 0x0a0a1a,
  GRID_DARK: 0x1a1a2e,

  // TRON Grid Lines - Cyan/Blue glow
  GRID_PRIMARY: 0x00d4ff,
  GRID_SECONDARY: 0x0066cc,
  GRID_GLOW: 0x4dd9ff,

  // Player Colors - Distinct neon hues
  PLAYER_1: 0x00ffff,      // Cyan (TRON blue)
  PLAYER_2: 0xff006e,      // Hot Pink/Magenta (opponent)
  PLAYER_3: 0x00ff88,      // Neon Green
  PLAYER_4: 0xffaa00,      // Orange

  // Neutral Elements
  NEUTRAL: 0x666699,
  NEUTRAL_GLOW: 0x9999cc,

  // Network States
  BANDWIDTH_LOW: 0x00ffff,     // Cyan - healthy
  BANDWIDTH_MED: 0xffff00,     // Yellow - moderate
  BANDWIDTH_HIGH: 0xff6600,    // Orange - stressed
  BANDWIDTH_CRITICAL: 0xff0044, // Red - overloaded

  // UI Elements
  UI_PRIMARY: 0x00ffff,
  UI_SECONDARY: 0x0099cc,
  UI_SUCCESS: 0x00ff88,
  UI_WARNING: 0xffaa00,
  UI_DANGER: 0xff0044,
  UI_TEXT: 0xffffff,
  UI_TEXT_DIM: 0x99ccff,

  // Effects
  PARTICLE_GLOW: 0xffffff,
  CAPTURE_FLASH: 0xffffff,
  ENERGY_CORE: 0x00ffff,
};

export const VISUAL_CONFIG = {
  // Grid Settings
  HEX_SIZE: 40,              // Radius of hexagon
  HEX_LINE_WIDTH: 2,         // Grid line thickness
  HEX_GLOW_WIDTH: 4,         // Glow effect width
  GRID_ALPHA: 0.6,           // Grid line transparency
  GRID_GLOW_ALPHA: 0.3,      // Grid glow transparency

  // Node Sizes (smaller regular nodes, much larger bases)
  NODE_NEUTRAL_RADIUS: 8,
  NODE_OWNED_RADIUS: 10,
  NODE_BASE_RADIUS: 40,

  // Animation Speeds
  PULSE_SPEED: 2000,         // ms for complete pulse cycle
  PARTICLE_SPEED: 200,       // pixels per second
  CAPTURE_DURATION: 3000,    // ms to capture a node

  // Stream Settings
  STREAM_MIN_WIDTH: 2,
  STREAM_MAX_WIDTH: 8,
  PARTICLE_SIZE: 3,
  PARTICLES_PER_SECOND: 30,
  PACKET_LOSS_FLICKER_RATE: 100, // ms between flickers

  // Camera
  CAMERA_ZOOM_MIN: 0.3,
  CAMERA_ZOOM_MAX: 2.0,
  CAMERA_ZOOM_SPEED: 0.1,
  CAMERA_PAN_SPEED: 800,     // pixels per second

  // Effects
  GLOW_INTENSITY: 0.8,
  SCANLINE_SPEED: 50,        // pixels per second
  CRT_FLICKER_RATE: 60,      // Hz

  // Fog of War
  FOG_ALPHA: 0.85,           // Darkness of unexplored areas
  VISION_RANGE: 3,           // Hexes visible around owned nodes
};

export const PLAYER_COLORS = [
  COLORS.PLAYER_1,
  COLORS.PLAYER_2,
  COLORS.PLAYER_3,
  COLORS.PLAYER_4,
];

export const getPlayerColor = (playerId: number): number => {
  return PLAYER_COLORS[playerId % PLAYER_COLORS.length];
};

export const getBandwidthColor = (utilization: number): number => {
  if (utilization < 0.5) return COLORS.BANDWIDTH_LOW;
  if (utilization < 0.75) return COLORS.BANDWIDTH_MED;
  if (utilization < 0.9) return COLORS.BANDWIDTH_HIGH;
  return COLORS.BANDWIDTH_CRITICAL;
};
