import SwiftUI

/// Tabs side by side in a capsule track, the selected one on a brighter capsule that glides to it; under Reduce
/// Motion the capsule cross-fades instead. Each tab is a button that VoiceOver reads as selected or not.
struct CapsuleTabs<Tab: Hashable & Sendable>: View {
  /// How closely the tabs' words sit.
  typealias Size = CapsuleTabStyle.Size

  /// The tabs and which one is selected.
  let state: CapsuleTabsState<Tab>
  /// How closely the tabs' words sit.
  var size = Size.regular
  /// Receives the tab the person picks.
  let send: @MainActor (Tab) -> Void
  @Namespace private var thumb
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    HStack(spacing: Spacing.half) {
      ForEach(state.tabs) { tab in
        Button(tab.title) { send(tab.id) }
          .buttonStyle(
            CapsuleTabStyle(isSelected: tab.id == state.selected, size: size, thumb: reduceMotion ? nil : thumb)
          )
          .accessibilityAddTraits(tab.id == state.selected ? .isSelected : [])
      }
    }
    .padding(Layout.trackInset)
    .background(
      CapsuleEdge()
        .fill(Color.fillControl)
    )
    .overlay(
      CapsuleEdge()
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .fixedSize()
    .animation(Motion.animation(Motion.slide, reduceMotion: reduceMotion, reduced: Motion.fade), value: state.selected)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(state.label)
  }
}

/// At file scope because `CapsuleTabs` is generic.
private enum Layout {
  /// 3: the track's inset around its tabs, the board's exact value (v3.py `segmented`). It cannot round: 28-point
  /// tabs and 3 on each side make the track 34 high, the height of the capsules and round buttons beside it in the
  /// top row; 2 or 4 would set the track a point off every one of them.
  static let trackInset: CGFloat = 3
}

#if DEBUG
  #Preview("Samples") {
    CapsuleTabsSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var selected = CapsuleTabsSamples.Pane.screen
    InteractivePreview {
      CapsuleTabs(
        state: CapsuleTabsState(tabs: CapsuleTabsSamples.tabs, selected: selected, label: "What you see")
      ) { tab in
        selected = tab
      }
    }
  }
#endif
