import Testing

@testable import AtlasKit

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
}
