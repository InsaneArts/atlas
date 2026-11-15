import Foundation

/// Why a record file could not be read or kept.
public enum PrivateFileError: Error, Equatable {
  /// It is not a regular file of this account's, others may change it, it
  /// is too long, or it could not be read.
  case unreadable(URL)
  /// It could not be written whole and made to reach the disk.
  case unwritable(URL)
}
