/// The cards beside the desktop: the helper, what happened, whether all is ready, the session and the picture.
package struct ViewerSidebarState: Equatable, Sendable {
  /// The helper as people know it.
  package var account: String
  /// Why the helper is in its mode.
  package var reason: String
  /// The latest actions, newest first.
  package var recent: [ActivityEntry]
  /// Each readiness check.
  package var ready: [ReadinessItem]
  /// The session's facts: display, input, control and pending actions.
  package var facts: [Fact]
  /// How often a picture is read while watching.
  package var rate: ViewerPictureRate
  /// The note under the picture's card: its size in pixels when known, and that pictures are never saved.
  package var pictureNote: String

  /// Creates the sidebar's state.
  ///
  /// - Parameters:
  ///   - account: The helper as people know it.
  ///   - reason: Why the helper is in its mode.
  ///   - recent: The latest actions, newest first.
  ///   - ready: Each readiness check.
  ///   - facts: The session's facts: display, input, control and pending actions.
  ///   - rate: How often a picture is read while watching.
  ///   - pictureNote: The note under the picture's card: its size in pixels when known, and that pictures are never
  ///     saved.
  package init(
    account: String,
    reason: String,
    recent: [ActivityEntry],
    ready: [ReadinessItem],
    facts: [Fact],
    rate: ViewerPictureRate,
    pictureNote: String
  ) {
    self.account = account
    self.reason = reason
    self.recent = recent
    self.ready = ready
    self.facts = facts
    self.rate = rate
    self.pictureNote = pictureNote
  }
}
