import Foundation
import os

/// Where making a VM stands, kept for its station as it goes: written at
/// most once a second within a phase, and at once when its phase moves on.
/// A download never goes back: one resumed counts on from what its earlier
/// tries received. Told from any thread; a write that fails is tried again
/// with the next.
public final class MakingLog: Sendable {
  /// The shortest time between two writes within one phase.
  public static let spacing = Duration.seconds(1)

  private let file: URL
  private let now: @Sendable () -> ContinuousClock.Instant
  private let state: OSAllocatedUnfairLock<MakingLogState>

  /// The log of the make `change` kept at `file`, its time told by `now`.
  /// What an earlier run of the same make received is counted on from.
  public init(
    file: URL,
    change: ChangeID,
    now: @escaping @Sendable () -> ContinuousClock.Instant = { ContinuousClock.now }
  ) {
    self.file = file
    self.now = now
    var floor: MakingLogState.Floor?
    if let kept = try? MakingProgress.read(from: file), kept.change == change, let received = kept.received,
      let total = kept.total
    {
      floor = (received, total)
    }
    let progress = MakingProgress(change: change, phase: .checking)
    state = OSAllocatedUnfairLock(initialState: MakingLogState(progress: progress, written: nil, floor: floor))
  }

  /// This Mac is being checked, and the restore image chosen.
  public func checking() {
    tell(phase: .checking) { _ in }
  }

  /// `received` bytes of a download of `total` are in.
  public func downloading(received: Int64, total: Int64) {
    tell(phase: .downloading) { state in
      guard total > 0 else { return }
      var counted = max(received, 0)
      if let floor = state.floor, floor.total == total {
        counted = max(counted, floor.received)
      }
      counted = min(counted, total)
      state.floor = (counted, total)
      state.progress.received = counted
      state.progress.total = total
      state.progress.fraction = Double(counted) / Double(total)
    }
  }

  /// macOS is being installed, `fraction` of it done.
  public func installing(fraction: Double) {
    tell(phase: .installing) { state in
      state.progress.received = nil
      state.progress.total = nil
      state.progress.fraction = min(max(fraction, 0), 1)
    }
  }

  /// The VM is made.
  public func done() {
    tell(phase: .done) { state in
      state.progress.received = nil
      state.progress.total = nil
      state.progress.fraction = 1
    }
  }

  /// Move to `phase`, change what is kept with `change`, and write it when
  /// the phase moved on or a second passed since the last write: under the
  /// lock, so what is kept is never older than what was kept before it.
  private func tell(phase: MakingPhase, _ change: @Sendable (inout MakingLogState) -> Void) {
    let instant = now()
    let file = file
    state.withLock { state in
      let moved = state.progress.phase != phase
      if moved {
        state.progress.received = nil
        state.progress.total = nil
        state.progress.fraction = nil
      }
      state.progress.phase = phase
      change(&state)
      if !moved, let written = state.written, instant - written < Self.spacing {
        return
      }
      state.written = (try? state.progress.write(to: file)) == nil ? nil : instant
    }
  }
}
