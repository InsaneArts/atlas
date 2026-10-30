import Foundation
import Testing

/// The app shell and its supporting UI and service source layout: rules a compiler cannot check.
@Suite("App shell layout")
struct AppShellLayoutTests {
  /// `app/Sources/AtlasApp`, found from this file.
  private static let sources = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/AtlasApp")

  /// Every Swift file of the app target, by its path under ``sources``, with its code: comment lines and trailing
  /// comments dropped, so a doc line naming a type is not a use of it.
  ///
  /// - Returns: The files.
  /// - Throws: A file that cannot be read.
  private static func code() throws -> [String: String] {
    let walk = FileManager.default
      .enumerator(at: sources, includingPropertiesForKeys: nil)
    let files = (walk?.allObjects ?? [])
      .compactMap { $0 as? URL }
      .filter { $0.pathExtension == "swift" }
    var code: [String: String] = [:]
    for file in files {
      let text = try String(contentsOf: file, encoding: .utf8)
      let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        .map(Self.dropComment)
      code[String(file.path.dropFirst(sources.path.count + 1))] = lines.joined(separator: "\n")
    }
    return code
  }

  /// A line without its comment: what comes before its first `//` outside a string literal.
  ///
  /// - Parameter line: The line.
  /// - Returns: Its code.
  private static func dropComment(_ line: Substring) -> String {
    var inString = false
    var escaped = false
    var previous: Character?
    for index in line.indices {
      let character = line[index]
      if escaped {
        escaped = false
      } else if inString && character == "\\" {
        escaped = true
      } else if character == "\"" {
        inString.toggle()
      } else if !inString && character == "/" && previous == "/" {
        return String(line[..<line.index(before: index)])
      }
      previous = character
    }
    return String(line)
  }

  @Test("the shell, presenter, sidebar and Activity source folders hold at most ten Swift files")
  func sourceFoldersStaySmall() throws {
    let app = Self.sources
      .deletingLastPathComponent()
      .deletingLastPathComponent()
    let roots = [
      Self.sources.appendingPathComponent("App"),
      Self.sources.appendingPathComponent("Shared"),
      Self.sources.appendingPathComponent("Windows/Viewer/Presenting"),
      app.appendingPathComponent("Packages/AtlasUI/Sources/AtlasUI/Viewer/Sidebar"),
      app.appendingPathComponent("Packages/AtlasService/Sources/AtlasService/Activity"),
    ]
    var crowded: [String] = []
    for root in roots {
      let walk = FileManager.default
        .enumerator(at: root, includingPropertiesForKeys: nil)
      let files = (walk?.allObjects ?? [])
        .compactMap { $0 as? URL }
        .filter { $0.pathExtension == "swift" }
      let folders = Dictionary(grouping: files) {
        $0.deletingLastPathComponent()
          .path
      }
      crowded += folders.filter { $0.value.count > 10 }
        .map(\.key)
    }
    #expect(crowded.isEmpty, "\(crowded.sorted())")
  }

  @Test("only the window shell makes a hosting view, and only Appearance names the dark appearance")
  func hostingAndTheDarkAppearanceAreWrittenOnce() throws {
    let code = try Self.code()
    let hosts = code.filter { $0.value.contains("NSHostingView(") || $0.value.contains("NSHostingController(") }
    let dark = code.filter { $0.value.contains(".darkAqua") }
    #expect(hosts.keys.sorted() == ["Shared/Windows/WindowShell.swift"])
    #expect(dark.keys.sorted() == ["Shared/Windows/Appearance.swift"])
  }

  @Test("no window, popover or menu sets its own appearance, so each follows the Mac's; only the renders name one")
  func nothingOnScreenForcesAnAppearance() throws {
    let appearance = try Regex(#"\.appearance\s*=(?!=)"#)
    let forcing = try Self.code()
      .filter { path, text in !path.hasPrefix("Rendering/") && text.contains(appearance) }
    #expect(forcing.isEmpty, "\(forcing.keys.sorted())")
    let named = try Self.code()
      .filter { path, text in
        !path.hasPrefix("Rendering/") && path != "AtlasApp.swift" && path != "Shared/Windows/Appearance.swift"
          && (text.contains("Appearance.dark") || text.contains("Appearance.light"))
      }
    #expect(named.isEmpty, "\(named.keys.sorted())")
  }

  @Test("no window is built, and no controller made, outside App")
  func windowsAndControllersAreBuiltInApp() throws {
    let window = try Regex(#"\b[A-Z][A-Za-z]*Window\(controller:"#)
    let controller = try Regex(#"\b[A-Z][A-Za-z]*Controller\("#)
    let building = try Self.code()
      .filter { path, text in
        let controllers = text.matches(of: controller)
          .filter { !text[$0.range].hasPrefix("NSHosting") }
        return !path.hasPrefix("App/") && (text.contains(window) || !controllers.isEmpty)
      }
    #expect(building.isEmpty, "\(building.keys.sorted())")
  }

  @Test("no controller reaches the standard defaults, the shared workspace or the process")
  func controllersReachNoSingleton() throws {
    let reaching = try Self.code()
      .filter { path, text in
        path.hasPrefix("Windows/") && path.contains("Controller")
          && ["UserDefaults.standard", "NSWorkspace.shared", "ProcessInfo"].contains { text.contains($0) }
      }
    #expect(reaching.isEmpty, "\(reaching.keys.sorted())")
  }

  @Test("no window's folder names a type declared in another window's folder")
  func windowsNeverNameEachOther() throws {
    let code = try Self.code()
    let window = { (path: String) -> String? in
      let parts = path.split(separator: "/")
      return parts.count > 1 ? String(parts[1]) : nil
    }
    let declaration = try Regex(
      #"(?m)^\s*(?:(?:private|fileprivate|internal|public|final|nonisolated)\s+)*(?:class|struct|enum|protocol|actor) ([A-Z][A-Za-z]+)"#
    )
    var owners: [String: Set<String>] = [:]
    for (path, text) in code where path.hasPrefix("Windows/") {
      for match in text.matches(of: declaration) {
        if let name = match.output[1].substring, let owner = window(path) {
          owners[String(name), default: []].insert(owner)
        }
      }
    }
    // A nested name two windows each declare for themselves, such as a private `Layout`, belongs to neither.
    let declared = owners.compactMapValues { $0.count == 1 ? $0.first : nil }
    var crossings: [String] = []
    for (path, text) in code where path.hasPrefix("Windows/") {
      let tokens = text.split { !$0.isLetter && !$0.isNumber }
      let words = Set(tokens.map(String.init))
      for (name, owner) in declared where window(path) != owner && words.contains(name) {
        crossings.append("\(path) names \(name)")
      }
    }
    #expect(crossings.isEmpty, "\(crossings.sorted())")
  }
}
