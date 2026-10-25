/// An explicit VoiceOver announcement, independent of routine status updates.
public struct SettingsAnnouncement: Equatable, Sendable {
  /// Distinguishes repeated announcements with the same words.
  public let request: Int
  /// The words VoiceOver speaks.
  public let text: String

  /// Creates an announcement request.
  ///
  /// - Parameters:
  ///   - request: The request counter.
  ///   - text: The words to speak.
  public init(request: Int, text: String) {
    self.request = request
    self.text = text
  }
}
