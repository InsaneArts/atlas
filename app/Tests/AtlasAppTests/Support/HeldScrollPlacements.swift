import AsyncSupportTesting

@testable import AtlasUI

/// Real placement tasks held on a manual clock, so a test can deselect before any placement executes.
@MainActor
final class HeldScrollPlacements {
  /// The clock that releases the tasks.
  let clock = ManualClock()
  /// A token of manual time; no wall-clock pause is involved.
  static let delay: Duration = .seconds(1)
  /// Every task the list scheduled, including ones it later cancelled.
  private(set) var tasks: [Task<Void, Never>] = []
  /// How many final placements actually executed.
  private(set) var executions = 0

  /// The list's normal deferral with its execution held, preserving task cancellation.
  var placement: CardScrollPlacement {
    CardScrollPlacement { place in
      let task = Task { @MainActor in
        do { try await self.clock.sleep(for: Self.delay) } catch { return }
        guard !Task.isCancelled else { return }
        self.executions += 1
        place()
      }
      self.tasks.append(task)
      return task
    }
  }

  /// Ends every task the fixture owns, including on a failed requirement.
  func cancel() {
    for task in tasks { task.cancel() }
  }

  /// Waits for the tasks already scheduled to finish, after cancellation or a manual release.
  func finish() async {
    for task in tasks { await task.value }
  }
}
