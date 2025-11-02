/**
 * Hexagonal Grid Math Utilities
 * Using axial/cube coordinate system
 */

import type { HexCoordinate } from '../types/gameTypes';
import { VISUAL_CONFIG } from '../config/visualConstants';

/**
 * Convert hex axial coordinates to pixel position
 */
export function hexToPixel(coord: HexCoordinate, hexSize: number = VISUAL_CONFIG.HEX_SIZE): { x: number; y: number } {
  const x = hexSize * (Math.sqrt(3) * coord.q + (Math.sqrt(3) / 2) * coord.r);
  const y = hexSize * ((3 / 2) * coord.r);
  return { x, y };
}

/**
 * Convert pixel position to hex axial coordinates
 */
export function pixelToHex(x: number, y: number, hexSize: number = VISUAL_CONFIG.HEX_SIZE): HexCoordinate {
  const q = ((Math.sqrt(3) / 3) * x - (1 / 3) * y) / hexSize;
  const r = ((2 / 3) * y) / hexSize;
  return hexRound(q, r);
}

/**
 * Round fractional hex coordinates to nearest hex
 */
function hexRound(q: number, r: number): HexCoordinate {
  const s = -q - r;

  let rq = Math.round(q);
  let rr = Math.round(r);
  let rs = Math.round(s);

  const qDiff = Math.abs(rq - q);
  const rDiff = Math.abs(rr - r);
  const sDiff = Math.abs(rs - s);

  if (qDiff > rDiff && qDiff > sDiff) {
    rq = -rr - rs;
  } else if (rDiff > sDiff) {
    rr = -rq - rs;
  }

  return { q: rq, r: rr, s: -rq - rr };
}

/**
 * Calculate distance between two hexes
 */
export function hexDistance(a: HexCoordinate, b: HexCoordinate): number {
  return (Math.abs(a.q - b.q) + Math.abs(a.r - b.r) + Math.abs(a.s - b.s)) / 2;
}

/**
 * Get all hexes within a certain distance
 */
export function hexesInRange(center: HexCoordinate, range: number): HexCoordinate[] {
  const results: HexCoordinate[] = [];

  for (let q = -range; q <= range; q++) {
    const r1 = Math.max(-range, -q - range);
    const r2 = Math.min(range, -q + range);

    for (let r = r1; r <= r2; r++) {
      results.push({ q: center.q + q, r: center.r + r, s: -q - r });
    }
  }

  return results;
}

/**
 * Get the 6 vertices of a hexagon for drawing
 */
export function getHexVertices(
  center: { x: number; y: number },
  size: number = VISUAL_CONFIG.HEX_SIZE
): { x: number; y: number }[] {
  const vertices: { x: number; y: number }[] = [];

  for (let i = 0; i < 6; i++) {
    const angleDeg = 60 * i - 30; // Flat-top hexagon
    const angleRad = (Math.PI / 180) * angleDeg;
    vertices.push({
      x: center.x + size * Math.cos(angleRad),
      y: center.y + size * Math.sin(angleRad),
    });
  }

  return vertices;
}

/**
 * Get neighbors of a hex (6 adjacent hexes)
 */
export function getHexNeighbors(coord: HexCoordinate): HexCoordinate[] {
  const directions = [
    { q: 1, r: 0 },
    { q: 1, r: -1 },
    { q: 0, r: -1 },
    { q: -1, r: 0 },
    { q: -1, r: 1 },
    { q: 0, r: 1 },
  ];

  return directions.map((dir) => ({
    q: coord.q + dir.q,
    r: coord.r + dir.r,
    s: coord.s - dir.q - dir.r,
  }));
}

/**
 * Linear interpolation between two points
 */
export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/**
 * Draw a line between two hex coordinates
 */
export function hexLine(a: HexCoordinate, b: HexCoordinate): HexCoordinate[] {
  const distance = hexDistance(a, b);
  const results: HexCoordinate[] = [];

  for (let i = 0; i <= distance; i++) {
    const t = distance === 0 ? 0 : i / distance;
    const q = lerp(a.q, b.q, t);
    const r = lerp(a.r, b.r, t);
    results.push(hexRound(q, r));
  }

  return results;
}
