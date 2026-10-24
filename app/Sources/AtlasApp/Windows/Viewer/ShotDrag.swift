import AtlasService
import Foundation
import UniformTypeIdentifiers

/// A screenshot dragged out of the Screenshots tab: the item the system carries, which reads the picture as taken
/// only once it is dropped, as PNG data and as a file for the Finder. The file is written to one folder of the app's
/// own under the temporary folder, emptied at the next drag and when the app quits, so drags leave nothing behind.
enum ShotDrag {
  /// The one folder a dropped file is written to before the system copies it where it was dropped.
  static var folder: URL {
    FileManager.default.temporaryDirectory
      .appendingPathComponent("Atlas screenshots", isDirectory: true)
  }

  /// Empties the folder, as a new drag starts and as the app quits.
  static func emptyFolder() {
    try? FileManager.default
      .removeItem(at: folder)
  }

  /// The item for the screenshot `name`.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - suggestedName: What a dropped file is called, without its extension.
  ///   - service: The viewer service, which reads the picture.
  /// - Returns: The item.
  static func provider(name: String, suggestedName: String, service: any ViewerService) -> NSItemProvider {
    emptyFolder()
    let folder = folder
    let provider = NSItemProvider()
    provider.suggestedName = suggestedName
    // Registered by type identifier: Foundation marks these handlers and their answers `@Sendable`, since the provider
    // asks and takes its answer on any thread; the `UTType` forms leave the answer unmarked. Each handler is
    // nonisolated, and so is its task. Unstructured, as the provider asks synchronously and takes the answer later.
    let type = UTType.png.identifier
    provider.registerDataRepresentation(forTypeIdentifier: type, visibility: .all) { done in
      Task {
        do {
          done(try await service.screenshot(name: name, full: true), nil)
        } catch {
          done(nil, error)
        }
      }
      return nil
    }
    provider.registerFileRepresentation(forTypeIdentifier: type, fileOptions: [], visibility: .all) { done in
      Task {
        do {
          let png = try await service.screenshot(name: name, full: true)
          try FileManager.default
            .createDirectory(at: folder, withIntermediateDirectories: true)
          let url = folder.appendingPathComponent(suggestedName)
            .appendingPathExtension(for: .png)
          try png.write(to: url, options: .atomic)
          done(url, false, nil)
        } catch {
          done(nil, false, error)
        }
      }
      return nil
    }
    return provider
  }
}
