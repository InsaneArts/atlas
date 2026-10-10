import Foundation
import ServiceManagement

/// "Open at login": the LaunchAgent bundled in Understudy.app at
/// Contents/Library/LaunchAgents, registered with the system so the menu bar
/// runs whenever the person logs in, and follows the app through updates.
public struct LoginItem: Sendable {
    public static let plist = "com.insanearts.understudy.menubar.plist"

    public init() {}

    var service: SMAppService { SMAppService.agent(plistName: Self.plist) }

    public var enabled: Bool { service.status == .enabled }

    /// Whether this app carries the agent at all (a development build does not).
    public var available: Bool { service.status != .notFound }

    public func set(_ on: Bool) throws {
        if on { try service.register() } else { try service.unregister() }
    }
}
