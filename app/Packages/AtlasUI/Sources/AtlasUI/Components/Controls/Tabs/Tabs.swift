import SwiftUI

/// One row of tabs (v3 U-10, V1, G1, X3). As a `track`, the tabs sit side by side in a capsule track, each its word or
/// its symbol, the selected one on a brighter thumb that glides to it (`matchedGeometryEffect(id: "thumb")`,
/// ``Motion/tab``; under Reduce Motion it moves in 0.15 seconds, ``Motion/crossfadeReduced``). As a `segment` they
/// sit the same way in a small rounded track, as a pane's filters. As `symbolOverWord`, each tab is its symbol over
/// its word, the selected one on a filled tile with its symbol in blue, as a settings window's. Each tab is a button
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
    case .segment: segment
    case .symbolOverWord: tiles
    }
  }

  /// The tabs in their capsule track.
  private var track: some View {
    HStack(spacing: TabsLayout.trackGap) {
      ForEach(state.tabs) { tab in
        Button {
          send(tab.id)
        } label: {
          trackLabel(tab)
            .modifier(hanging(tab.id, gap: face == .symbol ? Spacing.xs : Spacing.xsPlus))
        }
        .buttonStyle(TrackTabStyle(isSelected: tab.id == state.selected, face: face, thumb: thumb))
        .help(face == .symbol ? tab.title : "")
        .accessibilityLabel(tab.title)
        .accessibilityAddTraits(tab.id == state.selected ? .isSelected : [])
      }
    }
    .padding(TabsLayout.trackInset)
    .background(
      Capsule()
        .fill(Color.fillControl)
    )
    .overlay(
      RoundedRectangle(cornerRadius: TabsLayout.trackHeight / 2, style: .circular)
        .strokeBorder(Color.separator, lineWidth: Stroke.hairline)
    )
    .fixedSize()
    .animation(
      Motion.animation(Motion.tab, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced),
      value: state.selected
    )
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

  /// The tabs in a small rounded track, each its word, as a pane's filters (v3 `.seg`).
  private var segment: some View {
    HStack(spacing: .zero) {
      ForEach(state.tabs) { tab in
        Button {
          send(tab.id)
        } label: {
          Text(tab.title)
            .modifier(hanging(tab.id, gap: Spacing.xs))
        }
        .buttonStyle(SegmentTabStyle(isSelected: tab.id == state.selected, thumb: thumb))
        .accessibilityLabel(tab.title)
        .accessibilityAddTraits(tab.id == state.selected ? .isSelected : [])
      }
    }
    .padding(Spacing.half)
    .background(
      RoundedRectangle(cornerRadius: Radius.small)
        .fill(Color.fillControl)
    )
    .fixedSize()
    .animation(
      Motion.animation(Motion.tab, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced),
      value: state.selected
    )
    .accessibilityElement(children: .contain)
    .accessibilityLabel(state.label)
  }

  /// The tabs as tiles, each its symbol over its word.
  private var tiles: some View {
    HStack(spacing: Spacing.xs) {
      ForEach(state.tabs) { tab in
        Button {
          send(tab.id)
        } label: {
          VStack(spacing: TabsLayout.tileGap) {
            if let symbol = tab.symbol {
              Image(systemName: symbol.rawValue)
                .font(Typography.symbol(.tab))
                .foregroundStyle(tab.id == state.selected ? Color.accent : Color.textSecondary)
                .frame(height: SymbolScale.tab.pointSize)
            }
            Text(tab.title)
              .font(Typography.font(.caption, weight: .medium))
              .modifier(hanging(tab.id, gap: Spacing.xs))
          }
        }
        .buttonStyle(TileTabStyle(isSelected: tab.id == state.selected))
        .accessibilityLabel(tab.title)
        .accessibilityAddTraits(tab.id == state.selected ? .isSelected : [])
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel(state.label)
  }

  /// What hangs `tab`'s accessory `gap` after the trailing edge of its label, outside the label's frame: the label stays
  /// centred in its tab, the thumb sits on the tab whole, and the gaps between the words stay even, whether or not the
  /// tab carries anything (an Activity dot that is not showing included).
  ///
  /// - Parameters:
  ///   - tab: The tab.
  ///   - gap: From the label's trailing edge to the accessory.
  /// - Returns: The modifier.
  private func hanging(_ tab: Tab, gap: CGFloat) -> TabsAccessory<Accessory> {
    TabsAccessory(accessory: accessory(tab), gap: gap)
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
