import SwiftUI

/// The sidebar beside the card, and the toggle over them in the window's space, as the sidebar stands. It keeps its
/// own drawn copy of the sidebar's mode, so each change moves as one: hiding or showing, the sidebar's edge, the
/// card's leading edge and the header's words travel together on ``Motion/slide``, the sidebar's content laid out at
/// its width throughout and cut off by its narrowing edge, never laid out again, while the toggle stays where it
/// stands; pressed again mid-way, every spring turns back from where it is. Under Reduce Motion nothing slides: hiding
/// fades the sidebar's content out before the card takes its room at once, and showing makes room at once and fades it
/// in; a newer change calls off a pending fade. While the window is too narrow for the sidebar, it hides, and the
/// toggle or Find shows it over the card instead (``SidebarOverlay``), sliding in from the window's edge on the same
/// spring, or fading under Reduce Motion, with the toggle over it; a click on the card puts it away. The card is told
/// how the sidebar stands as it moves (``SidebarSlide``), so its lines of words keep their room while its edge
/// travels.
struct SidebarSplit<Card: View>: View {
  /// The sidebar, whose mode is the one to settle on.
  private let sidebar: ViewerSidebarState
  /// Whether the window is in full screen, which hides the window buttons.
  private let isFullScreen: Bool
  /// Receives what the person does.
  private let send: @MainActor (ViewerEvent) -> Void
  /// The card beside the sidebar, given how the sidebar stands as it moves: the mode drawn now, which its header's
  /// inset follows, the mode its lists are laid out for, which they take at once, and the mode it last rested in.
  private let card: (SidebarSlide) -> Card
  /// The mode drawn now, which follows the sidebar's on its animation.
  @State private var drawn: ViewerSidebarMode
  /// The mode the sidebar last came to rest in, once its slide settled.
  @State private var rested: ViewerSidebarMode
  /// Whether its content shows, which only Reduce Motion fades.
  @State private var showsContent: Bool
  /// Whether the sidebar shows over the card, which follows the sidebar's on its animation.
  @State private var showsOverlay: Bool
  /// Each change calls off what an earlier one left to finish.
  @State private var generation = 0
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates the split.
  ///
  /// - Parameters:
  ///   - sidebar: The sidebar.
  ///   - isFullScreen: Whether the window is in full screen.
  ///   - send: Receives what the person does.
  ///   - card: The card, given how the sidebar stands as it moves.
  init(
    sidebar: ViewerSidebarState,
    isFullScreen: Bool,
    send: @escaping @MainActor (ViewerEvent) -> Void,
    @ViewBuilder card: @escaping (SidebarSlide) -> Card
  ) {
    self.sidebar = sidebar
    self.isFullScreen = isFullScreen
    self.send = send
    self.card = card
    _drawn = State(initialValue: sidebar.mode)
    _rested = State(initialValue: sidebar.mode)
    _showsContent = State(initialValue: sidebar.mode != .hidden)
    _showsOverlay = State(initialValue: sidebar.overlay)
  }

  /// The sidebar, cut to its room, beside the card, the sidebar over the card when shown there, and the toggle over
  /// all of it.
  var body: some View {
    HStack(spacing: .zero) {
      ViewerSidebar(sidebar: beside, isFullScreen: isFullScreen, send: send)
        .frame(width: ViewerLayout.sidebarWidth)
        .opacity(showsContent ? 1 : 0)
        .frame(width: ViewerLayout.sidebarRoom(drawn), alignment: .leading)
        .clipped()
        .disabled(sidebar.mode == .hidden || sidebar.overlay || drawn == .hidden)
        .allowsHitTesting(drawn != .hidden)
        .accessibilityHidden(drawn == .hidden)
      card(SidebarSlide(drawn: drawn, settling: reduceMotion ? drawn : sidebar.mode, rested: rested))
        .padding(.leading, ViewerLayout.cardLeading(drawn))
    }
    .overlay {
      if showsOverlay {
        Color.clear
          .contentShape(Rectangle())
          .onTapGesture { send(.dismissSidebarOverlay) }
          .accessibilityHidden(true)
      }
    }
    .overlay(alignment: .leading) {
      if showsOverlay {
        SidebarOverlay(sidebar: sidebar, isFullScreen: isFullScreen, send: send)
          .disabled(!sidebar.overlay)
          .transition(Motion.transition(.move(edge: .leading), reduceMotion: reduceMotion))
      }
    }
    .overlay(alignment: .topLeading) {
      ViewerTitleButtons(mode: showsOverlay ? .full : drawn, isFullScreen: isFullScreen, send: send)
    }
    .onChange(of: sidebar.mode) { _, mode in shift(to: mode) }
    .onChange(of: sidebar.overlay) { _, shows in
      withAnimation(ViewerLayout.sidebarMotion(reduceMotion: reduceMotion)) {
        showsOverlay = shows
      }
    }
  }

  /// The sidebar beside the card, with search closed whenever it is hidden or shown over the card.
  /// Only the visible search field may hold keyboard focus.
  private var beside: ViewerSidebarState {
    guard sidebar.overlay || sidebar.mode == .hidden || drawn == .hidden else { return sidebar }
    var beside = sidebar
    beside.search.isOpen = false
    return beside
  }

  /// Moves the sidebar to `mode`: sliding, or under Reduce Motion fading (``shiftAtOnce(to:generation:)``).
  ///
  /// - Parameter mode: The mode to settle on.
  private func shift(to mode: ViewerSidebarMode) {
    generation += 1
    let current = generation
    guard !reduceMotion else { return shiftAtOnce(to: mode, generation: current) }
    showsContent = true
    withAnimation(ViewerLayout.sidebarMotion(reduceMotion: false)) {
      drawn = mode
    } completion: {
      // Only once the sidebar has come to rest, and only if no newer change came meanwhile.
      guard generation == current else { return }
      // The card's lines take their new room at once, their words cut short anew where they stand.
      var still = Transaction()
      still.disablesAnimations = true
      withTransaction(still) { rested = mode }
    }
  }

  /// Moves the sidebar to `mode` with nothing sliding: hidden, its content fades out first and the card then takes
  /// its room at once; shown, it takes its room at once and its content fades in.
  ///
  /// - Parameters:
  ///   - mode: The mode to settle on.
  ///   - current: The change's generation, which a newer change outdates.
  private func shiftAtOnce(to mode: ViewerSidebarMode, generation current: Int) {
    if mode == .hidden {
      withAnimation(ViewerLayout.sidebarMotion(reduceMotion: true)) {
        showsContent = false
      } completion: {
        guard generation == current else { return }
        settle(mode)
      }
      return
    }
    settle(mode)
    withAnimation(ViewerLayout.sidebarMotion(reduceMotion: true)) { showsContent = true }
  }

  /// Draws `mode` at once, with no animation.
  ///
  /// - Parameter mode: The mode.
  private func settle(_ mode: ViewerSidebarMode) {
    var still = Transaction()
    still.disablesAnimations = true
    withTransaction(still) {
      drawn = mode
      rested = mode
    }
  }
}

#if DEBUG
  #Preview("Motion") {
    SidebarMotionSample()
  }
#endif
