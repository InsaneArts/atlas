import AtlasService
import AtlasUI
import Foundation
import Observation

/// Coalesces app-wide icon requests by bundle identity, or name while the identity is unknown. At most two source
/// reads run at once. New live evidence replaces queued evidence and supersedes an active weaker result before any
/// waiter receives it. Images and misses share a bounded LRU cache in front of the disk store.
@Observable
final class AppIconRepository {
  /// The existing workspace concurrency limit, now shared by all windows.
  nonisolated static let atOnce = 2
  /// At most 128 decoded icons or misses are retained between window requests.
  nonisolated static let memoryLimit = 128

  @ObservationIgnored private let lookup: @Sendable (AppIconWant) async -> AppIconFound?
  @ObservationIgnored private let limit: Int
  private var cached: [String: AppIconCached] = [:]
  private var requests: [String: AppIconRequest] = [:]
  private var waiting: [String] = []
  private var active: [String: AppIconWant] = [:]
  private var use: UInt64 = 0

  /// Injected source reads make scheduling tests independent of the workspace and the disk cache.
  nonisolated init(
    memoryLimit: Int = AppIconRepository.memoryLimit,
    lookup: @escaping @Sendable (AppIconWant) async -> AppIconFound?
  ) {
    limit = max(1, memoryLimit)
    self.lookup = lookup
  }

  /// The retained entries, including misses, for checking the memory bound.
  var cachedCount: Int { cached.count }
  /// Callers still waiting, for checking cancellation and window teardown.
  var waiterCount: Int { requests.values.reduce(0) { $0 + $1.waiters.count } }

  /// Window snapshots share the cached images. A name-only lookup also supplies its discovered bundle identity;
  /// an explicit bundle entry always takes precedence over that alias.
  var images: [String: IconImage] {
    var images: [String: IconImage] = [:]
    for (key, entry) in cached {
      guard let found = entry.found else { continue }
      images[key] = found.drawn(as: key)
    }
    for entry in cached.values {
      guard case .name = entry.want.key, let found = entry.found,
        let bundleKey = AppIconKey(bundleId: found.bundleId), images[bundleKey.text] == nil
      else { continue }
      images[bundleKey.text] = found.drawn(as: bundleKey.text)
    }
    return images
  }

  /// Returns one image or miss, sharing pending work. Cancelling a caller removes only its own waiter; another
  /// window's lookup continues, and a running read can still warm the bounded cache after its window closes.
  func find(_ want: AppIconWant) async -> AppIconFound? {
    let caller = UUID()
    return await withTaskCancellationHandler {
      await withCheckedContinuation { resume in
        guard !Task.isCancelled else { return resume.resume(returning: nil) }
        enqueue(want, caller: caller, resume: resume)
      }
    } onCancel: {
      Task { @MainActor in self.cancel(caller, key: want.key.text) }
    }
  }

  /// Lets an existing consumer supply newer live evidence without registering another waiter.
  func updateEvidence(_ want: AppIconWant) {
    guard want.isLive, var request = requests[want.key.text] else { return }
    request.want = want.keepingEvidence(from: request.want)
    requests[want.key.text] = request
  }

  private func enqueue(
    _ incoming: AppIconWant,
    caller: UUID,
    resume: CheckedContinuation<AppIconFound?, Never>
  ) {
    let key = incoming.key.text
    var want = incoming
    if var request = requests[key] {
      if want.isLive { request.want = want.keepingEvidence(from: request.want) }
      request.waiters[caller] = resume
      requests[key] = request
      return
    }
    if var entry = cached[key] {
      if !want.isLive || entry.want.covers(want) {
        use += 1
        entry.used = use
        cached[key] = entry
        resume.resume(returning: entry.found)
        return
      }
      want = want.keepingEvidence(from: entry.want)
    }
    requests[key] = AppIconRequest(want: want, waiters: [caller: resume])
    waiting.append(key)
    start()
  }

  private func start() {
    while active.count < Self.atOnce, !waiting.isEmpty {
      let key = waiting.removeFirst()
      guard let request = requests[key], active[key] == nil else { continue }
      let want = request.want
      active[key] = want
      Task { [lookup] in
        let found = await lookup(want)
        finish(found, for: want)
      }
    }
  }

  private func finish(_ found: AppIconFound?, for want: AppIconWant) {
    let key = want.key.text
    active[key] = nil
    guard let request = requests[key] else { return start() }
    guard want.covers(request.want) else {
      // No stale weaker image or miss reaches callers while stronger evidence waits.
      waiting.append(key)
      return start()
    }
    requests[key] = nil
    use += 1
    // Keep an already drawn icon if newer evidence cannot be read, as the windows did before sharing the cache.
    let result = found ?? cached[key]?.found
    cached[key] = AppIconCached(want: want, found: result, used: use)
    if cached.count > limit, let oldest = cached.min(by: { $0.value.used < $1.value.used })?.key {
      cached[oldest] = nil
    }
    for waiter in request.waiters.values { waiter.resume(returning: result) }
    start()
  }

  private func cancel(_ caller: UUID, key: String) {
    guard let waiter = requests[key]?.waiters.removeValue(forKey: caller) else { return }
    waiter.resume(returning: nil)
    if requests[key]?.waiters.isEmpty == true, active[key] == nil {
      requests[key] = nil
      waiting.removeAll { $0 == key }
    }
  }
}
