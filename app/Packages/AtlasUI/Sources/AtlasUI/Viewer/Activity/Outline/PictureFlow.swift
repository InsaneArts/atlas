import SwiftUI

/// Pictures in a row that wraps onto the next line where the width runs out, each line starting at the leading edge.
struct PictureFlow: Layout {
  /// Between two pictures, across and down.
  let spacing: CGFloat

  /// The lines' size: the widest line, and every line's height with the spacing between.
  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let lines = lines(of: subviews, width: proposal.width ?? .infinity)
    let width =
      lines.map(\.width)
      .max() ?? .zero
    let height =
      lines.map(\.height)
      .reduce(.zero, +) + spacing * CGFloat(max(lines.count - 1, 0))
    return CGSize(width: width, height: height)
  }

  /// Places each line's pictures from the leading edge, line under line.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    var top = bounds.minY
    for line in lines(of: subviews, width: bounds.width) {
      var leading = bounds.minX
      for index in line.indices {
        let size = subviews[index].sizeThatFits(.unspecified)
        subviews[index].place(at: CGPoint(x: leading, y: top), anchor: .topLeading, proposal: ProposedViewSize(size))
        leading += size.width + spacing
      }
      top += line.height + spacing
    }
  }

  /// The pictures in lines no wider than `width`, each line's indices with its width and height.
  ///
  /// - Parameters:
  ///   - subviews: The pictures.
  ///   - width: The room across.
  /// - Returns: The lines.
  private func lines(of subviews: Subviews, width: CGFloat) -> [(indices: [Int], width: CGFloat, height: CGFloat)] {
    var lines: [(indices: [Int], width: CGFloat, height: CGFloat)] = []
    for index in subviews.indices {
      let size = subviews[index].sizeThatFits(.unspecified)
      if var line = lines.last, line.width + spacing + size.width <= width {
        line.indices.append(index)
        line.width += spacing + size.width
        line.height = max(line.height, size.height)
        lines[lines.count - 1] = line
      } else {
        lines.append(([index], size.width, size.height))
      }
    }
    return lines
  }
}
