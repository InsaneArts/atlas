import Observation

/// A deterministic suspended icon lookup. Once released, later reads pass through immediately.
@Observable
final class IconLookupGate {
  private(set) var count = 0
  private(set) var active = 0
  private(set) var maximumActive = 0
  private var released = false
  @ObservationIgnored private var waiting: [CheckedContinuation<Void, Never>] = []

  func read() async {
    count += 1
    active += 1
    maximumActive = max(maximumActive, active)
    defer { active -= 1 }
    guard !released else { return }
    await withCheckedContinuation { waiting.append($0) }
  }

  func release() {
    released = true
    let resumes = waiting
    waiting = []
    for resume in resumes { resume.resume() }
  }
}
