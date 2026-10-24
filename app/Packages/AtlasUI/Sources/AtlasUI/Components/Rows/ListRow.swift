import SwiftUI

/// The one row every list draws (v3 D6, U-6): something leading, a medium title over a secondary detail, and
/// something trailing, at the height its window uses. In a ``GroupedList`` it draws the separator above it, inset to
/// where its title starts. A row with a radio is a choice: the whole row picks it, the radio is its only mark (v3 D7),
/// and the row's fill answers the pointer.
struct ListRow: View {
  /// What leads the row.
  enum Lead: Equatable, Sendable {
    /// Nothing: the text starts at the row's inset.
    case none
    /// A small tile with a symbol or characters, such as a permission's or an agent's.
    case tile(TileIcon)
    /// The helper account's picture in a circle.
    case picture
    /// An account's first letter on an avatar disc.
    case letter(String)
    /// A check's mark: a spinner, a green check or an orange round "!".
    case check(CheckState)
  }

  /// What ends the row.
  enum Trailing: Equatable, Sendable {
    /// Nothing.
    case none
    /// A word in a tone, such as a quiet Off or a window's size, or a failed check's note in orange.
    case text(String, tone: NoteTone = .quiet)
    /// A mark and a word in its colour, such as a green check and Allowed.
    case check(CheckState, String)
    /// A switch, which calls the row's action when flipped.
    case toggle(isOn: Bool)
    /// A glass button with its title, which calls the row's action.
    case button(String)
    /// A radio: the row is a choice, and the whole row calls its action.
    case radio(isOn: Bool)
    /// A spinner while something runs, which waits before it shows.
    case progress
  }

  /// How a trailing word is painted.
  enum NoteTone: Equatable, Sendable {
    /// Tertiary: a remark, such as Off.
    case quiet
    /// The attention text colour: a failed check's note (v3 `.row.check.failed .trail`).
    case problem

    /// The word's colour.
    var color: Color {
      switch self {
      case .quiet: .textTertiary
      case .problem: .textAttention
      }
    }
  }

  /// How tall the row is, by where it sits (v3 D6).
  enum Size: Sendable, CaseIterable {
    /// 48: a setup list.
    case regular
    /// 44: a Settings group.
    case compact
    /// 38: Activity.
    case dense
    /// 34: a window under its app.
    case tight

    /// The space above and below the row's content: v3's 10 on a setup row; the tighter rows keep 4, so their least
    /// height sets their rhythm.
    var verticalInset: CGFloat {
      self == .regular ? Spacing.sPlus : Spacing.xs
    }

    /// The row's least height.
    var height: CGFloat {
      switch self {
      case .regular: Layout.regularHeight
      case .compact: ControlHeight.xl
      case .dense: Layout.denseHeight
      case .tight: ControlHeight.mPlus
      }
    }
  }

  /// The title.
  private let title: String
  /// The detail under it, or nil for none.
  private let detail: String?
  /// What leads the row.
  private let lead: Lead
  /// What ends the row.
  private let trailing: Trailing
  /// How tall the row is.
  private let size: Size
  /// What the row's control does: flips its switch, presses its button or picks its choice.
  private let action: (@MainActor () -> Void)?
  @State private var isHovered = false
  @Environment(\.drawsListSeparator) private var drawsSeparator

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - title: The title.
  ///   - detail: The detail under it, or nil for none.
  ///   - lead: What leads the row; nothing by default.
  ///   - trailing: What ends the row; nothing by default.
  ///   - size: How tall the row is; a setup list's by default.
  ///   - action: What its switch, button or radio does, or nil for a row that only shows.
  init(
    _ title: String,
    detail: String? = nil,
    lead: Lead = .none,
    trailing: Trailing = .none,
    size: Size = .regular,
    action: (@MainActor () -> Void)? = nil
  ) {
    self.title = title
    self.detail = detail
    self.lead = lead
    self.trailing = trailing
    self.size = size
    self.action = action
  }

  /// The row; a choice is a plain button around it, so Tab, Full Keyboard Access and VoiceOver can pick it.
  var body: some View {
    if isChoice {
      Button {
        action?()
      } label: {
        content
      }
      .buttonStyle(.plain)
      .accessibilityAddTraits(trailing == .radio(isOn: true) ? .isSelected : [])
    } else {
      content
    }
  }

