import AppKit
import AtlasKit
import SwiftUI

/// The helper, what happened, and whether all is ready, as cards of rows.
struct Sidebar: View {
    @Bindable var model: ViewerModel

    var body: some View {
        let overview = model.overview
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                VStack(alignment: .leading, spacing: 4) {
                    Eyebrow(text: "Helper")
                    Text(model.account).font(.system(size: 20, weight: .semibold)).tracking(-0.3)
                    Text(overview.reason(holdingControl: model.holdingControl))
                        .font(.system(size: 12.5))
                        .foregroundStyle(AtlasStyle.ink2)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
                SideSection(title: "Recent") {
                    let items = overview.activity()
                    if items.isEmpty {
                        Card {
                            Text("Nothing yet. Actions appear here as they happen.")
                                .font(.system(size: 12.5)).foregroundStyle(AtlasStyle.ink3)
                                .padding(14)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    } else {
                        Card {
                            ForEach(items) { item in
                                if item.id != items.first?.id { Rule() }
                                ActivityRow(item: item)
                            }
                        }
                    }
                }
                SideSection(title: "Ready") {
                    Card {
                        let rows = overview.statusRows
                        ForEach(rows) { row in
                            if row.id != rows.first?.id { Rule() }
                            ReadyRow(row: row)
                        }
                    }
                }
                SideSection(title: "Session") {
                    Card {
                        let facts = overview.facts(holdingControl: model.holdingControl)
                        ForEach(facts, id: \.0) { fact in
                            if fact.0 != facts.first?.0 { Rule() }
                            SideFact(name: fact.0, value: fact.1)
                        }
                    }
                }
                SideSection(title: "Picture") {
                    Card {
                        HStack {
                            Text("Refresh").font(.system(size: 13)).foregroundStyle(AtlasStyle.ink2)
                            Spacer()
                            Picker("Refresh", selection: $model.rate) {
                                Text("Every second").tag(1.0)
                                Text("Every 2 seconds").tag(2.0)
                                Text("Every 5 seconds").tag(5.0)
                            }
                            .labelsHidden()
                            .fixedSize()
                        }
                        .padding(.horizontal, 14)
                        .frame(height: 44)
                    }
                    Note(text: pictureNote)
                        .multilineTextAlignment(.leading)
                        .padding(.horizontal, 4)
                }
            }
            .padding(20)
        }
        .scrollIndicators(.automatic)
        .frame(width: 300)
        .frame(maxHeight: .infinity, alignment: .top)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Sidebar")
    }

    /// The picture's size in pixels, which a Retina picture's points are not.
    private var pictureNote: String {
        let memory = "Pictures of the helper stay in memory and are never saved."
        guard let pixels = model.image?.representations.first, pixels.pixelsWide > 0 else { return memory }
        return "\(pixels.pixelsWide) × \(pixels.pixelsHigh) pixels. \(memory)"
    }
}

/// An eyebrow over its card.
struct SideSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Eyebrow(text: title).padding(.horizontal, 4).accessibilityAddTraits(.isHeader)
            content
        }
    }
}

/// Who did what, and when.
struct ActivityRow: View {
    let item: ActivityItem

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                Circle().fill(
                    item.ok ? (item.who == "You" ? AtlasStyle.blue.opacity(0.2) : AtlasStyle.fill) : AtlasStyle.orange)
                Text(item.ok ? String(item.who.prefix(1)) : "!")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(
                        item.ok ? (item.who == "You" ? AtlasStyle.blue : AtlasStyle.ink) : AtlasStyle.ground)
            }
            .frame(width: 24, height: 24)
            .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.line).font(.system(size: 13))
                    .foregroundStyle(item.ok ? AtlasStyle.ink : AtlasStyle.orange)
                    .fixedSize(horizontal: false, vertical: true)
                Text(item.when).font(.system(size: 11.5)).foregroundStyle(AtlasStyle.ink3).monospacedDigit()
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .accessibilityElement(children: .combine)
    }
}

/// One readiness check, as the onboarding's checklist shows it: the mark
/// says how it stands, and the name has the rest of the row.
struct ReadyRow: View {
    let row: StatusRow

    var body: some View {
        HStack(alignment: row.state == .ready ? .center : .top, spacing: 12) {
            switch row.state {
            case .ready: Check(size: 18)
            case .bad:
                ZStack {
                    Circle().fill(AtlasStyle.orange)
                    Image(systemName: "exclamationmark").font(.system(size: 9, weight: .heavy))
                        .foregroundStyle(AtlasStyle.ground)
                }
                .frame(width: 18, height: 18)
                .accessibilityHidden(true)
            case .unknown:
                Circle().strokeBorder(AtlasStyle.lineStrong, lineWidth: 1.5).frame(width: 18, height: 18)
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(row.name).font(.system(size: 13))
                    .fixedSize(horizontal: false, vertical: true)
                if row.state != .ready {
                    Text(row.text).font(.system(size: 11.5))
                        .foregroundStyle(row.state == .bad ? AtlasStyle.orange : AtlasStyle.ink2)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .accessibilityElement(children: .combine)
        .accessibilityValue(row.state == .ready ? "Ready" : row.state == .bad ? "Not ready" : "Unknown")
    }
}

/// A name and its value, the value on the right.
struct SideFact: View {
    let name: String
    let value: String

    var body: some View {
        HStack(spacing: 12) {
            Text(name).foregroundStyle(AtlasStyle.ink2)
            Spacer(minLength: 8)
            Text(value).lineLimit(1).truncationMode(.tail).help(value)
        }
        .font(.system(size: 13))
        .padding(.horizontal, 14)
        .frame(height: 44)
        .accessibilityElement(children: .combine)
    }
}
