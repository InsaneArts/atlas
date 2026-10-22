import Foundation

/// Why a request to the viewer service did not work, in the service's own
/// words when it gave any.
package struct ServiceError: Error, Equatable, Sendable, LocalizedError {
  /// What went wrong, as one line the person can read.
  package var message: String
  /// The service's code for the refusal, such as `busy` or `stale-frame`, when it gave one.
  package var code: String?

  /// Creates an error that says `message`.
  ///
  /// - Parameters:
  ///   - message: What went wrong, as one line the person can read.
  ///   - code: The service's code for the refusal, when it gave one.
  package init(_ message: String, code: String? = nil) {
    self.message = message
    self.code = code
  }

  /// The message, so `localizedDescription` shows the service's own words.
  package var errorDescription: String? { message }

  /// Whether the service refused because something else holds the helper now, as an agent's action under way does:
  /// asked again a moment later, it may agree.
  package var isBusy: Bool { code == "busy" }
}
