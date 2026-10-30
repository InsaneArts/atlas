import CoreGraphics

/// Where an app's windows sit on the helper's display, drawn small: the display's shape and each window's frame as
/// shares of it, front to back.
public struct ViewerScreenMap: Equatable, Sendable {
  /// The display's width over its height.
  public var aspect: CGFloat
  /// The app's windows, front to back, each as shares of the display from its top-leading corner.
  public var windows: [CGRect]

  /// Creates a map.
  ///
  /// - Parameters:
  ///   - aspect: The display's width over its height.
  ///   - windows: The app's windows, front to back, as shares of the display.
  public init(aspect: CGFloat, windows: [CGRect]) {
    self.aspect = aspect
    self.windows = windows
  }
}
