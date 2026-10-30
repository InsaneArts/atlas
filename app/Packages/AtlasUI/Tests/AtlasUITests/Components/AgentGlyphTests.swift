import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("Agent glyphs")
struct AgentGlyphTests {
  @Test("all Pi colour layers share one scale and remain centred in the card")
  func piLayers() {
    let paths = AgentMarkKind.pi.paths
    var combined = CGRect.null
    for data in paths {
      let glyph = AgentGlyph(data: data, layers: paths)
      let bounds = glyph.path(in: CGRect(x: 0, y: 0, width: 26, height: 26)).boundingRect
      #expect(!bounds.isEmpty)
      combined = combined.union(bounds)
    }
    #expect(abs(combined.midX - 13) < 0.001)
    #expect(abs(combined.midY - 13) < 0.001)
    #expect(abs(combined.width - 13) < 0.001)
  }

  @Test("an incomplete or unsupported path draws nothing instead of trapping")
  func invalidPaths() {
    for data in ["M", "M 0", "M 0 0Q 2 3 4 5", "M 0 0C 1 2", "M 0 0X 1"] {
      #expect(AgentGlyph.parse(data).isEmpty)
    }
  }

  @Test("opencode's adjacent close and move commands keep the white frame and its inner hole")
  func opencodeFrame() throws {
    let paths = AgentMarkKind.opencode.paths
    let data = try #require(paths.last)
    let path = AgentGlyph.parse(data)
    #expect(!path.isEmpty)
    #expect(path.boundingRect == CGRect(x: 128, y: 96, width: 256, height: 320))
    #expect(path.contains(CGPoint(x: 150, y: 200), eoFill: true))
    #expect(!path.contains(CGPoint(x: 256, y: 200), eoFill: true))
  }

  @Test("the terminal prompt's chevron and underscore share one baseline and stay centred in the card")
  func terminalPrompt() throws {
    let paths = AgentMarkKind.terminal.paths
    let chevron = try #require(paths.first.map(AgentGlyph.parse))
    let underscore = try #require(paths.last.map(AgentGlyph.parse))
    #expect(chevron.boundingRect.maxY == underscore.boundingRect.maxY)
    #expect(chevron.boundingRect.maxX < underscore.boundingRect.minX)
    #expect(chevron.contains(CGPoint(x: 232, y: 256), eoFill: true))
    #expect(!chevron.contains(CGPoint(x: 150, y: 256), eoFill: true))
    var combined = CGRect.null
    for data in paths {
      let glyph = AgentGlyph(data: data, layers: paths)
      combined = combined.union(glyph.path(in: CGRect(x: 0, y: 0, width: 26, height: 26)).boundingRect)
    }
    #expect(abs(combined.midX - 13) < 0.001)
    #expect(abs(combined.midY - 13) < 0.001)
  }
}
