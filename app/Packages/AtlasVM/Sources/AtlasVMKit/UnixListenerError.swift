import Foundation

/// Why a socket could not be listened on.
public enum UnixListenerError: Error, Equatable {
  /// Its path is too long for a socket's address.
  case pathTooLong(URL)
  /// Something not a socket of this account's is at its path.
  case occupied(URL)
  /// It could not be made, bound or listened on.
  case unavailable(URL)
}
