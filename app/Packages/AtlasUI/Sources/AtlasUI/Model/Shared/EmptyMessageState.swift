/// Why there is nothing to show, where a picture or a list would be: a headline, a sentence and, at most, one button.
public struct EmptyMessageState: Equatable, Sendable {
  /// What is the matter, such as "The helper is logged out".
  public var headline: String
  /// What it means, or what helps, in one sentence.
  public var message: String
  /// The one button, if any.
  public var action: EmptyMessageAction?

  /// Creates an empty message.
  ///
  /// - Parameters:
  ///   - headline: What is the matter.
  ///   - message: What it means, or what helps, in one sentence.
  ///   - action: The one button, if any.
  public init(headline: String, message: String, action: EmptyMessageAction? = nil) {
    self.headline = headline
    self.message = message
    self.action = action
  }
}
