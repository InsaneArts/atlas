/// A complete helper window observation, independent of the regular applications displayed in Apps.
public struct HelperOccluderList: Decodable, Equatable, Sendable {
  /// All windows in front-to-back order. A malformed row fails the entire observation.
  public var windows: [HelperOccluder]

  /// Creates a complete observation.
  ///
  /// - Parameter windows: Every window, front to back.
  public init(windows: [HelperOccluder]) {
    self.windows = windows
  }

  /// The windows that can obscure a crop, with order assigned before any filtering.
  ///
  /// - Parameter display: The captured display's geometry.
  /// - Returns: Occluding windows, excluding transparent windows and the system's display-wide pass-through layers.
  public func covering(_ display: HelperDisplay) -> [HelperWindow] {
    windows.enumerated()
      .compactMap { order, observed in
        var window = observed.window
        guard observed.alpha > 0 else { return nil }
        let passesThrough = observed.owner == "Dock" || observed.owner == "Notification Center"
        let spansDisplay =
          window.x <= 0 && window.y <= 0
          && window.x + window.width >= display.width && window.y + window.height >= display.height
        guard !(passesThrough && spansDisplay) else { return nil }
        window.order = order
        return window
      }
  }
}
