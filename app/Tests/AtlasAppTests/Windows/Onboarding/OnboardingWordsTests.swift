import Foundation
import Testing

@testable import AtlasApp

@Suite("OnboardingWords")
struct OnboardingWordsTests {
  /// `app/`, found from this file.
  private static let app = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()

  /// The folders whose sources hold the setup window's words: its own words and presenter, its views and samples, and
  /// the setup rules that word a problem.
  private static let folders = [
    "Sources/AtlasApp/Windows/Onboarding",
    "Packages/AtlasUI/Sources/AtlasUI/Onboarding",
    "Packages/AtlasOnboarding/Sources/AtlasOnboarding",
  ]

  @Test("a person's full name is called by its given name in a sentence, and any other name reads as itself")
  func sentencesUseTheGivenName() {
    #expect(OnboardingWords.named("Christopher Alexander Montgomery-Smith") == "Christopher")
    #expect(OnboardingWords.named("Maria Fernanda Montgomery-Castellanos de la Vega") == "Maria Fernanda")
    #expect(OnboardingWords.named("Ismayil") == "Ismayil")
    #expect(OnboardingWords.named("Test Account") == "Test Account", "no given name starts it")
    #expect(OnboardingWords.named("Atlas") == "the Atlas account")
    #expect(OnboardingWords.logInStep(to: "Christopher Alexander Montgomery-Smith") == "Log in to Christopher")
  }

  @Test("no setup sentence uses a straight apostrophe where the typographic one belongs")
  func noStraightApostrophes() throws {
    var straight: [String] = []
    for folder in Self.folders {
      let root = Self.app
        .appendingPathComponent(folder)
      let files = (FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)?.allObjects ?? [])
        .compactMap { $0 as? URL }
        .filter { $0.pathExtension == "swift" }
      for file in files {
        let lines = try String(contentsOf: file, encoding: .utf8)
          .split(separator: "\n", omittingEmptySubsequences: false)
        for (number, line) in lines.enumerated()
        where Self.literals(in: String(line)).contains(where: { $0.contains("'") }) {
          straight.append("\(file.lastPathComponent):\(number + 1)")
        }
      }
    }
    #expect(straight.isEmpty, "straight apostrophes at \(straight)")
  }

  /// The string literals on one line of code, leaving out a comment line.
  ///
  /// - Parameter line: The line.
  /// - Returns: Each literal's text, without its quotes.
  private static func literals(in line: String) -> [String] {
    guard !line.trimmingCharacters(in: .whitespaces).hasPrefix("//") else { return [] }
    let pattern = /"((?:[^"\\]|\\.)*)"/
    return line.matches(of: pattern)
      .map { String($0.output.1) }
  }
}
