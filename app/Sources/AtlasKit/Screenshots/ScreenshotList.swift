import Foundation

/// `screenshots.json`'s body: the screenshots agents took, newest first, and the folder they are kept in. Every field
/// may be missing, so each has a safe default.
package struct ScreenshotList: Decodable, Equatable, Sendable {
  /// The screenshots, newest first; none when the list is missing or of the wrong type.
  package var shots: [Screenshot]
  /// The folder the screenshots are kept in; nil when the service names none, or a path that is not absolute.
  package var folder: URL?

  /// Creates a list.
  ///
  /// - Parameters:
  ///   - shots: The screenshots, newest first.
  ///   - folder: The folder they are kept in.
  package init(shots: [Screenshot] = [], folder: URL? = nil) {
    self.shots = shots
    self.folder = folder
  }

  /// The body's fields this app reads.
  enum CodingKeys: String, CodingKey {
    case shots = "screenshots"
    case folder
  }

  /// Decodes the body, keeping no screenshots when the list is missing or of the wrong type, and no folder unless it
  /// is an absolute path.
  ///
  /// - Parameter decoder: The decoder reading `screenshots.json`.
  /// - Throws: `DecodingError` only when the body is not an object.
  package init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    shots = (try? container.decodeIfPresent([Screenshot].self, forKey: .shots)) ?? []
    let path = try? container.decodeIfPresent(String.self, forKey: .folder)
    folder = path.flatMap { $0.hasPrefix("/") ? URL(fileURLWithPath: $0, isDirectory: true) : nil }
  }

  /// The file of a listed screenshot: its name inside the folder, and only a name the list holds that names one file
  /// in it, never a path.
  ///
  /// - Parameter name: The screenshot's file name.
  /// - Returns: The file, or nil when the folder is unknown or the name is not a listed one.
  package func file(named name: String) -> URL? {
    guard let folder, !name.isEmpty, name != ".", name != "..", !name.contains("/"),
      shots.contains(where: { $0.name == name })
    else { return nil }
    return folder.appendingPathComponent(name, isDirectory: false)
  }
}
