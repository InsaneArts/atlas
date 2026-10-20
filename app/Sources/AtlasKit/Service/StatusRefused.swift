import Foundation

/// The service answered `status.json` without a status (it was busy, say): it is still there, so the last status
/// stands rather than reading as unavailable.
package struct StatusRefused: Error, Equatable, Sendable, LocalizedError {
  /// Why there was no status, in the service's own words when it gave any.
  package var message: String

  /// Creates a refusal that says `message`.
  ///
  /// - Parameter message: Why there was no status.
  package init(_ message: String) {
    self.message = message
  }

  /// The message, so `localizedDescription` shows the service's own words.
  package var errorDescription: String? { message }
}
