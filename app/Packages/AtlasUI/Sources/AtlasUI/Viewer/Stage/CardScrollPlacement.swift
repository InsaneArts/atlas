/// Schedules a selected row's placement after its layout pass. Tests can hold that work until deselection has run.
struct CardScrollPlacement {
  /// Starts cancellable work on the main actor, just beyond the current layout pass.
  var schedule: @MainActor (@escaping @MainActor () -> Void) -> Task<Void, Never> = { place in
    Task { @MainActor in
      guard !Task.isCancelled else { return }
      place()
    }
  }
}
