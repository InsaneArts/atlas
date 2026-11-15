import Darwin
import Foundation
import Security

/// Whose code is at the other end of a local socket, as the kernel stamped
/// the connection with its audit token: never a PID looked up later.
public enum CodeIdentity {
  /// The running code of the process that connected `connection`.
  static func peer(_ connection: Int32) -> SecCode? {
    var token = audit_token_t()
    var length = socklen_t(MemoryLayout<audit_token_t>.size)
    guard getsockopt(connection, SOL_LOCAL, LOCAL_PEERTOKEN, &token, &length) == 0 else { return nil }
    let data = withUnsafeBytes(of: &token) { Data($0) }
    var guest: SecCode?
    let attributes = [kSecGuestAttributeAudit: data] as CFDictionary
    guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &guest) == errSecSuccess else { return nil }
    return guest
  }

  /// This program's own signing, read once, at its start: an update of
  /// Atlas may delete its file while it runs, and then it could be read
  /// no more.
  private static let signing: (team: String?, designated: String?) = {
    guard let own = own() else { return (nil, nil) }
    return (team(of: own), designated(of: own))
  }()

  /// Read this program's own signing now, its file perhaps already gone:
  /// whether all of it was read.
  public static func remember() -> Bool {
    signing.team != nil && signing.designated != nil
  }

  /// This program's own code, as signed on disk.
  private static func own() -> SecStaticCode? {
    var me: SecCode?
    var mine: SecStaticCode?
    guard SecCodeCopySelf([], &me) == errSecSuccess, let me,
      SecCodeCopyStaticCode(me, [], &mine) == errSecSuccess
    else {
      return nil
    }
    return mine
  }

  /// Whether `connection`'s peer runs code that meets this program's own
  /// designated requirement.
  static func runsThisCode(_ connection: Int32) -> Bool {
    var requirement: SecRequirement?
    guard let peer = peer(connection), let text = signing.designated,
      SecRequirementCreateWithString(text as CFString, [], &requirement) == errSecSuccess, let requirement
    else {
      return false
    }
    return SecCodeCheckValidity(peer, [], requirement) == errSecSuccess
  }

  /// This program's own designated requirement, as text.
  private static func designated(of own: SecStaticCode) -> String? {
    var requirement: SecRequirement?
    var text: CFString?
    guard SecCodeCopyDesignatedRequirement(own, [], &requirement) == errSecSuccess,
      let requirement, SecRequirementCopyString(requirement, [], &text) == errSecSuccess
    else {
      return nil
    }
    return text as String?
  }

  /// Whether `connection`'s peer runs Atlas itself: its identifier, signed
  /// with a Developer ID by the team this program is signed by.
  public static func runsAtlas(_ connection: Int32) -> Bool {
    guard let peer = peer(connection), let text = atlasRequirement() else { return false }
    var requirement: SecRequirement?
    guard SecRequirementCreateWithString(text as CFString, [], &requirement) == errSecSuccess, let requirement else {
      return false
    }
    return SecCodeCheckValidity(peer, [], requirement) == errSecSuccess
  }

  /// The code requirement Atlas itself meets: its identifier, signed with
  /// a Developer ID by the team this program is signed by; none when this
  /// program's own team cannot be read.
  public static func atlasRequirement() -> String? {
    guard let team = signing.team else { return nil }
    return "anchor apple generic and identifier \"com.insanearts.atlas\" "
      + "and certificate 1[field.1.2.840.113635.100.6.2.6] "
      + "and certificate leaf[field.1.2.840.113635.100.6.1.13] "
      + "and certificate leaf[subject.OU] = \"\(team)\""
  }

  /// The team the code `own` is signed by.
  private static func team(of own: SecStaticCode) -> String? {
    var information: CFDictionary?
    let flags = SecCSFlags(rawValue: kSecCSSigningInformation)
    guard SecCodeCopySigningInformation(own, flags, &information) == errSecSuccess,
      let information = information as? [String: Any]
    else {
      return nil
    }
    return information[kSecCodeInfoTeamIdentifier as String] as? String
  }
}
