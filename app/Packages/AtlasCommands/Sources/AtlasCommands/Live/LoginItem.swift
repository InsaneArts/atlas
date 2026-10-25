import Foundation
import ServiceManagement

/// "Open at login": the LaunchAgent bundled in Atlas.app at
/// Contents/Library/LaunchAgents, registered with the system so Atlas runs
/// whenever the person logs in, and follows the app through updates.
public struct LoginItem: Sendable {
  /// The agent's file name in Contents/Library/LaunchAgents.
  static let plist = "com.insanearts.atlas.menubar.plist"

  /// Creates the login item for this app's bundled agent.
  public init() {
    self.init(service: SMAppService.agent(plistName: Self.plist))
  }

  /// Creates a login item with the given service.
  init(service: any LoginItemService) {
    self.service = service
  }

  /// The system's record of the bundled agent.
  let service: any LoginItemService

  /// Whether Atlas opens at login now.
  public var enabled: Bool { service.status == .enabled }

  /// Whether this app carries the agent at all (a development build does
  /// not). The service reports "not found" until first registered, so the
  /// bundle is checked instead.
  public var available: Bool {
    let agent = Bundle.main.bundleURL
      .appendingPathComponent("Contents/Library/LaunchAgents")
      .appendingPathComponent(Self.plist)
    return FileManager.default
      .fileExists(atPath: agent.path)
  }

  /// Turns opening at login on or off.
  ///
  /// - Parameter on: Whether Atlas should open at login.
  /// - Throws: The system's error when it refuses to register or unregister the agent.
  public func set(_ on: Bool) throws {
    guard !matches(on) else { return }
    do {
      if on { try service.register() } else { try service.unregister() }
    } catch {
      let failure = error as NSError
      guard failure.domain == "SMAppServiceErrorDomain",
        failure.code == kSMErrorAlreadyRegistered || failure.code == kSMErrorJobNotFound,
        matches(on)
      else { throw error }
    }
  }

  /// Whether the service's current status satisfies the requested preference.
  private func matches(_ on: Bool) -> Bool {
    let status = service.status
    return on ? status == .enabled : status == .notRegistered || status == .notFound
  }
}

extension SMAppService: LoginItemService {}
