import Foundation
import Testing

@Suite("Words punctuation")
struct WordsPunctuationTests {
  /// The app's sources, found from this file: `app/Tests/AtlasAppTests/Shared/Words` up to `app`, then down.
  private static let sources = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appending(path: "Sources/AtlasApp")

  /// Every string literal in the app's words files, with where it is.
  ///
  /// - Returns: Each literal's file, line and text.
  /// - Throws: When a file can't be read.
  private static func literals() throws -> [(file: String, line: Int, text: String)] {
    let manager = FileManager.default
    let listed = manager.enumerator(at: sources, includingPropertiesForKeys: nil)?
      .compactMap { $0 as? URL }
    let files = (listed ?? [])
      .filter { $0.lastPathComponent.contains("Words") && $0.pathExtension == "swift" }
    let literal = /"((?:[^"\\]|\\.)*)"/
    var found: [(file: String, line: Int, text: String)] = []
    for file in files {
      let lines = try String(contentsOf: file, encoding: .utf8)
        .components(separatedBy: "\n")
      for (index, line) in lines.enumerated() where !line.trimmingCharacters(in: .whitespaces).hasPrefix("//") {
        for match in line.matches(of: literal) {
          found.append((file.lastPathComponent, index + 1, String(match.output.1)))
        }
      }
    }
    return found
  }

  @Test("the words files are found, so the rule below checks something")
  func theWordsAreFound() throws {
    #expect(try Self.literals().count > 100)
  }

  @Test("every message of two sentences in the app's words ends with a full stop")
  func twoSentencesEndWithAFullStop() throws {
    let twoSentences = /[.!?] [A-Z“]/
    let ending = /[.?!…]$/
    for (file, line, text) in try Self.literals() where text.contains(twoSentences) {
      #expect(text.contains(ending), "\(file):\(line) \(text)")
    }
  }
}
