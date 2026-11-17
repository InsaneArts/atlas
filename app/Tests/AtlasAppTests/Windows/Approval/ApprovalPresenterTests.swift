import AtlasCommands
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ApprovalPresenter")
struct ApprovalPresenterTests {
  @Test(
    "each request kind shows as its operation, naming the VM and the session",
    arguments: [
      (LifecycleKind.add, ApprovalOperation.add),
      (.remove, .remove),
      (.uninstall, .uninstall),
    ]
  )
  func eachKind(kind: LifecycleKind, operation: ApprovalOperation) {
    let request = LifecycleRequest(
      id: "a",
      kind: kind,
      name: "work",
      vm: "b",
      settings: "~/.config/atlas.toml",
      session: "a terminal",
      until: Date(timeIntervalSince1970: 0)
    )
    let state = ApprovalPresenter.state(ApprovalSnapshot(request: request, phase: .asking))
    #expect(state == ApprovalState(operation: operation, desktop: "work", session: "a terminal", phase: .asking))
  }

  @Test("an uninstall names the VMs it removes, and one calling an uninstall off asks to call it off")
  func uninstallNamesItsVms() {
    let request = { (cancel: Bool) in
      LifecycleRequest(
        id: "a",
        kind: .uninstall,
        name: "work and spare",
        vm: "b",
        settings: "~/.config/atlas.toml",
        session: "a terminal",
        until: Date(timeIntervalSince1970: 0),
        cancel: cancel
      )
    }
    let uninstalling = ApprovalPresenter.state(ApprovalSnapshot(request: request(false), phase: .asking))
    let callingOff = ApprovalPresenter.state(ApprovalSnapshot(request: request(true), phase: .asking))
    #expect(uninstalling.operation == .uninstall)
    #expect(uninstalling.desktop == "work and spare")
    #expect(callingOff.operation == .cancelUninstall)
  }

  @Test("before a request is read the window names no VM")
  func unreadNamesNone() {
    let state = ApprovalPresenter.state(ApprovalSnapshot())
    #expect(state == .loading)
  }
}
