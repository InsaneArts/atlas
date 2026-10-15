import Foundation
import ServiceManagement

/// "Open at login": the LaunchAgent bundled in Atlas.app at
/// Contents/Library/LaunchAgents, registered with the system so Atlas runs
/// whenever the person logs in, and follows the app through updates.
public struct LoginItem: Sendable {
    public static let plist = "com.insanearts.atlas.menubar.plist"

    public init() {}

    var service: SMAppService { SMAppService.agent(plistName: Self.plist) }

    public var enabled: Bool { service.status == .enabled }

    /// Whether this app carries the agent at all (a development build does
    /// not). The service reports "not found" until first registered, so the
    /// bundle is checked instead.
    public var available: Bool {
        FileManager.default.fileExists(
            atPath: Bundle.main.bundleURL
                .appendingPathComponent("Contents/Library/LaunchAgents")
                .appendingPathComponent(Self.plist).path)
    }

    public func set(_ on: Bool) throws {
        if on { try service.register() } else { try service.unregister() }
    }
}
