import Foundation

/// Why a request to the viewer service did not work, in the service's own
/// words when it gave any.
package struct ServiceError: Error, Equatable, Sendable, LocalizedError {
  /// What went wrong, as one line the person can read.
  package var message: String

  /// Creates an error that says `message`.
  ///
  /// - Parameter message: What went wrong, as one line the person can read.
  package init(_ message: String) {
    self.message = message
  }

  /// The message, so `localizedDescription` shows the service's own words.
  package var errorDescription: String? { message }
}
