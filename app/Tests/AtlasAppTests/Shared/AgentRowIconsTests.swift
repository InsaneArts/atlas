import AtlasCommands
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("Agent row icons")
struct AgentRowIconsTests {
  @Test("agents use their bundled logos when the app is absent")
  func bundledLogos() {
    for id in AgentRowIcons.apps.keys {
      guard case .app(let logo) = AgentRowIcons.icon(id, [:]) else {
        Issue.record("Known agent \(id) resolved to a tile")
        continue
      }
      let expected =
        switch id {
        case AgentID.codex.rawValue: AgentLogos.codex
        case AgentID.cursor.rawValue: AgentLogos.cursor
        default: AgentLogos.claude
        }
      #expect(logo == expected)
      #expect(logo.image.width == Int(FileIcons.pixels))
      #expect(logo.image.height == logo.image.width)
    }
  }

  @Test("each known agent prefers its installed app's live icon over the bundled logo")
  func installedLogos() {
    let home = URL(fileURLWithPath: "/Users/example")
    let icons = AgentRowIcons.onThisMac(home: home) { _ in AgentLogos.codex.image }
    for (id, app) in AgentRowIcons.apps {
      let expected = IconImage(id: "/Applications/\(app)", image: AgentLogos.codex.image)
      #expect(AgentRowIcons.icon(id, icons) == .app(expected))
    }
    #expect(icons[AgentID.claude.rawValue] == icons[AgentsStatus.claudeAppID])
  }

  @Test("apps in the person's Applications folder supply the same live logos to command line agents")
  func homeApplications() {
    let home = URL(fileURLWithPath: "/Users/example")
    let icons = AgentRowIcons.onThisMac(home: home) { path in
      path.hasPrefix(home.path) ? AgentLogos.claude.image : nil
    }
    for (id, app) in AgentRowIcons.apps {
      #expect(icons[id]?.id == "/Users/example/Applications/\(app)")
    }
  }
}
