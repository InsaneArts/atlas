import Foundation

/// Why a command in the guest did not run, or ran and failed, with what it
/// printed; `unreachable` when `ssh` could not sign in at all.
struct RemoteFailure: Error, CustomStringConvertible {
  /// Why, as the person reads it.
  let description: String
  /// What the command printed before it failed.
  var printed = Data()
  /// Whether `ssh` could not sign in at all.
  var unreachable = false
}
