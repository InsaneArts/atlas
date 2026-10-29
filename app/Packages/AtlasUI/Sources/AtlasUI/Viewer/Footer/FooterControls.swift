import SwiftUI

/// The footer's buttons at its trailing edge, 6 apart: the picture's pause, Stop or Resume, and the main capsule. Only
/// the actions that apply are drawn: each stands in its slot, growing in from its leading edge as it applies and
/// shrinking away as it stops applying, its neighbours closing the gap (``ActionSlot``), while the main capsule's
/// trailing edge keeps its place. Pressing a button never moves it or its neighbours: each keeps one width through
/// every label it takes. In a narrow card Stop and Resume stand as a circle round their symbol and Take Control as its
/// hand alone, as ``FooterFold`` says. Each tooltip names its menu item and shortcut, so every shortcut can be found from its
/// button.
struct FooterControls: View {
  /// The buttons' states.
  let controls: ViewerFooterControls
  /// How the footer draws in a narrow card.
  var fold = FooterFold(.full)
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The buttons that apply, each in its slot.
  var body: some View {
    HStack(spacing: ViewerLayout.footerGap) {
      ViewerPauseSlot(pause: controls.pause, send: send)
      FooterAgentButton(agent: controls.agent, help: controls.agentHelp, fold: fold, send: send)
      ViewerMainButton(control: controls.control, fold: fold, send: send)
    }
    .fixedSize()
    .animatesActionSlots(controls.pause != nil)
    .animatesActionSlots(controls.agent != nil)
    .animatesActionSlots(controls.control != nil)
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      FooterControls(controls: state.footer.controls) { event in
        state = PreviewStates.viewer(state, applying: event)
      }
    }
  }
#endif
