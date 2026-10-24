import SwiftUI

/// The transient local results presentation replaces page content without becoming a history destination.
struct SettingsSearchResults: View {
  /// The current query and ranked results.
  let state: SettingsSearchState
  /// Receives local selection and navigation.
  let send: @MainActor (SettingsEvent) -> Void

  /// Lazy rows keep loaded project lists responsive, without animating filtering or row layout.
  var body: some View {
    ScrollViewReader { scroll in
      ScrollView {
        VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
          if state.results.isEmpty {
            Text("No settings found")
              .textStyle(.heading)
              .foregroundStyle(Color.textPrimary)
            Text(verbatim: state.query)
              .textStyle(.body)
              .foregroundStyle(Color.textSecondary)
              .fixedSize(horizontal: false, vertical: true)
            Button("Clear Search") {
              send(.search(""))
              send(.find)
            }
            .buttonStyle(.capsule(.glass, size: .small))
          } else {
            LazyVStack(spacing: .zero) {
              ForEach(state.results) { entry in
                SettingsSearchRow(
                  entry: entry,
                  selected: state.selection == entry.id,
                  select: { send(.selectSearchResult(entry.id)) }
                ) {
                  send(.openSearchResult(entry.id))
                }
                .id(entry.id)
                .overlay(alignment: .bottom) {
                  if entry.id != state.results.last?.id {
                    SeparatorLine()
                      .padding(.horizontal, Spacing.m)
                      .accessibilityHidden(true)
                      .allowsHitTesting(false)
                  }
                }
              }
            }
            .background(Color.fillGroup)
            .clipShape(RoundedRectangle(cornerRadius: Radius.small))
          }
        }
        .frame(maxWidth: SettingsLayout.contentMaximumWidth, alignment: .leading)
        .frame(maxWidth: .infinity, alignment: .top)
        .padding(SettingsLayout.swapInsets)
        .transaction { $0.animation = nil }
      }
      .scrollBounceBehavior(.basedOnSize)
      .onChange(of: state.selection) { _, selection in
        if let selection { scroll.scrollTo(selection) }
      }
    }
    .onKeyPress(.upArrow) {
      send(.moveSearchSelection(-1))
      return .handled
    }
    .onKeyPress(.downArrow) {
      send(.moveSearchSelection(1))
      return .handled
    }
    .onKeyPress(.return) {
      send(.openSelectedSearchResult)
      return .handled
    }
    .onKeyPress(.escape) {
      send(.search(""))
      send(.find)
      return .handled
    }
  }
}
