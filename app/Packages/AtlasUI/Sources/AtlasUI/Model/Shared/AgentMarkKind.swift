import SwiftUI

/// The original brand mark an agent's connection row draws, or the prompt a terminal session shows.
public enum AgentMarkKind: Equatable, Sendable {
  /// Pi's three-colour mark on graphite.
  case pi
  /// opencode's original frame on graphite.
  case opencode
  /// A white `>_` prompt on graphite, for commands from a terminal and for a host Atlas does not know.
  case terminal

  /// The card's colours, from its top edge to its bottom.
  var colors: [Color] {
    switch self {
    case .pi: [.brandPiTop, .brandPiBottom]
    case .opencode: [.brandOpencodeTop, .brandOpencodeBottom]
    case .terminal: [.brandTerminalTop, .brandTerminalBottom]
    }
  }

  /// Each independently coloured part of the original glyph.
  var paths: [String] {
    switch self {
    case .pi:
      [
        "M165.29 165.29H517.36V400H400V282.65H165.29Z",
        "M165.29 282.65H282.65V400H400V517.36H282.65V634.72H165.29Z",
        "M517.36 400H634.72V634.72H517.36Z",
      ]
    case .opencode:
      [
        "M 320 224V352H192V224H320Z",
        "M 384 416H128V96H384V416ZM320 160H192V352H320V160Z",
      ]
    case .terminal:
      [
        "M104 168L144 128L272 256L144 384L104 344L192 256Z",
        "M288 328H416V384H288Z",
      ]
    }
  }

  /// The colours corresponding to the glyph's paths.
  var inks: [Color] {
    switch self {
    case .pi: [.brandPiCoral, .brandPiBlue, .brandPiGold]
    case .opencode: [.brandOpencodeInner, .brandWhite]
    case .terminal: [.brandWhite, .brandWhite]
    }
  }
}
