import SwiftUI

/// One row of tabs (v3 U-10, V1, G1). As a `track`, the tabs sit side by side in a capsule track, each its word or its
/// symbol, the selected one on a brighter thumb that glides to it (`matchedGeometryEffect(id: "thumb")`, ``Motion/tab``;
/// under Reduce Motion it moves on the short ``Motion/reduced``). As `symbolOverWord`, each tab is its symbol over its
/// word, the selected one on a filled tile with its symbol in blue, as a settings window's. Each tab is a button
/// VoiceOver reads as selected or not; anything a tab carries beside its word, such as a dot for something new, comes
/// from `accessory`.
struct Tabs<Tab: Hashable & Sendable, Accessory: View>: View {
  /// How the tabs are drawn.
  typealias Style = TabsStyle
  /// What a tab in a track shows.
  typealias Face = TabsFace

  /// The tabs and which one is selected.
  private let state: CapsuleTabsState<Tab>
  /// How the tabs are drawn.
  private let style: Style
  /// What a tab in a track shows.
  private let face: Face
  /// Receives the tab the person picks.
  private let send: @MainActor (Tab) -> Void
  /// What a tab carries beside its word, by the tab.
  private let accessory: (Tab) -> Accessory
  @Namespace private var thumb
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates a row of tabs.
  ///
  /// - Parameters:
  ///   - state: The tabs and which one is selected.
  ///   - style: How the tabs are drawn; a track by default.
  ///   - face: What a tab in a track shows; its word by default.
  ///   - send: Receives the tab the person picks.
  ///   - accessory: What a tab carries beside its word, by the tab.
  init(
    state: CapsuleTabsState<Tab>,
    style: Style = .track,
    face: Face = .word,
    send: @escaping @MainActor (Tab) -> Void,
    @ViewBuilder accessory: @escaping (Tab) -> Accessory
  ) {
    self.state = state
    self.style = style
    self.face = face
    self.send = send
    self.accessory = accessory
  }

  /// The tabs in their track, or as tiles.
  var body: some View {
    switch style {
    case .track: track
    case .symbolOverWord: tiles
    }
  }

  /// The tabs in their capsule track.
  private var track: some View {
    HStack(spacing: Layout.trackGap) {
      ForEach(state.tabs) { tab in
        Button {
          send(tab.id)
        } label: {
          HStack(spacing: Spacing.xsPlus) {
            trackLabel(tab)
            accessory(tab.id)
          }
        }
        .buttonStyle(TrackTabStyle(isSelected: tab.id == state.selected, face: face, thumb: thumb))
        .help(face == .symbol ? tab.title : "")
        .accessibilityLabel(tab.title)
        .accessibilityAddTraits(tab.id == state.selected ? .isSelected : [])
      }
    }
    .padding(Layout.trackInset)
    .background(
      Capsule()
        .fill(Color.fillControl)
    )
    .overlay(
      RoundedRectangle(cornerRadius: Layout.trackHeight / 2, style: .circular)
        .strokeBorder(Color.separator, lineWidth: Stroke.hairline)
    )
    .fixedSize()
    .animation(Motion.animation(Motion.tab, reduceMotion: reduceMotion, reduced: Motion.reduced), value: state.selected)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(state.label)
  }

  /// A track tab's word, or its symbol.
  ///
  /// - Parameter tab: The tab.
  /// - Returns: The label.
  @ViewBuilder private func trackLabel(_ tab: CapsuleTab<Tab>) -> some View {
    if face == .symbol, let symbol = tab.symbol {
      Image(systemName: symbol.rawValue)
        .font(Typography.symbol(.control, weight: .medium))
    } else {
      Text(tab.title)
    }
  }