  /// The lead, the title over its detail, and the trailing control, with the separator above.
  private var content: some View {
    HStack(spacing: Layout.gap) {
      ListRowLead(lead: lead)
      VStack(alignment: .leading, spacing: Layout.lineGap) {
        Text(title)
          .textStyle(.body, weight: Self.titleWeight(lead))
          .foregroundStyle(Color.textPrimary)
          .lineLimit(2)
          .truncationMode(.middle)
        if let detail {
          Text(detail)
            .textStyle(.callout)
            .foregroundStyle(Color.textSecondary)
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      ListRowTrailing(trailing: trailing, action: action)
    }
    .padding(.horizontal, Layout.inset)
    .padding(.vertical, size.verticalInset)
    .frame(minHeight: size.height)
    .background(rowFill)
    .overlay(alignment: .top) {
      if drawsSeparator {
        SeparatorLine()
          .padding(.leading, Self.separatorInset(lead))
      }
    }
    .contentShape(Rectangle())
    .onHover { isHovered = $0 }
    .animation(Motion.hover(isHovered), value: isHovered)
    .accessibilityElement(children: accessibilityChildren)
  }

  /// How VoiceOver reads the row: as one element, except where it ends in a switch or a button, which stays its own
  /// element beside the row's words so it can be operated.
  private var accessibilityChildren: AccessibilityChildBehavior {
    switch trailing {
    case .toggle, .button: .contain
    case .none, .text, .check, .radio, .progress: .combine
    }
  }

  /// The row's fill: the hover fill while the pointer is over a choice, none otherwise.
  private var rowFill: Color {
    isHovered && isChoice ? .fillRowHover : .clear
  }

  /// Whether the whole row is a choice: it ends in a radio and has an action.
  private var isChoice: Bool {
    guard action != nil, case .radio = trailing else { return false }
    return true
  }

  /// Where the separator above a row starts: where its title starts, past its inset, its lead and the gap after it.
  ///
  /// - Parameter lead: What leads the row.
  /// - Returns: The separator's inset from the row's leading edge: 14 with nothing leading, 42 after a check's mark,
  ///   56 after a tile, a picture or a letter (v3's `--sep`).
  nonisolated static func separatorInset(_ lead: Lead) -> CGFloat {
    let width = leadWidth(lead)
    return width == 0 ? Layout.inset : Layout.inset + width + Layout.gap
  }

  /// How wide a row's lead is.
  ///
  /// - Parameter lead: What leads the row.
  /// - Returns: None for nothing, 16 for a check's mark, 30 for a tile, a picture or a letter.
  nonisolated static func leadWidth(_ lead: Lead) -> CGFloat {
    switch lead {
    case .none: 0
    case .check: IconSize.s
    case .tile, .picture, .letter: IconSize.l
    }
  }

  /// The title's weight: medium, so a list reads as content (v3 D1), but regular in a checklist, where the mark says
  /// how the line stands.
  ///
  /// - Parameter lead: What leads the row.
  /// - Returns: The weight.
  nonisolated static func titleWeight(_ lead: Lead) -> Font.Weight {
    if case .check = lead { return .regular }
    return .medium
  }
}

/// A row's own measures, from v3's `.row`.
private enum Layout {
  /// 14: the row's inset on each side.
  static let inset = Spacing.mPlus
  /// 12: between the lead, the text and the trailing control.
  static let gap = Spacing.m
  /// 1: between the title and its detail.
  static let lineGap = Spacing.hairline
  /// 48: a setup list's row (v3 D6).
  static let regularHeight: CGFloat = 48
  /// 38: an Activity row (v3 D6, V8).
  static let denseHeight: CGFloat = 38
}

#if DEBUG
  #Preview("Leads") {
    ListRowSamples(.leads)
  }

  #Preview("Trailing") {
    ListRowSamples(.trailing)
  }

  #Preview("Sizes") {
    ListRowSamples(.sizes)
  }

  #Preview("Interactive") {
    @Previewable @State var picked = 0
    @Previewable @State var isOn = true
    InteractivePreview {
      GroupedList(ListRowSamples.accounts) { account in
        ListRow(
          account.title,
          detail: account.detail,
          lead: account.lead,
          trailing: .radio(isOn: picked == account.id)
        ) { picked = account.id }
      }
      .frame(width: SampleWidth.column)
    } content: {
      GroupedList(ListRowSamples.settings) { setting in
        ListRow(setting.title, detail: setting.detail, trailing: .toggle(isOn: isOn), size: .compact) {
          isOn.toggle()
        }
      }
      .frame(width: SampleWidth.column)
    }
  }
#endif
