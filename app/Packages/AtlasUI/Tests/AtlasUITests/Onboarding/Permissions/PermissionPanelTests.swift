import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("PermissionPanel")
struct PermissionPanelTests {
  @Test("a permission that cannot be read reads Checking… on its step, while one read keeps what it lets agents do")
  func anUnreadPermissionReadsChecking() {
    var state = PreviewStates.onboardingPermissionPanelState
    state.screenRecording.isKnown = false
    state.screenRecording.isCurrent = false
    state.screenRecording.standing = .checking("Checking…")
    let steps = PermissionPanel<EmptyView>.steps(state)
    #expect(steps.map(\.detail) == ["Click, type, read buttons", "Checking…"])
    #expect(steps.last?.state == .waiting, "nothing to allow while it cannot be read")
    state.screenRecording.isKnown = true
    state.screenRecording.isCurrent = true
    state.screenRecording.standing = .yourTurn
    let read = PermissionPanel<EmptyView>.steps(state)
    #expect(read.last?.detail == "Take screenshots")
    #expect(read.last?.state == .current)
  }

  @Test("only the permission the panel guides is lit: one whose turn has come but is not guided yet waits")
  func onlyTheGuidedStepIsLit() {
    let state = PreviewStates.onboardingPermissionPanelAllowed
    guard case .permissionPanel(let panel) = state.content else {
      Issue.record("not the panel")
      return
    }
    let steps = PermissionPanel<EmptyView>.steps(panel)
    #expect(steps.map(\.state) == [.done, .waiting], "the next one lights up as the panel moves on to it")
  }
}
