# ⚡ Packet Royale - Network Warfare Visualization

A TRON-inspired network strategy game where players capture nodes by creating bandwidth streams and sending packets. Built for a hackathon with the theme "unintended behavior."

## 🎮 Game Concept

Players expand their digital territory by:
- Creating **bandwidth streams** between nodes
- Sending **packets** until capturing nodes via packet loss/bandwidth thresholds
- Exploring an **infinite dynamically-generated hex grid**
- Conquering opponent bases to trigger a simulated DDoS attack

## 🎨 Visual Design

### TRON Aesthetic
- **Neon cyan/blue grid** - Classic TRON color palette
- **Glowing nodes** - Energy cores with pulsing effects
- **Animated packet streams** - Particle flows representing data transmission
- **Fog of war** - Unexplored areas remain dark
- **CRT effects** - Scanlines and subtle flicker for retro-futuristic feel

### Color Coding
- **Cyan (#00ffff)** - Player 1 (you)
- **Hot Pink (#ff006e)** - Player 2 (opponent)
- **Green (#00ff88)** - Player 3
- **Orange (#ffaa00)** - Player 4
- **Grey** - Neutral nodes
- **Yellow → Red gradient** - Bandwidth utilization (healthy → overloaded)

## 🛠️ Tech Stack

- **Phaser.js** - 2D game framework
- **TypeScript** - Type-safe development
- **Vite** - Fast build tool and dev server
- **Hex Grid System** - Axial/cube coordinates for strategy gameplay

## 🚀 Getting Started

### Installation

```bash
# Navigate to frontend directory
cd packet-royale-frontend

# Install dependencies (if not already installed)
npm install

# Start development server
npm run dev
```

The game will be available at `http://localhost:5173/`

### Building for Production

```bash
npm run build
```

## 🎯 Controls

### Camera Navigation
- **Arrow Keys** - Pan the camera
- **Right-Click + Drag** - Pan with mouse
- **Mouse Wheel** - Zoom in/out

### Interaction
- **Left-Click on Node** - Select node (shows pulse effect)
- **Hover over Node** - Cursor changes to pointer

### HUD Buttons (Demo)
- **[BUILD STREAM]** - Create new bandwidth connection
- **[UPGRADE NODE]** - Increase node capacity
- **[LAUNCH ATTACK]** - Trigger DDoS simulation (when connected to enemy base)

## 📁 Project Structure

```
packet-royale-frontend/
├── src/
│   ├── config/
│   │   └── visualConstants.ts    # TRON colors, visual settings
│   ├── scenes/
│   │   ├── GameScene.ts           # Main game visualization
│   │   └── UIScene.ts             # HUD overlay
│   ├── types/
│   │   └── gameTypes.ts           # TypeScript interfaces
│   ├── utils/
│   │   ├── dummyData.ts           # Test data generator
│   │   └── hexUtils.ts            # Hexagonal grid math
│   ├── main.ts                    # Game initialization
│   └── style.css                  # TRON styling
├── index.html
└── package.json
```

## 🎨 Key Features Implemented

### ✅ Phase 1: Foundation
- Vite + TypeScript + Phaser setup
- TRON color palette and visual constants
- Dummy data structures for testing

### ✅ Phase 2: Hex Grid
- Infinite hex grid with glowing lines
- Camera controls (pan, zoom, smooth movement)
- Fog of war for unexplored areas

### ✅ Phase 3: Nodes
- Neutral nodes (geometric wireframes)
- Player-owned nodes (glowing energy cores)
- Base nodes (animated mainframe structures with rotating rings)
- Capture progress visualization (circular progress bars)
- Node hover and selection effects

### ✅ Phase 4: Bandwidth Streams
- Particle emitter system for animated data flow
- Stream thickness based on bandwidth capacity
- Color-coded by utilization (cyan → yellow → red)
- Directional packet flow

### ✅ Phase 5: HUD
- Cyberpunk command interface
- Real-time throughput display
- Node count tracker
- Interactive buttons with hover effects
- Scanline effect for retro aesthetic

### ✅ Phase 6: Visual Polish
- Bloom/glow effects on nodes and streams
- Pulsing animations
- CRT flicker effect
- Smooth camera movement
- Selection feedback (pulse rings)

## 🔄 Dummy Data System

The visualization currently uses **dummy data** to demonstrate all features:

### Current Demo State
- **~91 hex nodes** (5 rings around center)
- **2 players** with bases at different positions
- **Player 1** (cyan) - 5 owned nodes with active streams
- **Player 2** (pink) - Hidden in fog of war
- **Active capture** - One neutral node at 65% capture progress
- **Dynamic bandwidth** - Fluctuates with random variations
- **Packet transmission** - Simulated packet loss rates

### Data Updates
Game state updates every 100ms:
- Bandwidth fluctuations (±20%)
- Packet transmission simulation
- Capture progress increments
- Player throughput calculations

## 🔌 Backend Integration (Future)

To connect to a real backend:

1. Replace `generateDummyGameState()` in [GameScene.ts:40](src/scenes/GameScene.ts#L40)
2. Implement WebSocket connection for real-time updates
3. Replace `updateDummyGameState()` with network event handlers
4. Add API calls for player actions (build stream, upgrade node, etc.)

### Recommended Backend Tech
- **WebSocket/Socket.io** - Real-time bidirectional communication
- **Node.js/Express** or **Python/FastAPI** - Game server
- **Redis** - Game state storage
- **Network simulation library** - Packet loss, bandwidth modeling

## 🎭 Customization

### Changing Colors
Edit [src/config/visualConstants.ts](src/config/visualConstants.ts):

```typescript
export const COLORS = {
  PLAYER_1: 0x00ffff,  // Change player color
  GRID_PRIMARY: 0x00d4ff,  // Change grid color
  // ... etc
}
```

### Adjusting Visual Settings
Edit visual config in same file:

```typescript
export const VISUAL_CONFIG = {
  HEX_SIZE: 40,           // Hex radius
  PARTICLE_SPEED: 200,    // Packet speed
  CAMERA_ZOOM_SPEED: 0.1, // Zoom sensitivity
  // ... etc
}
```

### Adding More Nodes
Edit [src/utils/dummyData.ts:53](src/utils/dummyData.ts#L53):

```typescript
const hexCoords = generateHexSpiral(10); // Increase ring count
```

## 🐛 Known Limitations (MVP Demo)

- No multiplayer networking (dummy data only)
- No actual packet loss simulation (visual only)
- Attack button shows modal but doesn't execute real attack
- Infinite grid is limited to pre-generated nodes
- No persistence (state resets on refresh)

## 🚧 Future Enhancements

### High Priority
- [ ] WebSocket integration for multiplayer
- [ ] Real bandwidth/packet simulation
- [ ] Dynamic map generation (true infinite)
- [ ] Save/load game state
- [ ] Audio effects (TRON-style synth sounds)

### Visual Enhancements
- [ ] Bloom post-processing shader
- [ ] Territory Voronoi regions
- [ ] Connection establishment lightning effects
- [ ] Node explosion on capture
- [ ] Minimap with real-time topology

### Gameplay
- [ ] Resource management (bandwidth as currency)
- [ ] Different node types (relay, fortress, generator)
- [ ] Upgrades and tech tree
- [ ] Turn-based vs real-time modes
- [ ] Fog of war reveal animation

## 📝 Notes for Hackathon

### What's Working
✅ Complete visualization of game concept
✅ TRON aesthetic successfully achieved
✅ All core mechanics visualized (streams, capture, fog of war)
✅ Interactive controls functional
✅ Dummy data system allows testing without backend

### Demo Tips
1. **Open browser console** - Shows node click events and logs
2. **Try zooming in/out** - See particle detail at different scales
3. **Watch the capture progress** - Node at position (2,0) is being captured
4. **Observe bandwidth colors** - Streams change color based on load
5. **Click the attack button** - Shows win/loss modal concept

## 🤝 Contributing

This is a hackathon project! Feel free to:
- Add backend integration
- Implement multiplayer networking
- Enhance visual effects
- Add sound design
- Create additional node types

## 📄 License

MIT License - Built for hackathon demonstration

---

**Built with ⚡ for the "Unintended Behavior" Hackathon**
