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

  func power() async -> Observed<PowerState> {
    if holdsPower {
      await withCheckedContinuation { continuation in
        heldPower = continuation
        for waiter in waitingPower {
          waiter.resume()
        }
        waitingPower = []
      }
    }
    return blind ? Observed(powered.value, confirmed: false) : powered
  }

  /// Whether each power read is held until let go.
  var holdsPower = false
  var heldPower: CheckedContinuation<Void, Never>?
  var waitingPower: [CheckedContinuation<Void, Never>] = []

  /// Hold the next power read until let go.
  func holdingPower() { holdsPower = true }

  /// Return once a power read is held.
  func untilPowerHeld() async {
    guard heldPower == nil else { return }
    await withCheckedContinuation { waitingPower.append($0) }
  }

  /// Let the held power read go on, and read the next ones at once.
  func releasePower() {
    holdsPower = false
    heldPower?.resume()
    heldPower = nil
  }

  func create() async throws {
    try call("create")
    powered = Observed(.stopped, confirmed: true)
  }

  /// Why each start is refused, in its own words; none lets it start.
  var startRefusal: String?

  /// Refuse each start for `reason`, or let it start again.
  func refusingStarts(_ reason: String?) { startRefusal = reason }

  func start() async throws {
    try call("start")
    if let startRefusal {
      throw HostRefusal(description: startRefusal)
    }
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
    if holdsWindow {
      await withCheckedContinuation { continuation in
        heldWindow = continuation
        for waiter in waitingWindow {
          waiter.resume()
        }
        waitingWindow = []
      }
    }
    windowIsOpen = true
  }

  /// Whether each window opening is held, the window not yet open, until
  /// let go.
  var holdsWindow = false
  var heldWindow: CheckedContinuation<Void, Never>?
  var waitingWindow: [CheckedContinuation<Void, Never>] = []

  /// Hold the next window openings until let go.
  func holdingWindow() { holdsWindow = true }

  /// Return once a window opening is held.
  func untilWindowHeld() async {
    guard heldWindow == nil else { return }
    await withCheckedContinuation { waitingWindow.append($0) }
  }

  /// Let the held window opening go on, and open the next ones at once.
  func releaseWindow() {
    holdsWindow = false
    heldWindow?.resume()
    heldWindow = nil
  }

  func closeWindow() async throws {
    try call("closeWindow")
    windowIsOpen = windowSticks
    windowMinimized = windowSticks && windowMinimized
  }

  /// Whether the window is minimized: off the screen, and restored with
  /// its input from the Dock, as the person may.
  var windowMinimized = false

  /// Minimize the window, as the person may.
  func minimizeWindow() {
    windowIsOpen = false
    windowMinimized = true
  }

  /// Restore the minimized window, its input with it, as the person may.
  func restoreWindow() {
    windowIsOpen = true
    windowMinimized = false
  }

  func windowOpen() async -> Bool {
    WindowState(visible: windowIsOpen, minimized: windowMinimized) == .open
  }

  var earlierAreGone = true

  /// Say earlier VMs gone, or not.
  func earlier(gone: Bool) { earlierAreGone = gone }

  func earlierGone() async -> Bool { earlierAreGone }

  var installs: [InstallRequest] = []
  var reports: [String] = []

  func install(_ change: ChangeID, _ request: InstallRequest) async -> ChangeOutcome? {
    installs.append(request)
    return nil
  }

  /// The installs gone on with without their secret, and what each comes
  /// to; none leaves it pending.
  var resumed: [ChangeID] = []
  var resumedOutcome: ChangeOutcome?

  func resumeInstall(_ change: ChangeID) async -> ChangeOutcome? {
    resumed.append(change)
    return resumedOutcome
  }

  /// Make each install gone on with come to `outcome`.
  func resuming(as outcome: ChangeOutcome?) { resumedOutcome = outcome }

  /// Reports that cannot be kept yet, as a full disk leaves them.
  var reportsStick = false

  func paired(install: ChangeID) async -> ChangeOutcome? {
    reports.append("paired")
    return reportsStick ? nil : .done
  }

  func closed(install: ChangeID) async -> ChangeOutcome? {
    reports.append("closed")
    return reportsStick ? nil : .done
  }

  func sticking(reports: Bool) { reportsStick = reports }

  /// How many times the VM was forgotten.
  var forgotten = 0

  func forget() async -> ChangeOutcome? {
    forgotten += 1
    return .done
  }

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
