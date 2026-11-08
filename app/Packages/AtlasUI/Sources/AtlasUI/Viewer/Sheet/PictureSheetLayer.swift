import SwiftUI

/// The picture sheet over the window: the window dims behind it, and the sheet comes in where it stands, growing from
/// a touch smaller as it fades in on ``Motion/present``, and goes the same way back a little quicker on
/// ``Motion/close``, with nothing travelling. Asked for again as it goes, or closed as it comes, it turns back from
/// where it is; the scrim and the window behind dim and come back on the same animation. Under Reduce Motion it only
/// fades. Pressing the dimmed window, Done, Return or Escape closes it; what it covers takes the keyboard back once it
/// has gone.
struct PictureSheetLayer: View {
  /// The sheet; nil shows none.
  let sheet: ViewerPictureSheet?
  /// Whether the sheet stands over the window, so what is behind it takes no presses, no keyboard and no VoiceOver:
  /// from its coming, on its animation, so the window behind dims with the scrim, until its going has finished, when
  /// the window behind comes back on the same spring and what opened the sheet takes the keyboard back.
  @Binding var covers: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The sheet last shown, drawn while it goes; nil once it has gone.
  @State private var leaving: ViewerPictureSheet?
  /// How far the sheet has come in, from 0 gone to 1 in place.
  @State private var presence: CGFloat
  /// Each opening or closing calls off what an earlier one left to finish.
  @State private var generation = 0
  @Environment(\.motionReduced) private var reduceMotion

  /// Creates the layer.
  ///
  /// - Parameters:
  ///   - sheet: The sheet; nil shows none.
  ///   - covers: Whether the sheet stands over the window, until its closing has finished.
  ///   - send: Receives what the person does.
  init(
    sheet: ViewerPictureSheet?,
    covers: Binding<Bool>,
    send: @escaping @MainActor (ViewerEvent) -> Void
  ) {
    self.sheet = sheet
    _covers = covers
    self.send = send
    _presence = State(initialValue: sheet == nil ? .zero : 1)
  }

  /// The scrim and the sheet.
  var body: some View {
    GeometryReader { proxy in
      ZStack(alignment: .top) {
        // The sheet as it is now while it shows, so every change to it draws at once; as it goes, the last one shown.
        if let shown = sheet ?? leaving {
          Color.scrim
            .opacity(presence)
            .contentShape(Rectangle())
            .onTapGesture { send(.closePicture) }
            .accessibilityHidden(true)
          PictureSheetCard(sheet: shown, window: proxy.size, send: send)
            .layoutFrame("picture sheet")
            .scaleEffect(scale)
            .opacity(presence)
            .padding(.top, ViewerLayout.sheetTop)
            // A sheet on its way out takes no presses and is not read.
            .disabled(sheet == nil)
            .accessibilityHidden(sheet == nil)
        }
      }
      .frame(width: proxy.size.width, height: proxy.size.height, alignment: .top)
    }
    .onChange(of: sheet == nil) { _, gone in
      if gone { close() } else { open() }
    }
    .onChange(of: sheet) { last, now in
      // Kept to draw as it goes.
      if now == nil { leaving = last }
    }
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(sheet == nil ? [] : .isModal)
  }

  /// The sheet's scale as it comes and goes: from ``ViewerLayout/sheetEnterScale`` to whole, and whole throughout
  /// under Reduce Motion.
  private var scale: CGFloat {
    guard !reduceMotion else { return 1 }
    return ViewerLayout.sheetEnterScale + (1 - ViewerLayout.sheetEnterScale) * presence
  }

  /// How the sheet comes in or goes: ``Motion/present`` in, ``Motion/close`` out, a little quicker; a short fade
  /// either way under Reduce Motion.
  ///
  /// - Parameters:
  ///   - opening: Whether it comes in, rather than goes.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  static func motion(opening: Bool, reduceMotion: Bool) -> Animation {
    guard !reduceMotion else { return Motion.crossfadeReduced }
    return opening ? Motion.present : Motion.close
  }

  /// Brings the sheet in from where it stands, the window behind dimming with it.
  private func open() {
    generation += 1
    withAnimation(Self.motion(opening: true, reduceMotion: reduceMotion)) {
      presence = 1
      covers = true
    }
  }

  /// Takes the sheet away from where it stands, and once it has gone, gives the window behind back on the same spring,
  /// so its controls brighten as one fade rather than at once, and what opened the sheet takes the keyboard back.
  private func close() {
    generation += 1
    let current = generation
    let motion = Self.motion(opening: false, reduceMotion: reduceMotion)
    withAnimation(motion) {
      presence = .zero
    } completion: {
      // Only if nothing asked for the sheet again meanwhile.
      guard generation == current else { return }
      leaving = nil
      withAnimation(motion) { covers = false }
    }
  }
}
