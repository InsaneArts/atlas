import Foundation

/// Why a change could not be kept or read.
public enum ChangeLedgerError: Error, Equatable {
  /// Its record could not be written and made to outlive a crash.
  case unwritable(URL)
  /// Its record could not be read, or is not a record.
  case unreadable(URL)
}
