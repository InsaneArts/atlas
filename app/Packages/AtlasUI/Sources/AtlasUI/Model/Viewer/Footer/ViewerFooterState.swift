/// The footer under the helper's screen: the newest thing to know at its leading edge and the buttons at its trailing
/// edge.
public struct ViewerFooterState: Equatable, Sendable {
  /// The line at the leading edge.
  public var latest: ViewerLatestLine
  /// The buttons at the trailing edge.
  public var controls: ViewerFooterControls
  /// How it gives way to a narrow card.
  public var fit: ViewerFooterFit

  /// Creates the footer's state.
  ///
  /// - Parameters:
  ///   - latest: The line at the leading edge.
  ///   - controls: The buttons at the trailing edge.
  ///   - fit: How it gives way to a narrow card; whole by default.
  public init(latest: ViewerLatestLine, controls: ViewerFooterControls, fit: ViewerFooterFit = .full) {
    self.latest = latest
    self.controls = controls
    self.fit = fit
  }
}
