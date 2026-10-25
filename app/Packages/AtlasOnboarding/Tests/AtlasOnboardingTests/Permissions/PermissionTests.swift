import Foundation
import Testing

@testable import AtlasOnboarding

@Suite("Permission")
struct PermissionTests {
  @Test("each permission opens its own Privacy & Security page")
  func settingsPage() {
    #expect(
      Permission.accessibility.settingsURL?.absoluteString
        == "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
    )
    #expect(
      Permission.screenRecording.settingsURL?.absoluteString
        == "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
    )
  }

  @Test("each permission takes the name its System Settings page has on that macOS")
  func titles() {
    let sonoma = OperatingSystemVersion(majorVersion: 14, minorVersion: 6, patchVersion: 0)
    let sequoia = OperatingSystemVersion(majorVersion: 15, minorVersion: 0, patchVersion: 0)
    let tahoe = OperatingSystemVersion(majorVersion: 26, minorVersion: 1, patchVersion: 0)
    let next = OperatingSystemVersion(majorVersion: 27, minorVersion: 0, patchVersion: 0)
    #expect(Permission.accessibility.title(on: sonoma) == "Accessibility")
    #expect(Permission.accessibility.title(on: tahoe) == "Accessibility")
    #expect(Permission.accessibility.title(on: next) == "Device Control and Data Access")
    #expect(Permission.screenRecording.title(on: sonoma) == "Screen Recording")
    #expect(Permission.screenRecording.title(on: sequoia) == "Screen & System Audio Recording")
    #expect(Permission.screenRecording.title(on: next) == "Screen & System Audio Recording")
  }
}
