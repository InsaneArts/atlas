import SwiftUI

/// Details' column at the window's trailing edge (v3 V3): it slides in from past the edge and back out on
/// ``Motion/slide``, a spring that turns back from wherever it is when Details is toggled again mid-way (R17); under
/// Reduce Motion it fades in place. It slides alone: the stage beside it takes its new width at once
/// (``ViewerLayout/stageTrailing(showsDetails:)``). Leaving, it keeps what it last showed, so it slides away whole
/// rather than emptying first; hidden, it takes no clicks or keyboard focus, and VoiceOver passes it by.
struct ViewerInspector: View {
  /// What Details shows; nil hides it.
  let details: ViewerDetailsState?
  /// Reads how old the newest picture is as its own row draws.
  let newest: @MainActor () -> String
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// What Details last showed, drawn while it slides away.
  @State private var lastShown: ViewerDetailsState?
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Details at its place, or past the window's edge.
  var body: some View {
    ZStack {
      if let shown = details ?? lastShown {
        ViewerDetails(state: shown, newest: newest, send: send)
      }
    }
    .frame(width: ViewerLayout.detailsWidth)
    .frame(maxHeight: .infinity)
    .padding([.trailing, .bottom], Spacing.m)
    .offset(x: ViewerLayout.detailsOffset(isShown: isShown, reduceMotion: reduceMotion))
    .opacity(ViewerLayout.detailsOpacity(isShown: isShown, reduceMotion: reduceMotion))
    .animation(Motion.animation(Motion.slide, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isShown)
    // Hidden past the edge, it takes no clicks, no keyboard focus and no VoiceOver.
    .allowsHitTesting(isShown)
    .disabled(!isShown)
    .accessibilityHidden(!isShown)
    .onChange(of: details, initial: true) { _, details in
      if let details { lastShown = details }
    }
  }

  /// Whether Details shows.
  private var isShown: Bool {
    details != nil
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var isShown = true
    InteractivePreview {
      Toggle(String("Details"), isOn: $isShown)
    } content: {
      HStack(spacing: .zero) {
        Color.fillPlaceholder
          .padding(.trailing, ViewerLayout.stageTrailing(showsDetails: isShown))
          .overlay(alignment: .trailing) {
            ViewerInspector(
              details: isShown ? PreviewStates.viewerDetails.details : nil,
              newest: { String("just now") }
            ) { _ in }
          }
      }
      .frame(width: PreviewStates.viewerNarrowest, height: SampleWidth.column)
      .clipped()
    }
  }
#endif
