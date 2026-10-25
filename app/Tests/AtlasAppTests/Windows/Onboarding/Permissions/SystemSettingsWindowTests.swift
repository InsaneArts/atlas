import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("SystemSettingsWindow")
struct SystemSettingsWindowTests {
  /// A window list entry.
  ///
  /// - Parameters:
  ///   - pid: Its owner's process ID.
  ///   - layer: Its layer.
  ///   - bounds: Its bounds.
  /// - Returns: The entry.
  private static func entry(pid: pid_t, layer: Int = 0, _ bounds: CGRect) -> [String: Any] {
    [
      kCGWindowOwnerPID as String: pid,
      kCGWindowLayer as String: layer,
      kCGWindowBounds as String: bounds.dictionaryRepresentation,
    ]
  }

  @Test("System Settings' window is its largest normal one, never another app's or a menu's")
  func largestNormalWindow() {
    let main = CGRect(x: 200, y: 100, width: 700, height: 640)
    let windows = [
      Self.entry(pid: 7, CGRect(x: 0, y: 0, width: 2000, height: 1200)),
      Self.entry(pid: 42, layer: 101, CGRect(x: 0, y: 0, width: 1500, height: 900)),
      Self.entry(pid: 42, main),
      Self.entry(pid: 42, CGRect(x: 300, y: 200, width: 400, height: 200)),
    ]
    #expect(SystemSettingsWindow.largest(in: windows, ownedBy: [42]) == main)
    #expect(SystemSettingsWindow.largest(in: windows, ownedBy: [9]) == nil)
  }

  @Test("bounds measured down from the primary screen's top turn into a frame measured up from its bottom")
  func appKitCoordinates() {
    let bounds = CGRect(x: 200, y: 100, width: 700, height: 640)
    #expect(SystemSettingsWindow.appKit(bounds, primaryTop: 982) == CGRect(x: 200, y: 242, width: 700, height: 640))
  }
}
