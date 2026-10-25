import SwiftUI

extension EnvironmentValues {
  /// Whether `View.glass(_:in:)` draws the system's glass where the system has it. True in every window. The
  /// design renders turn it off: AppKit's `cacheDisplay` draws nothing at all for a view under `glassEffect`
  /// (measured on macOS 27 with `ImageRenderer`), so a render gets the thin material and its tint, the
  /// material fallback, in its place.
  @Entry public var drawsGlassEffect = true
}
