use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Bounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Bounds {
    /// The main display of `width` by `height` points: its corner is the
    /// origin of global points, so a window's bounds are points of it.
    pub(crate) fn display(width: f64, height: f64) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    /// These bounds grown by `pad` on every side.
    pub(crate) fn padded(&self, pad: f64) -> Self {
        Self {
            x: self.x - pad,
            y: self.y - pad,
            width: self.width + 2.0 * pad,
            height: self.height + 2.0 * pad,
        }
    }

    pub(crate) fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    pub(crate) fn covers(&self, other: &Bounds) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.x + self.width >= other.x + other.width
            && self.y + self.height >= other.y + other.height
    }

    pub(crate) fn center(&self) -> (f64, f64) {
        (
            (self.x + self.width / 2.0).round(),
            (self.y + self.height / 2.0).round(),
        )
    }

    /// Whether two are the same within a point.
    pub(crate) fn matches(&self, other: &Bounds) -> bool {
        let near = |a: f64, b: f64| (a - b).abs() <= 1.0;
        near(self.x, other.x)
            && near(self.y, other.y)
            && near(self.width, other.width)
            && near(self.height, other.height)
    }

    /// Whether the two share an area, not just an edge or a corner.
    pub(crate) fn intersects(&self, other: &Bounds) -> bool {
        self.width > 0.0
            && self.height > 0.0
            && other.width > 0.0
            && other.height > 0.0
            && self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }
}
