import CryptoKit
import Foundation

/// What putting Atlas inside a VM's guest needs: the bundle to install, the
/// enrolment its guest pairs for, and the pairing secret the station made.
public struct InstallRequest: Equatable, Sendable {
  /// The Atlas.app to install, on this Mac.
  public var bundle: String
  /// The enrolment the guest daemon pairs for.
  public var enrolment: String
  /// The pairing secret; only its digest is ever kept.
  public var secret: String

  /// `bundle`, for `enrolment`, with `secret`.
  public init(bundle: String, enrolment: String, secret: String) {
    self.bundle = bundle
    self.enrolment = enrolment
    self.secret = secret
  }

  /// It as kept: the secret replaced by its SHA-256, so no record holds it.
  var recorded: InstallRequest {
    guard !secret.hasPrefix("sha256:") else { return self }
    let digest =
      SHA256
      .hash(data: Data(secret.utf8))
      .map { String(format: "%02x", $0) }
      .joined()
    return InstallRequest(bundle: bundle, enrolment: enrolment, secret: "sha256:\(digest)")
  }
}
