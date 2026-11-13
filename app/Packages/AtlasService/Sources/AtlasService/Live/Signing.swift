import Foundation
import Security

/// Whether the process at the other end of a socket is Atlas as this app was signed: the same signing team and
/// identifier under Developer ID, checked on that process's running code through the audit token the kernel stamped
/// when it connected, so one that ran another program since, or handed the socket on, never passes.
enum Signing {
  /// Whether the process that connected `socket` runs `executable` and its running code is signed as this app is.
  ///
  /// - Parameters:
  ///   - socket: The connected socket.
  ///   - executable: The file it must run.
  /// - Returns: Whether it does.
  static func peer(of socket: Int32, runs executable: URL) -> Bool {
    guard let token = auditToken(of: socket), let requirement = requirement() else { return false }
    let attributes = [kSecGuestAttributeAudit: token] as CFDictionary
    var code: SecCode?
    guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &code) == errSecSuccess, let code else {
      return false
    }
    guard SecCodeCheckValidity(code, [], requirement) == errSecSuccess else { return false }
    var staticCode: SecStaticCode?
    guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode else { return false }
    // The main executable, not the code's path: for a program inside a bundle that path is the bundle.
    var information: CFDictionary?
    guard SecCodeCopySigningInformation(staticCode, [], &information) == errSecSuccess,
      let information = information as? [String: Any],
      let main = information[kSecCodeInfoMainExecutable as String] as? URL
    else { return false }
    return main.resolvingSymlinksInPath().standardizedFileURL
      == executable.resolvingSymlinksInPath().standardizedFileURL
  }

  /// The audit token of the process that connected `socket`.
  ///
  /// - Parameter socket: The connected socket.
  /// - Returns: The token's bytes.
  private static func auditToken(of socket: Int32) -> Data? {
    var token = audit_token_t()
    var length = socklen_t(MemoryLayout<audit_token_t>.size)
    guard getsockopt(socket, SOL_LOCAL, LOCAL_PEERTOKEN, &token, &length) == 0,
      length == socklen_t(MemoryLayout<audit_token_t>.size)
    else { return nil }
    return withUnsafeBytes(of: &token) { Data($0) }
  }

  /// The requirement Atlas's own code meets: Apple's anchor under Developer ID, this app's identifier and its signing
  /// team, read from this app's own signature; nil for an app not signed by a team.
  ///
  /// - Returns: The requirement.
  private static func requirement() -> SecRequirement? {
    var own: SecCode?
    guard SecCodeCopySelf([], &own) == errSecSuccess, let own else { return nil }
    var staticCode: SecStaticCode?
    guard SecCodeCopyStaticCode(own, [], &staticCode) == errSecSuccess, let staticCode else { return nil }
    var information: CFDictionary?
    let flags = SecCSFlags(rawValue: kSecCSSigningInformation)
    guard SecCodeCopySigningInformation(staticCode, flags, &information) == errSecSuccess,
      let information = information as? [String: Any],
      let team = information[kSecCodeInfoTeamIdentifier as String] as? String,
      let identifier = information[kSecCodeInfoIdentifier as String] as? String
    else { return nil }
    let text =
      "anchor apple generic and identifier \"\(identifier)\" "
      + "and certificate 1[field.1.2.840.113635.100.6.2.6] "
      + "and certificate leaf[field.1.2.840.113635.100.6.1.13] "
      + "and certificate leaf[subject.OU] = \"\(team)\""
    var requirement: SecRequirement?
    guard SecRequirementCreateWithString(text as CFString, [], &requirement) == errSecSuccess else { return nil }
    return requirement
  }
}
