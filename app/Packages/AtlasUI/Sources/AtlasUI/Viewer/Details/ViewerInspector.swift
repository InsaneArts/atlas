import SwiftUI

/// Details' column at the window's trailing edge. On the Screen it slides in from past the edge and back out on
/// ``Motion/slide``, a spring that turns back from wherever it is when Details is toggled again mid-way, while the
/// stage beside it gives up or takes back its room on the same spring (``ViewerLayout/stageSlide(tab:reduceMotion:)``),
/// so the picture's edge keeps to the column's. On Activity, Apps and Screenshots, whose panes take their new width at
/// once, and under Reduce Motion, it fades in place instead, so no blank strip shows while it travels. In a narrow
/// window it floats over the stage's edge on glass with a shadow, leaving the stage its room. Leaving, it keeps what
/// it last showed, so it goes whole rather than emptying first; hidden, it takes no clicks or keyboard focus, and
/// VoiceOver passes it by.
struct ViewerInspector: View {
  /// What Details shows; nil hides it.
  let details: ViewerDetailsState?
  /// Whether it slides in and out rather than fading in place (``ViewerLayout/detailsSlides(tab:reduceMotion:)``).
  let slides: Bool
  /// Whether it floats over the stage rather than taking its room.
  let floats: Bool
  /// Reads how old the newest picture is as its own row draws.
  let newest: @MainActor () -> String
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// What Details last showed, drawn while it goes.
  @State private var lastShown: ViewerDetailsState?

  /// Details at its place, or past the window's edge, or faded out in place.
  var body: some View {
    ZStack {
      if let shown = details ?? lastShown {
        ViewerDetails(state: shown, floats: floats, newest: newest, send: send)
      }
    }
    .frame(width: ViewerLayout.detailsWidth)
    .frame(maxHeight: .infinity)
    .padding([.trailing, .bottom], Spacing.m)
    .offset(x: ViewerLayout.detailsOffset(isShown: isShown, slides: slides))
    .opacity(ViewerLayout.detailsOpacity(isShown: isShown, slides: slides))
    .animation(ViewerLayout.detailsMotion(slides: slides, isShown: isShown), value: isShown)
    // Hidden, it takes no clicks, no keyboard focus and no VoiceOver.
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
    @Previewable @State var slides = true
    InteractivePreview {
      Toggle(String("Details"), isOn: $isShown)
      Toggle(String("Slides"), isOn: $slides)
    } content: {
      HStack(spacing: .zero) {
        Color.fillPlaceholder
          .padding(.trailing, ViewerLayout.stageTrailing(showsDetails: isShown))
          .overlay(alignment: .trailing) {
            ViewerInspector(
              details: isShown ? PreviewStates.viewerDetails.details : nil,
              slides: slides,
              floats: false,
              newest: { String("just now") }
            ) { _ in }
          }
      }
      .frame(width: PreviewStates.viewerNarrowest, height: SampleWidth.column)
      .clipped()
    }
  }
#endif
