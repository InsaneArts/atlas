import Foundation

@testable import AtlasVMKit

/// A VM's host that only remembers what it was told, and can be told to fail.
actor FakeHost: VMHost {
  var powered: Observed<PowerState>
  var windowIsOpen = false
  var windowSticks = false
  var calls: [String] = []
  var failing: String?
  var failingAfter: String?
  var holds = false
  var blind = false
  var held: CheckedContinuation<Void, Never>?
  var waiting: [CheckedContinuation<Void, Never>] = []

  init(power: PowerState = .absent, confirmed: Bool = true) {
    powered = Observed(power, confirmed: confirmed)
  }

  func power() async -> Observed<PowerState> { blind ? Observed(powered.value, confirmed: false) : powered }

  func create() async throws {
    try call("create")
    powered = Observed(.stopped, confirmed: true)
  }

  func start() async throws {
    try call("start")
    if holds {
      await withCheckedContinuation { continuation in
        held = continuation
        for waiter in waiting {
          waiter.resume()
        }
        waiting = []
      }
    }
    powered = Observed(.running, confirmed: true)
  }

  func stop() async throws {
    try call("stop")
    powered = Observed(.stopped, confirmed: true)
    if failingAfter == "stop" {
      throw CocoaError(.featureUnsupported)
    }
  }

  func openWindow() async throws {
    try call("openWindow")
    windowIsOpen = true
  }

  func closeWindow() async throws {
    try call("closeWindow")
    windowIsOpen = windowSticks
  }

  func windowOpen() async -> Bool { windowIsOpen }

  func sticking() { windowSticks = true }

  func failing(_ name: String?, after: String? = nil) {
    failing = name
    failingAfter = after
  }

  /// Say every power as not seen just now, until told to see again.
  func blinding() { blind = true }

  /// Say power as seen again.
  func seeing() { blind = false }

  /// Hold each start until released.
  func holding() { holds = true }

  /// Return once a start is held.
  func untilHeld() async {
    guard held == nil else { return }
    await withCheckedContinuation { waiting.append($0) }
  }

  /// Let the held start go on.
  func release() {
    held?.resume()
    held = nil
  }

  private func call(_ name: String) throws {
    calls.append(name)
    if failing == name {
      throw CocoaError(.featureUnsupported)
    }
  }
}
