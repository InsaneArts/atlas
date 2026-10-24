import SwiftUI

/// How thick a glass surface is. Glass goes only on what floats over the window's ground, never on the ground
/// itself. Names describe thickness so each backing can be shared by several surfaces.
enum GlassStyle: Sendable, CaseIterable {
  /// The chrome: a chip over the picture, the tab track, the large screenshot's bar, the panel.
  case regular
  /// A modal surface over the scrim, such as an alert: near opaque on the material, never the clear system glass,
  /// which is meant for controls and would let what is behind show through its paragraphs.
  case thick

  /// What a glass surface is drawn with.
  enum Backing: Sendable, Equatable {
    /// The system's glass, `glassEffect`, on macOS 26 and later.
    case glass
    /// The thin material under a tint, before macOS 26.
    case material
    /// An opaque fill, under Reduce Transparency.
    case opaque
  }

  /// What draws a glass surface: the system glass where the system has it, the thin material before it, and
  /// an opaque fill under Reduce Transparency on either. A design render, which cannot draw the system glass,
  /// gets the material with its tint so the surface remains legible.
  ///
  /// - Parameters:
  ///   - glassAvailable: Whether the system has its glass, macOS 26 and later.
  ///   - reduceTransparency: Whether the Reduce Transparency setting is on.
  ///   - drawsGlassEffect: Whether the system glass can be drawn here; false in a design render.
  /// - Returns: The backing.
  static func backing(glassAvailable: Bool, reduceTransparency: Bool, drawsGlassEffect: Bool = true) -> Backing {
    if reduceTransparency { return .opaque }
    return glassAvailable && drawsGlassEffect ? .glass : .material
  }

  /// What draws a surface of this thickness: the chrome's backing as the type's own `backing` says, and for a modal
  /// surface the material where the chrome would get the system glass.
  ///
  /// - Parameters:
  ///   - glassAvailable: Whether the system has its glass, macOS 26 and later.
  ///   - reduceTransparency: Whether the Reduce Transparency setting is on.
  ///   - drawsGlassEffect: Whether the system glass can be drawn here; false in a design render.
  /// - Returns: The backing.
  func backing(glassAvailable: Bool, reduceTransparency: Bool, drawsGlassEffect: Bool = true) -> Backing {
    let backing = Self.backing(
      glassAvailable: glassAvailable,
      reduceTransparency: reduceTransparency,
      drawsGlassEffect: drawsGlassEffect
    )
    return self == .thick && backing == .glass ? .material : backing
  }

  /// The fill laid between the backing and the content, or nil for none.
  ///
  /// - Parameter backing: What the surface is drawn with.
  /// - Returns: For the system glass, none for the chrome and ``Color/fillGlassThick`` for a modal surface; for the
  ///   material, ``Color/fillGlass`` or ``Color/fillGlassThick`` again; under
  ///   Reduce Transparency, the opaque ``Color/fillFloatingSolid`` or ``Color/fillModalSolid``.
  func fill(on backing: Backing) -> Color? {
    switch (self, backing) {
    case (.regular, .glass): nil
    case (.thick, .glass): .fillGlassThick
    case (.regular, .material): .fillGlass
    case (.thick, .material): .fillGlassThick
    case (.regular, .opaque): .fillFloatingSolid
    case (.thick, .opaque): .fillModalSolid
    }
  }
}
