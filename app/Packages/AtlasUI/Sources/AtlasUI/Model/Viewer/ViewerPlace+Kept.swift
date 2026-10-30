extension ViewerPlace {
  /// The places whose panes the window keeps built once shown, in the Library's order: every list, not the Screen,
  /// whose picture and input surface exist only while it shows.
  public static let keptPlaces = allCases.filter(\.isKept)

  /// Whether the window keeps this place's pane built once shown, fading it as the place changes rather than building
  /// it again.
  public var isKept: Bool {
    self != .screen
  }
}
