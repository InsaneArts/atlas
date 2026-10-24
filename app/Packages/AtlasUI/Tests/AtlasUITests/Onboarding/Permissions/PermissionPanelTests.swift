import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("PermissionPanel")
struct PermissionPanelTests {
  @Test("a permission that cannot be read reads Checking… on its step, while one read keeps what it lets agents do")
  func anUnreadPermissionReadsChecking() {
    var state = PreviewStates.onboardingPermissionPanelState
    state.accessibility.isOn = true
    state.accessibility.status = "Allowed"
    state.screenRecording.isOn = false
    state.screenRecording.isKnown = false
    state.screenRecording.canAllow = false
    state.screenRecording.status = "Checking…"
    let steps = PermissionPanel<EmptyView>.steps(state)
    #expect(steps.map(\.detail) == ["Click, type, read buttons", "Checking…"])
    #expect(steps.last?.state == .waiting, "nothing to allow while it cannot be read")
    state.screenRecording.isKnown = true
    state.screenRecording.canAllow = true
    state.screenRecording.status = "Off"
    let read = PermissionPanel<EmptyView>.steps(state)
    #expect(read.last?.detail == "Take screenshots")
    #expect(read.last?.state == .current)
  }
}
