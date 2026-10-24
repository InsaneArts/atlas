import Foundation

/// Why a request to the viewer service did not work, in the service's own
/// words when it gave any.
public struct ServiceError: Error, Equatable, Sendable, LocalizedError {
  /// What went wrong, as one line the person can read.
  public var message: String
  /// The service's code for the refusal, such as `busy` or `stale-frame`, when it gave one.
  public var code: String?

  /// Creates an error that says `message`.
  ///
  /// - Parameters:
  ///   - message: What went wrong, as one line the person can read.
  ///   - code: The service's code for the refusal, when it gave one.
  public init(_ message: String, code: String? = nil) {
    self.message = message
    self.code = code
  }

  /// The message, so `localizedDescription` shows the service's own words.
  public var errorDescription: String? { message }

  /// Whether the service refused because something else holds the helper now, as an agent's action under way does:
  /// asked again a moment later, it may agree.
  public var isBusy: Bool { code == "busy" }
}
