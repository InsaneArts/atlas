import Foundation

/// One frame of a change in the footer: whether Stop's mark is drawn, and whether the sound button's mark is
/// drawn in Stop's capsule, clear of Stop's own mark and word.
struct FooterSlotFrame: CustomStringConvertible {
  /// Seconds since the change.
  let moment: TimeInterval
  /// Whether Stop's mark is drawn.
  let stopDrawn: Bool
  /// Whether the sound button's mark is drawn inside Stop's capsule.
  let soundInStop: Bool

  /// The frame, for a failure to say.
  var description: String {
    String(format: "%.3f s: Stop %@, sound %@", moment, stopDrawn ? "drawn" : "-", soundInStop ? "in Stop" : "-")
  }
}
