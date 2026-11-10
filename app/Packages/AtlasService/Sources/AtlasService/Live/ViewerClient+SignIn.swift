import Foundation

/// A held sign-in: starting it for a site, renewing its lease with no picture, and ending it with Done or Cancel.
extension ViewerClient {
  /// Posts to `sign-in/start`, `forget/start` or `reset/start`.
  ///
  /// - Parameters:
  ///   - kind: What it is for.
  ///   - site: The site.
  ///   - browser: The browser to open; the helper's own when nil.
  /// - Returns: The sign-in, with its lease.
  /// - Throws: ``ServiceError`` with the service's refusal.
  public func startSignIn(_ kind: SignInKind, site: String, browser: String?) async throws -> SignInStarted {
    /// The request: the site and the browser.
    struct Body: Encodable {
      /// The site.
      let site: String
      /// The browser's bundle identifier, when one is named.
      let browser: String?
    }
    let data = try await post(Self.startRoute(kind), Body(site: site, browser: browser))
    guard let started = Self.decoded(SignInStarted.self, from: data) else {
      throw ServiceError("Atlas couldn't start the sign-in. Try again.")
    }
    return started
  }

  /// The route that starts a hold for `kind`.
  ///
  /// - Parameter kind: What it is for.
  /// - Returns: The route.
  static func startRoute(_ kind: SignInKind) -> String {
    switch kind {
    case .signIn: "sign-in/start"
    case .forget: "forget/start"
    case .reset: "reset/start"
    }
  }

  /// Posts to `sign-in/renew`.
  ///
  /// - Parameter lease: The sign-in's lease.
  /// - Throws: ``ServiceError`` when it has ended.
  public func renewSignIn(lease: String) async throws {
    _ = try await post("sign-in/renew", SignInLease(lease: lease))
  }

  /// Posts to `sign-in/done`.
  ///
  /// - Parameter lease: The sign-in's lease.
  /// - Returns: What it was for and what it added.
  /// - Throws: ``ServiceError`` when it had ended.
  public func finishSignIn(lease: String) async throws -> SignInEnded {
    let data = try await post("sign-in/done", SignInLease(lease: lease))
    guard let ended = Self.decoded(SignInEnded.self, from: data) else {
      throw ServiceError("Atlas couldn't read how the sign-in ended.")
    }
    return ended
  }

  /// Posts to `sign-in/cancel`.
  ///
  /// - Parameter lease: The sign-in's lease.
  /// - Throws: ``ServiceError`` when the service refuses.
  public func cancelSignIn(lease: String) async throws {
    _ = try await post("sign-in/cancel", SignInLease(lease: lease))
  }
}
