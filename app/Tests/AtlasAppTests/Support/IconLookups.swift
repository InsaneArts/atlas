import Foundation

@testable import AtlasApp

/// What a stand-in icon lookup was asked, in order, so a test sees which way each icon was found.
actor IconLookups {
  /// Each read, as `icon <bundle>`, `id <bundle id>`, `name <name>` or `bundleId <bundle>`.
  private(set) var asked: [String] = []
  /// Those waiting for a number of reads, with that number.
  private var waiting: [(count: Int, resume: CheckedContinuation<Void, Never>)] = []

  /// Notes one read, and lets go those waiting for as many.
  ///
  /// - Parameter read: The read.
  func note(_ read: String) {
    asked.append(read)
    let due = waiting.filter { $0.count <= asked.count }
    waiting.removeAll { $0.count <= asked.count }
    for waiter in due { waiter.resume.resume() }
  }

  /// Returns once `count` reads were made: a sentinel for a test whose next step needs them done.
  ///
  /// - Parameter count: The reads.
  func until(_ count: Int) async {
    guard asked.count < count else { return }
    await withCheckedContinuation { resume in waiting.append((count, resume)) }
  }

  /// A lookup that finds the icons of `bundles`, the bundles of `ids` and of `names`, and notes each read.
  ///
  /// - Parameters:
  ///   - icons: Each bundle's icon PNG.
  ///   - ids: Each bundle identifier's bundle.
  ///   - names: Each name's bundle.
  ///   - declared: Each bundle's declared bundle identifier.
  /// - Returns: The lookup.
  nonisolated func lookup(
    icons: [String: Data] = [:],
    ids: [String: String] = [:],
    names: [String: String] = [:],
    declared: [String: String] = [:]
  ) -> AppIconLookup {
    AppIconLookup(
      icon: { bundle in
        await self.note("icon \(bundle)")
        return icons[bundle]
      },
      bundleOfId: { id in
        await self.note("id \(id)")
        return ids[id]
      },
      bundleNamed: { name in
        await self.note("name \(name)")
        return names[name]
      },
      bundleId: { bundle in
        await self.note("bundleId \(bundle)")
        return declared[bundle]
      }
    )
  }
}

/// A folder of its own for one test, removed when the test is done with it.
struct TemporaryFolder {
  /// The folder.
  let url: URL

  /// Makes a new, empty folder.
  init() throws {
    url = FileManager.default.temporaryDirectory
      .appendingPathComponent("atlas-icons-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default
      .createDirectory(at: url, withIntermediateDirectories: true)
  }

  /// Removes the folder and what is in it.
  func remove() {
    try? FileManager.default
      .removeItem(at: url)
  }
}
