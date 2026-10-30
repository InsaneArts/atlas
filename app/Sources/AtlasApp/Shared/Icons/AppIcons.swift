import AtlasUI
import Foundation

/// The app-owned icon repository and its disk store. Copies handed to windows share the same requests, misses and
/// bounded decoded cache; the source keeps the live, disk, bundle identifier and name lookup order.
nonisolated struct AppIcons: Sendable {
  /// The icons kept between launches.
  let store: AppIconStore
  /// The person's home folder, for their own Applications folder.
  let home: String
  /// One scheduler and decoded cache shared by every window receiving these icons.
  let repository: AppIconRepository

  /// Creates a repository over injected reads, without reaching the workspace itself.
  init(store: AppIconStore, lookup: AppIconLookup, home: String) {
    self.store = store
    self.home = home
    let source = AppIconSource(store: store, lookup: lookup, home: home)
    repository = AppIconRepository { want in await source.find(want) }
  }

  /// No icons found and none kept, as in tests and renders.
  static var none: AppIcons {
    AppIcons(store: AppIconStore(caches: nil), lookup: .none, home: "")
  }

  /// The person's own: kept in their Atlas caches, read through the workspace.
  ///
  /// - Parameter home: The person's home folder.
  /// - Returns: The icons.
  static func live(home: URL) -> AppIcons {
    AppIcons(store: .live(home: home), lookup: .workspace(home: home.path), home: home.path)
  }

  /// Finds an icon through the shared repository, or returns a recorded miss.
  func find(_ want: AppIconWant) async -> AppIconFound? {
    await repository.find(want)
  }

  /// Bridges a window's synchronous request to the shared async API. The receiver should capture its window
  /// weakly, retain the task by icon key, and cancel it when the icon is no longer wanted. Snapshots contain only
  /// the repository's bounded images. `latest` supplies evidence received before the task starts; once it is
  /// waiting, the owner can pass newer evidence to the repository without another request.
  @MainActor @discardableResult func request(
    _ want: AppIconWant,
    latest: @escaping @MainActor () -> AppIconWant? = { nil },
    receive: @escaping @MainActor (AppIconFound?, [String: IconImage]) -> Void
  ) -> Task<Void, Never> {
    Task { [repository] in
      guard !Task.isCancelled else { return }
      var requested = latest() ?? want
      var found = await repository.find(requested)
      // Evidence may change after the repository finishes but before this task resumes to deliver its result.
      while !Task.isCancelled, let newer = latest(), !requested.covers(newer) {
        requested = newer.keepingEvidence(from: requested)
        found = await repository.find(requested)
      }
      guard !Task.isCancelled else { return }
      receive(found, repository.images)
    }
  }
}
