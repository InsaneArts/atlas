import Darwin
import Foundation

/// This Mac's record of who is logged in (utmpx), which `who` and `last` read: the helper's login time comes from
/// here, since the helper's own status does not carry it. Reading it changes nothing.
package enum LoginRecords {
  /// When `user` last logged in and is still logged in, or nil when no current login names them.
  ///
  /// The records are a process-wide cursor, so this runs on the main actor alone.
  ///
  /// - Parameter user: The account's short name, such as `atlas`.
  /// - Returns: The newest login still open for that account.
  @MainActor package static func loginTime(of user: String) -> Date? {
    setutxent()
    defer { endutxent() }
    var newest: Date?
    while let entry = getutxent() {
      let record = entry.pointee
      guard Int32(record.ut_type) == USER_PROCESS, name(record) == user else { continue }
      let login = Date(timeIntervalSince1970: TimeInterval(record.ut_tv.tv_sec))
      newest = max(newest ?? login, login)
    }
    return newest
  }

  /// A record's account name, which the record keeps as a fixed run of bytes ended by a zero.
  ///
  /// - Parameter record: The record.
  /// - Returns: The name.
  private static func name(_ record: utmpx) -> String {
    withUnsafeBytes(of: record.ut_user) { bytes in
      String(decoding: bytes.prefix { $0 != 0 }, as: UTF8.self)
    }
  }
}
