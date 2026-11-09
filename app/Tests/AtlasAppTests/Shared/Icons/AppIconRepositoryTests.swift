import AsyncSupportTesting
import AtlasService
import Foundation
import Testing

@testable import AtlasApp

@Suite("AppIconRepository")
struct AppIconRepositoryTests {
  @Test("two windows asking for the same icon share one lookup", arguments: [true, false])
  func coalescesRequests(nameOnly: Bool) async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let image = try found("shared")
    let repository = AppIconRepository { _ in
      await gate.read()
      return image
    }
    let want = try #require(AppIconWant.named(bundleId: nameOnly ? nil : "test.icons.shared", name: "Shared"))
    let viewer = Task { await repository.find(want) }
    let panel = Task { await repository.find(want) }
    try await settle("both windows never joined the lookup") { repository.waiterCount == 2 && gate.count == 1 }
    gate.release()
    let first = try #require(await viewer.value)
    let second = try #require(await panel.value)
    #expect(first.image === second.image)
    #expect(gate.count == 1)
    let reopened = try #require(await repository.find(want))
    #expect(reopened.image === first.image)
    #expect(gate.count == 1, "reopening shares the decoded image")
    #expect(repository.waiterCount == 0)
  }

  @Test("all windows together run no more than two icon lookups")
  func limitsConcurrency() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let repository = AppIconRepository { _ in
      await gate.read()
      return nil
    }
    let wants = try (0..<8)
      .map { try want(String($0)) }
    let tasks = wants.map { want in Task { await repository.find(want) } }
    try await settle("the requests never reached the shared queue") { repository.waiterCount == 8 && gate.count == 2 }
    #expect(gate.maximumActive == 2)
    gate.release()
    for task in tasks { _ = await task.value }
    #expect(gate.count == 8)
    #expect(gate.maximumActive == 2)
  }

  @Test("a recorded miss is read only once")
  func readsAMissOnce() async throws {
    let gate = IconLookupGate()
    gate.release()
    let repository = AppIconRepository { _ in
      await gate.read()
      return nil
    }
    let missing = try want("missing")
    #expect(await repository.find(missing) == nil)
    #expect(await repository.find(missing) == nil)
    #expect(gate.count == 1)
  }

  @Test("the decoded cache never holds more than its bound and evicts the least recently used entry")
  func boundsDecodedImages() async throws {
    let gate = IconLookupGate()
    gate.release()
    let image = try found("cached")
    let repository = AppIconRepository(memoryLimit: 3) { _ in
      await gate.read()
      return image
    }
    let wants = try (0..<12)
      .map { try want(String($0)) }
    for want in wants.prefix(3) {
      _ = await repository.find(want)
      #expect(repository.cachedCount <= 3)
    }
    _ = await repository.find(wants[0])
    _ = await repository.find(wants[3])
    _ = await repository.find(wants[0])
    #expect(gate.count == 4, "a cache hit keeps the oldest icon recently used")
    _ = await repository.find(wants[1])
    #expect(gate.count == 5, "the least recently used icon must be read again")
    for want in wants.dropFirst(4) {
      _ = await repository.find(want)
      #expect(repository.cachedCount == 3)
      #expect(repository.images.count == 3, "window snapshots cannot accumulate evicted images")
    }
  }

  @Test("recorded misses share the memory bound")
  func boundsMisses() async throws {
    let repository = AppIconRepository(memoryLimit: 3) { _ in nil }
    for number in 0..<12 {
      #expect(await repository.find(try want(String(number))) == nil)
      #expect(repository.cachedCount <= 3)
    }
    #expect(repository.cachedCount == 3)
  }

  @Test("cancelling one window removes its waiter without cancelling another window's lookup")
  func cancelsCallersIndependently() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let image = try found("shared")
    let repository = AppIconRepository { _ in
      await gate.read()
      return image
    }
    let want = try want("shared")
    let closing = Task { await repository.find(want) }
    let staying = Task { await repository.find(want) }
    try await settle("the callers never joined") { repository.waiterCount == 2 && gate.count == 1 }
    closing.cancel()
    #expect(await closing.value == nil)
    #expect(repository.waiterCount == 1)
    #expect(gate.count == 1)
    gate.release()
    #expect(await staying.value?.sha == "shared")
    #expect(repository.waiterCount == 0)
  }

  @Test("a cancelled queued request does not consume a lookup slot")
  func cancelsQueuedRequests() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let repository = AppIconRepository { _ in
      await gate.read()
      return nil
    }
    let wants = try (0..<3)
      .map { try want(String($0)) }
    let active = wants.prefix(2)
      .map { want in Task { await repository.find(want) } }
    try await settle("the two reads never started") { gate.count == 2 }
    let queued = Task { await repository.find(wants[2]) }
    try await settle("the third request never queued") { repository.waiterCount == 3 }
    queued.cancel()
    #expect(await queued.value == nil)
    gate.release()
    for task in active { _ = await task.value }
    #expect(gate.count == 2)
    #expect(repository.waiterCount == 0)
  }

  private func want(_ name: String) throws -> AppIconWant {
    try #require(AppIconWant.named(bundleId: "test.icons.\(name)", name: name))
  }

  private func found(_ sha: String) throws -> AppIconFound {
    AppIconFound(image: try #require(AppIconStore.picture(png())), sha: sha, bundleId: nil)
  }
}
