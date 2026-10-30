extension Keycap {
  /// How a keycap is drawn.
  enum Style: Sendable {
    /// With a fine rim and a shaded lower edge, to stand out over a picture.
    case edged
    /// A raised fill with a solid lip under it and no edge, in a list.
    case soft
  }
}