  /// The tabs as tiles, each its symbol over its word.
  private var tiles: some View {
    HStack(spacing: Spacing.half) {
      ForEach(state.tabs) { tab in
        Button {
          send(tab.id)
        } label: {
          VStack(spacing: Spacing.half) {
            if let symbol = tab.symbol {
              Image(systemName: symbol.rawValue)
                .font(Typography.symbol(.tab))
                .foregroundStyle(tab.id == state.selected ? Color.textLink : Color.textSecondary)
            }
            HStack(spacing: Spacing.xs) {
              Text(tab.title)
                .font(Typography.font(.caption, weight: .medium))
              accessory(tab.id)
            }
          }
        }
        .buttonStyle(TileTabStyle(isSelected: tab.id == state.selected))
        .accessibilityAddTraits(tab.id == state.selected ? .isSelected : [])
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel(state.label)
  }
}

extension Tabs where Accessory == EmptyView {
  /// Creates a row of tabs that carry nothing beside their words.
  ///
  /// - Parameters:
  ///   - state: The tabs and which one is selected.
  ///   - style: How the tabs are drawn; a track by default.
  ///   - face: What a tab in a track shows; its word by default.
  ///   - send: Receives the tab the person picks.
  init(
    state: CapsuleTabsState<Tab>,
    style: Style = .track,
    face: Face = .word,
    send: @escaping @MainActor (Tab) -> Void
  ) {
    self.init(state: state, style: style, face: face, send: send) { _ in EmptyView() }
  }
}

/// A tab in the track: its label bright on the thumb while selected, quiet otherwise and brightening while held, which
/// also dips it a little (v3 X9: tabs 0.96).
private struct TrackTabStyle: ButtonStyle {
  /// Whether it is the selected tab.
  let isSelected: Bool
  /// What the tab shows: a symbol sits closer to its edges than a word.
  let face: TabsFace
  /// The namespace the thumb glides in.
  let thumb: Namespace.ID
  @Environment(\.isEnabled) private var enabled

  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(Typography.font(.body, weight: .medium))
      .foregroundStyle(isSelected || configuration.isPressed ? Color.textPrimary : Color.textSecondary)
      .lineLimit(1)
      .padding(.horizontal, face == .symbol ? Spacing.sPlus : Spacing.mPlus)
      .frame(height: Layout.trackHeight)
      .background {
        if isSelected {
          TabsThumb()
            .matchedGeometryEffect(id: Layout.thumbID, in: thumb)
        }
      }
      .contentShape(Capsule())
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.pressedScale))
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
  }
}

/// The selected track tab's thumb (v3 `.vtabs .thumb`): the held fill, a fine highlight along its top and a small
/// shadow under it.
private struct TabsThumb: View {
  var body: some View {
    Capsule()
      .fill(Color.fillControlOn)
      .overlay(alignment: .top) {
        RoundedRectangle(cornerRadius: Layout.trackHeight / 2, style: .circular)
          .strokeBorder(Color.strokeHighlight, lineWidth: Stroke.hairline)
          .mask(alignment: .top) {
            Rectangle()
              .frame(height: Stroke.hairline)
          }
      }
      .shadow(color: Color.dropShadowFaint, radius: Layout.thumbShadow, y: Layout.thumbDrop)
  }
}

/// A tab as a tile (v3 G1): its symbol over its word, both quiet; selected, the tile filled and the symbol blue.
private struct TileTabStyle: ButtonStyle {
  /// Whether it is the selected tab.
  let isSelected: Bool
  @Environment(\.isEnabled) private var enabled
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .foregroundStyle(isSelected ? Color.textPrimary : Color.textSecondary)
      .frame(width: Layout.tile.width, height: Layout.tile.height)
      .background(
        RoundedRectangle(cornerRadius: Radius.small)
          .fill(isSelected || configuration.isPressed ? Color.fillControlOn : Color.clear)
      )
      .contentShape(RoundedRectangle(cornerRadius: Radius.small))
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: configuration.isPressed)
  }
}

/// The tabs' own measures, from v3's track (`.vtabs`) and tiles (G1). At file scope because `Tabs` is generic.
private enum Layout {
  /// 28: a track tab's height, and the thumb's.
  static let trackHeight = ControlHeight.s
  /// 3: the track's inset around its tabs, so the track stands 34 high.
  static let trackInset: CGFloat = 3
  /// 2: between two track tabs.
  static let trackGap = Spacing.half
  /// 0.96: how far a held tab dips (v3 X9).
  static let pressedScale: CGFloat = 0.96
  /// 1.5: the thumb's shadow, half v3's 3-point blur.
  static let thumbShadow: CGFloat = 1.5
  /// 1: how far the thumb's shadow falls.
  static let thumbDrop: CGFloat = 1
  /// 74 by 50: a tile (v3 G1).
  static let tile = CGSize(width: 74, height: 50)
  /// The thumb's one identity, which it keeps as it glides from tab to tab.
  static let thumbID = "thumb"
}

#if DEBUG
  #Preview("Samples") {
    TabsSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var selected = TabsSamples.Pane.screen
    @Previewable @State var symbols = false
    InteractivePreview {
      Toggle(String("Symbols"), isOn: $symbols)
    } content: {
      Tabs(
        state: CapsuleTabsState(tabs: TabsSamples.tabs, selected: selected, label: "What you see"),
        face: symbols ? .symbol : .word
      ) { tab in
        selected = tab
      }
    }
  }
#endif
