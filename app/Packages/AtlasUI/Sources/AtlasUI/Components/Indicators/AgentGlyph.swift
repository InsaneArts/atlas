import Foundation
import SwiftUI

/// An original SVG silhouette, fitted against the combined bounds of all its colour layers.
struct AgentGlyph: Shape {
  /// The absolute SVG path commands for this layer.
  let data: String
  /// All the mark's layers, so each shares the same scale and origin.
  let layers: [String]

  /// Fits this layer into the middle of the card, keeping its original proportions.
  ///
  /// - Parameter rect: The card's drawing bounds.
  /// - Returns: The fitted silhouette.
  func path(in rect: CGRect) -> Path {
    var combined = Path()
    for layer in layers { combined.addPath(Self.parse(layer)) }
    let bounds = combined.boundingRect
    guard bounds.width > 0, bounds.height > 0 else { return Path() }
    let scale = min(rect.width, rect.height) * BrandMark.glyphFraction / max(bounds.width, bounds.height)
    let transform = CGAffineTransform(translationX: rect.midX, y: rect.midY)
      .scaledBy(x: scale, y: scale)
      .translatedBy(x: -bounds.midX, y: -bounds.midY)
    return Self.parse(data)
      .applying(transform)
  }

  /// Reads the absolute move, line, horizontal, vertical, curve and close commands used by the brand files.
  ///
  /// - Parameter data: An SVG path's absolute commands.
  /// - Returns: The path, or an empty path for unsupported or incomplete input.
  static func parse(_ data: String) -> Path {
    let separated = data.replacingOccurrences(of: "([MLHVCZ])", with: " $1 ", options: .regularExpression)
    let scanner = Scanner(string: separated)
    scanner.charactersToBeSkipped = .whitespacesAndNewlines.union(CharacterSet(charactersIn: ","))
    var path = Path()
    var point = CGPoint.zero
    var command = ""
    while !scanner.isAtEnd {
      if let next = scanner.scanCharacters(from: .letters) {
        command = next
        if command == "Z" {
          path.closeSubpath()
          command = ""
          continue
        }
      }
      guard let first = scanner.scanDouble() else { return Path() }
      switch command {
      case "M", "L":
        guard let y = scanner.scanDouble() else { return Path() }
        point = CGPoint(x: first, y: y)
        if command == "M" { path.move(to: point) } else { path.addLine(to: point) }
        command = "L"
      case "H":
        point.x = first
        path.addLine(to: point)
      case "V":
        point.y = first
        path.addLine(to: point)
      case "C":
        guard let y1 = scanner.scanDouble(), let x2 = scanner.scanDouble(), let y2 = scanner.scanDouble(),
          let x = scanner.scanDouble(), let y = scanner.scanDouble()
        else { return Path() }
        point = CGPoint(x: x, y: y)
        path.addCurve(to: point, control1: CGPoint(x: first, y: y1), control2: CGPoint(x: x2, y: y2))
      default: return Path()
      }
    }
    return path
  }
}
