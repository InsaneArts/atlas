import Foundation
import ServiceManagement
import os

@testable import AtlasCommands

/// A login service that records changes and reports a chosen status and error afterwards.
final class ScriptedLoginItemService: LoginItemService {
  private let state: OSAllocatedUnfairLock<(status: SMAppService.Status, calls: [Bool])>
  private let statusAfterChange: SMAppService.Status
  private let failure: NSError?

  init(status: SMAppService.Status, statusAfterChange: SMAppService.Status, failure: NSError? = nil) {
    state = OSAllocatedUnfairLock(initialState: (status, []))
    self.statusAfterChange = statusAfterChange
    self.failure = failure
  }

  var status: SMAppService.Status { state.withLock { $0.status } }
  var calls: [Bool] { state.withLock { $0.calls } }

  func register() throws { try change(true) }
  func unregister() throws { try change(false) }

  private func change(_ on: Bool) throws {
    state.withLock {
      $0.calls.append(on)
      $0.status = statusAfterChange
    }
    if let failure { throw failure }
  }
}
