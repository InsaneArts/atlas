import SwiftUI

/// An action's two pictures compared, each with its mark: the screen before alone, the screen after alone, or one
/// picture with a line the person drags, before to its left and after to its right, a label over each side shown.
/// Show changes lays a shade over what stayed the same.
struct ActionCompare: View {
  /// The action shown large.
  let large: ViewerActionLarge
  /// Where the line between before and after stands, as a share of the picture's width.
  @State private var cut = ViewerLayout.compareStart

  /// The pictures, the shade, the labels and the line.
  var body: some View {
    let mode = large.shownCompare
    GeometryReader { proxy in
      ZStack(alignment: .topLeading) {
        MarkedPicture(picture: large.before, mark: large.mark)
        if mode != .before {
          MarkedPicture(picture: large.after, mark: large.mark)
            .mask(alignment: .leading) {
              Rectangle()
                .frame(width: mode == .slide ? proxy.size.width * (1 - cut) : proxy.size.width)
                .frame(maxWidth: .infinity, alignment: .trailing)
            }
        }
        if large.showsChanges, let changes = large.changes {
          ActionChangesShade(changes: changes)
        }
        labels(mode)
        if mode == .slide {
          line(at: proxy.size.width * cut)
        }
      }
      .contentShape(Rectangle())
      .gesture(sliding(width: proxy.size.width), isEnabled: mode == .slide)
    }
    .clipShape(RoundedRectangle(cornerRadius: Radius.large, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    }
    .accessibilityElement()
    .accessibilityLabel(large.words.text)
    .accessibilityAddTraits(.isImage)
    .accessibilityAdjustableAction { direction in
      guard mode == .slide else { return }
      let step = ViewerLayout.compareStart / 5
      cut = min(1, max(0, cut + (direction == .increment ? step : -step)))
    }
  }

  /// The label over each side shown: before at the top left, after at the top right.
  ///
  /// - Parameter mode: How the pictures are compared.
  /// - Returns: The labels.
  private func labels(_ mode: ViewerCompareMode) -> some View {
    HStack {
      if mode != .after {
        label(large.beforeLabel)
      }
      Spacer(minLength: Spacing.s)
      if mode != .before {
        label(large.afterLabel)
      }
    }
    .padding(Spacing.sPlus)
  }

  /// One side's label, on a dark capsule.
  ///
  /// - Parameter text: What it says.
  /// - Returns: The label.
  private func label(_ text: String) -> some View {
    Text(verbatim: text)
      .font(Typography.digits(.caption, weight: .semibold))
      .foregroundStyle(Color.textOverPicture)
      .padding(.horizontal, Spacing.s)
      .padding(.vertical, Spacing.half)
      .background(Capsule().fill(Color.fillOverPicture))
  }

  /// The line between before and after, with its round handle.
  ///
  /// - Parameter x: Where it stands.
  /// - Returns: The line.
  private func line(at x: CGFloat) -> some View {
    Rectangle()
      .fill(Color.markHalo)
      .frame(width: ViewerLayout.compareLine)
      .overlay {
        Circle()
          .fill(Color.markHalo)
          .frame(width: ViewerLayout.compareHandle, height: ViewerLayout.compareHandle)
          .shadow(color: Color.dropShadow, radius: Spacing.xs)
      }
      .shadow(color: Color.dropShadowSoft, radius: Stroke.regular)
      .frame(maxHeight: .infinity)
      .offset(x: x - ViewerLayout.compareLine / 2)
      .allowsHitTesting(false)
  }

  /// Dragging across the picture moves the line to the pointer.
  ///
  /// - Parameter width: The picture's width.
  /// - Returns: The gesture.
  private func sliding(width: CGFloat) -> some Gesture {
    DragGesture(minimumDistance: .zero)
      .onChanged { drag in
        guard width > 0 else { return }
        cut = min(1, max(0, drag.location.x / width))
      }
  }
}
