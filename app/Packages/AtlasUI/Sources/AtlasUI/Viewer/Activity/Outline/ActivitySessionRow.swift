import SwiftUI

/// A session's row, 44 high: its disclosure, the agent's icon at 24 with a blue dot while it works, the agent in
/// semibold and the project in grey, then a one-line summary with its problems in orange, and its time on the first
/// line. Both lines tail-truncate.
struct ActivitySessionRow: View {
  /// The session.
  let session: ViewerOutlineSession
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The row, which opens or folds the session.
  var body: some View {
    Button {
      withAnimation(ActivityMotion.fold(reduceMotion: reduceMotion)) {
        send(.toggleSession(session.id, opens: !session.isOpen))
      }
    } label: {
      label
    }
    .buttonStyle(OutlineRowStyle(height: ViewerLayout.sessionRowHeight))
    .layoutFrame("session \(session.id)")
    .accessibilityElement(children: .combine)
    .accessibilityLabel(session.label)
    .accessibilityValue(session.isOpen ? Text("Open") : Text("Folded"))
    .accessibilityHint(session.isOpen ? Text("Folds this session") : Text("Shows this session's runs"))
    .accessibilityAddTraits(.isHeader)
  }

  /// The disclosure, the icon, the two lines and the time.
  private var label: some View {
    HStack(spacing: .zero) {
      OutlineDisclosure(isOpen: session.isOpen)
      lead
        .frame(width: ViewerLayout.outlineLead)
        .padding(.leading, ViewerLayout.outlineDisclosureGap)
        .padding(.trailing, ViewerLayout.outlineLeadGap)
      HStack(alignment: .firstTextBaseline, spacing: ViewerLayout.outlineTimeGap) {
        VStack(alignment: .leading, spacing: ViewerLayout.sessionLineGap) {
          who
          summary
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        OutlineTime(time: session.time)
      }
    }
    .lineLimit(1)
  }

  /// The agent's icon, with a blue dot while it works; the person's own mark for their own work.
  @ViewBuilder private var lead: some View {
    if let icon = session.icon {
      SidebarBadgedIcon(icon: icon, badge: session.isWorking ? .acting : nil, ground: Color.fillContent)
    } else {
      ActivityGlyph(icon: .you, side: ViewerLayout.outlineLead)
    }
  }

  /// The agent in semibold, then the project in grey.
  private var who: some View {
    let agent = Text(verbatim: session.agent)
      .font(Typography.font(.body, weight: .semibold))
      .foregroundStyle(Color.textPrimary)
    let project = Text(verbatim: session.project.isEmpty ? "" : " · \(session.project)")
      .font(Typography.font(.body))
      .foregroundStyle(Color.textSecondary)
    return Text("\(agent)\(project)")
      .truncationMode(.tail)
  }

  /// The summary in grey, then its problems in orange.
  private var summary: some View {
    let said = Text(verbatim: session.summary)
      .foregroundStyle(Color.textSecondary)
    let problems = Text(verbatim: session.problems.isEmpty ? "" : " · \(session.problems)")
      .foregroundStyle(Color.textAttention)
    return Text("\(said)\(problems)")
      .font(Typography.digits(.callout))
      .truncationMode(.tail)
  }
}
