use super::events::NodeCoord;

impl NodeCoord {
    /// Create a new coordinate
    pub fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Get all 6 neighbors for a triangular grid coordinate
    /// In axial coordinates, neighbors are: (q±1, r), (q, r±1), (q+1, r-1), (q-1, r+1)
    pub fn neighbors(&self) -> [NodeCoord; 6] {
        [
            NodeCoord::new(self.q + 1, self.r),
            NodeCoord::new(self.q - 1, self.r),
            NodeCoord::new(self.q, self.r + 1),
            NodeCoord::new(self.q, self.r - 1),
            NodeCoord::new(self.q + 1, self.r - 1),
            NodeCoord::new(self.q - 1, self.r + 1),
        ]
    }

    /// Check if two coordinates are adjacent (distance 1)
    pub fn is_adjacent(&self, other: &NodeCoord) -> bool {
        self.distance(other) == 1
    }

    /// Calculate distance between two coordinates
    pub fn distance(&self, other: &NodeCoord) -> u32 {
        let dq = (self.q - other.q).abs();
        let dr = (self.r - other.r).abs();
        let ds = (self.q + self.r - other.q - other.r).abs();
        ((dq + dr + ds) / 2) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neighbors() {
        let coord = NodeCoord::new(0, 0);
        let neighbors = coord.neighbors();
        assert_eq!(neighbors.len(), 6);

        // All neighbors should be distance 1
        for neighbor in &neighbors {
            assert_eq!(coord.distance(neighbor), 1);
            assert!(coord.is_adjacent(neighbor));
        }
    }

    #[test]
    fn test_distance() {
        let c1 = NodeCoord::new(0, 0);
        let c2 = NodeCoord::new(2, 1);
        assert_eq!(c1.distance(&c2), 3);

        let c3 = NodeCoord::new(1, 0);
        assert_eq!(c1.distance(&c3), 1);
    }
}
