import SwiftUI

/// A chosen project folder: green once taught, and removable until then.
struct ProjectChip: View {
  /// The project.
  let project: OnboardingProjectItem
  /// Removes it.
  let remove: @MainActor () -> Void

  /// The folder's name, with a remove button while it can be removed.
  var body: some View {
    HStack(spacing: Spacing.s) {
      Text(project.name)
        .font(Typography.font(.callout))
        .lineLimit(1)
        .truncationMode(.middle)
      if project.isRemovable {
        Button(action: remove) {
          Image(systemName: "xmark")
            .font(Typography.font(.footnote, weight: .bold))
            // The glyph is 9 points; its hit area reaches past it, over the chip's height and its trailing inset,
            // without moving or growing what is drawn.
            .contentShape(
              Rectangle()
                .inset(by: -Layout.hitOutset)
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(project.removeLabel)
      }
    }
    .padding(.horizontal, Spacing.ml)
    .frame(height: ControlHeight.chip)
    .foregroundStyle(project.isTaught ? Color.textSuccess : Color.textPrimary)
    .background(
      Capsule()
        .fill(fill)
    )
  }

  /// The chip's ground: a green tint once taught, a control's fill until then.
  private var fill: Color {
    guard project.isTaught else { return .fillControl }
    return Color.success
      .opacity(Opacity.tint)
  }
}

/// The chip's own measures.
private enum Layout {
  /// 8: how far the remove button's hit area reaches past its glyph on every side, enough to cover the chip's
  /// height (26) around a 9-point glyph.
  static let hitOutset: CGFloat = 8
}

#if DEBUG
  #Preview("Chosen and taught") {
    HStack(spacing: Spacing.s) {
      ForEach(PreviewStates.onboardingAgentsScreen.projects) { project in
        ProjectChip(project: project) {}
      }
      ProjectChip(
        project: OnboardingProjectItem(
          id: "/Users/me/dev/Jolt",
          name: "Jolt",
          isTaught: true,
          isRemovable: false,
          removeLabel: "Remove Jolt"
        )
      ) {}
    }
    .sampleSurface()
  }
#endif
