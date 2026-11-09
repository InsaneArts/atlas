import AtlasUI
import Foundation

/// The app-owned icon repository and its disk store. Copies handed to windows share the same requests, misses and
/// bounded decoded cache; the source keeps the disk, bundle identifier and name lookup order.
nonisolated struct AppIcons: Sendable {
  /// One scheduler and decoded cache shared by every window receiving these icons.
  let repository: AppIconRepository

  /// Creates a repository over injected reads, without reaching the workspace itself.
  init(store: AppIconStore, lookup: AppIconLookup) {
    let source = AppIconSource(store: store, lookup: lookup)
    repository = AppIconRepository { want in await source.find(want) }
  }

  /// No icons found and none kept, as in tests and renders.
  static var none: AppIcons {
    AppIcons(store: AppIconStore(caches: nil), lookup: .none)
  }

  /// The person's own: kept in their Atlas caches, read through the workspace.
  ///
  /// - Parameter home: The person's home folder.
  /// - Returns: The icons.
  static func live(home: URL) -> AppIcons {
    AppIcons(store: .live(home: home), lookup: .workspace(home: home.path))
  }

  /// Finds an icon through the shared repository, or returns a recorded miss.
  func find(_ want: AppIconWant) async -> AppIconFound? {
    await repository.find(want)
  }

  /// Bridges a window's synchronous request to the shared async API. The receiver should capture its window
  /// weakly, retain the task by icon key, and cancel it when the icon is no longer wanted. Snapshots contain only
  /// the repository's bounded images.
  @MainActor @discardableResult func request(
    _ want: AppIconWant,
    receive: @escaping @MainActor (AppIconFound?, [String: IconImage]) -> Void
  ) -> Task<Void, Never> {
    Task { [repository] in
      guard !Task.isCancelled else { return }
      let found = await repository.find(want)
      guard !Task.isCancelled else { return }
      receive(found, repository.images)
    }
  }
}
